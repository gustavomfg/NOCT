//! Review suggestions, checked against `SuggestionRepository.ts`. The row gate compares the
//! same reads and a reconciliation run against the TypeScript repository.

use noct_core::suggestions::{
    Suggestion, SuggestionCategory, SuggestionInput, SuggestionLevel, SuggestionSeverity,
    SuggestionStatus,
};
use noct_core::text::utf16_len;
use noct_db::Database;
use noct_db::repositories::suggestions::Suggestions;

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";

fn open(name: &str) -> Database {
    let db = Database::open_runtime(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) \
                 VALUES('/work/alpha','alpha','{AT}','{AT}');\
             INSERT INTO conversations(id,title,workspace,created_at,updated_at) \
                 VALUES('conv-1','Conversa','/work/alpha','{AT}','{AT}');"
        ))
        .expect("workspace and conversation");
    db
}

fn input(title: &str, severity: SuggestionSeverity) -> SuggestionInput {
    SuggestionInput {
        title: title.to_string(),
        description: "Descrição do problema.".to_string(),
        reasoning: "Motivo da conclusão.".to_string(),
        category: SuggestionCategory::Bug,
        severity,
        affected_files: vec!["src/a.ts".to_string()],
        proposed_changes: "Ajustar a leitura.".to_string(),
        expected_benefits: vec!["Menos falhas.".to_string()],
        complexity: SuggestionLevel::Low,
        risk: SuggestionLevel::Medium,
        evidence: None,
        confidence: None,
        source: None,
        responsible: None,
    }
}

fn set_updated_at(db: &Database, id: &str, at: &str) {
    db.connection()
        .execute("UPDATE suggestions SET updated_at=?1 WHERE id=?2", [at, id])
        .expect("order the suggestion");
}

#[test]
fn a_new_suggestion_gets_its_defaults_and_a_first_history_entry() {
    let db = open("suggestions-add");
    let suggestions = Suggestions::new(db.connection());

    let added = suggestions
        .add(
            "conv-1",
            "/work/alpha",
            &input("Revisar config", SuggestionSeverity::Medium),
        )
        .expect("add");

    assert_eq!(added.status, SuggestionStatus::New);
    assert_eq!(added.confidence, 60);
    assert_eq!(added.source, "Análise do agente");
    assert_eq!(added.responsible, "Agente de revisão");
    assert_eq!(added.history.len(), 1);
    assert_eq!(added.history[0].status, SuggestionStatus::New);
    assert_eq!(added.history[0].result, None);
    assert_eq!(
        suggestions.get(&added.id, Some("conv-1")).expect("get"),
        Some(added)
    );
}

#[test]
fn a_status_change_is_recorded_and_a_forbidden_one_is_refused() {
    let db = open("suggestions-transitions");
    let suggestions = Suggestions::new(db.connection());
    let added = suggestions
        .add(
            "conv-1",
            "/work/alpha",
            &input("Revisar config", SuggestionSeverity::Low),
        )
        .expect("add");

    let in_analysis = suggestions
        .set_status(&added.id, SuggestionStatus::InAnalysis, None)
        .expect("in analysis");
    assert_eq!(in_analysis.status, SuggestionStatus::InAnalysis);
    assert_eq!(in_analysis.history.len(), 2);

    let accepted = suggestions
        .set_status(
            &added.id,
            SuggestionStatus::Accepted,
            Some(&"r".repeat(20_005)),
        )
        .expect("accepted");
    let kept = accepted
        .history
        .last()
        .expect("entry")
        .result
        .clone()
        .expect("result");
    assert_eq!(utf16_len(&kept), 20_000, "the result keeps 20,000 units");

    let error = suggestions
        .set_status(&added.id, SuggestionStatus::New, None)
        .expect_err("accepted cannot return to new");
    assert_eq!(
        error.to_string(),
        "Transição de sugestão inválida: accepted → new."
    );

    let same = suggestions
        .set_status(&added.id, SuggestionStatus::Accepted, None)
        .expect("same status");
    assert_eq!(
        same.history.len(),
        3,
        "setting the current status records nothing"
    );
}

#[test]
fn a_missing_suggestion_is_reported_by_name() {
    let db = open("suggestions-missing");
    let error = Suggestions::new(db.connection())
        .set_status("missing", SuggestionStatus::Rejected, None)
        .expect_err("missing");

    assert_eq!(error.to_string(), "Sugestão não encontrada.");
}

#[test]
fn the_list_shows_open_suggestions_newest_first() {
    let db = open("suggestions-list");
    let suggestions = Suggestions::new(db.connection());
    let first = suggestions
        .add(
            "conv-1",
            "/work/alpha",
            &input("Primeira", SuggestionSeverity::Low),
        )
        .expect("first");
    let closed = suggestions
        .add(
            "conv-1",
            "/work/alpha",
            &input("Fechada", SuggestionSeverity::Low),
        )
        .expect("closed");
    let last = suggestions
        .add(
            "conv-1",
            "/work/alpha",
            &input("Última", SuggestionSeverity::Low),
        )
        .expect("last");
    suggestions
        .set_status(&closed.id, SuggestionStatus::Rejected, None)
        .expect("reject");
    set_updated_at(&db, &first.id, "2026-01-01T00:00:01.000Z");
    set_updated_at(&db, &last.id, "2026-01-01T00:00:03.000Z");

    let listed: Vec<String> = suggestions
        .list("conv-1")
        .expect("list")
        .into_iter()
        .map(|item| item.title)
        .collect();

    assert_eq!(listed, ["Última", "Primeira"]);
}

#[test]
fn a_page_reports_whether_more_open_suggestions_follow() {
    let db = open("suggestions-page");
    let suggestions = Suggestions::new(db.connection());
    for title in ["Um", "Dois", "Três"] {
        suggestions
            .add(
                "conv-1",
                "/work/alpha",
                &input(title, SuggestionSeverity::Info),
            )
            .expect("add");
    }

    let first = suggestions.page("conv-1", 0, Some(2)).expect("first page");
    assert_eq!(first.items.len(), 2);
    assert!(first.has_more);
    let last = suggestions.page("conv-1", 2, Some(2)).expect("last page");
    assert_eq!(last.items.len(), 1);
    assert!(!last.has_more);
}

#[test]
fn a_review_keeps_persistent_suggestions_resolves_missing_ones_and_reports_changes() {
    let db = open("suggestions-reconcile");
    let suggestions = Suggestions::new(db.connection());
    let first = suggestions
        .reconcile(
            "conv-1",
            "/work/alpha",
            &[
                input("Revisar configuração", SuggestionSeverity::Medium),
                input("Remover cache", SuggestionSeverity::Low),
            ],
            true,
        )
        .expect("first review");
    assert_eq!(first.comparison.new_suggestions.len(), 2);
    let cache_id = first.suggestions[1].id.clone();

    let second = suggestions
        .reconcile(
            "conv-1",
            "/work/alpha",
            &[
                input("REVISAR CONFIGURACAO", SuggestionSeverity::High),
                input("Nova análise", SuggestionSeverity::Info),
            ],
            true,
        )
        .expect("second review");

    let comparison = &second.comparison;
    assert_eq!(
        comparison
            .new_suggestions
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>(),
        ["Nova análise"]
    );
    assert_eq!(
        comparison
            .persistent_suggestions
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>(),
        ["Revisar configuração"],
        "the persistent item keeps the title it was stored with"
    );
    assert_eq!(comparison.severity_changes.len(), 1);
    assert_eq!(
        comparison.severity_changes[0].from,
        SuggestionSeverity::Medium
    );
    assert_eq!(comparison.severity_changes[0].to, SuggestionSeverity::High);
    assert_eq!(comparison.resolved_suggestions.len(), 1);
    assert_eq!(comparison.resolved_suggestions[0].id, cache_id);

    let resolved = suggestions
        .get(&cache_id, Some("conv-1"))
        .expect("get")
        .expect("present");
    assert_eq!(resolved.status, SuggestionStatus::Resolved);
    assert_eq!(
        resolved
            .history
            .last()
            .and_then(|entry| entry.result.as_deref()),
        Some("A sugestão não reapareceu na revisão estruturada atual.")
    );
    let updated = second
        .suggestions
        .iter()
        .find(|item| item.title == "REVISAR CONFIGURACAO")
        .expect("updated suggestion");
    assert_eq!(updated.severity, SuggestionSeverity::High);
}

#[test]
fn an_accepted_suggestion_is_kept_as_it_was_when_it_reappears() {
    let db = open("suggestions-accepted");
    let suggestions = Suggestions::new(db.connection());
    suggestions
        .reconcile(
            "conv-1",
            "/work/alpha",
            &[input("Revisar config", SuggestionSeverity::Medium)],
            true,
        )
        .expect("first review");
    let accepted = suggestions.list("conv-1").expect("list")[0].clone();
    suggestions
        .set_status(&accepted.id, SuggestionStatus::Accepted, None)
        .expect("accept");

    let review = suggestions
        .reconcile(
            "conv-1",
            "/work/alpha",
            &[input("revisar  config", SuggestionSeverity::High)],
            true,
        )
        .expect("second review");

    assert_eq!(review.comparison.persistent_suggestions.len(), 1);
    assert_eq!(review.comparison.severity_changes.len(), 1);
    let kept: &Suggestion = &review.suggestions[0];
    assert_eq!(kept.status, SuggestionStatus::Accepted);
    assert_eq!(kept.severity, SuggestionSeverity::Medium);
    assert_eq!(kept.title, "Revisar config");
}

#[test]
fn a_review_without_resolution_leaves_missing_suggestions_open() {
    let db = open("suggestions-no-resolve");
    let suggestions = Suggestions::new(db.connection());
    suggestions
        .reconcile(
            "conv-1",
            "/work/alpha",
            &[input("Revisar config", SuggestionSeverity::Low)],
            true,
        )
        .expect("first review");

    let review = suggestions
        .reconcile("conv-1", "/work/alpha", &[], false)
        .expect("second review");

    assert!(review.comparison.resolved_suggestions.is_empty());
    assert_eq!(
        suggestions.list("conv-1").expect("list")[0].status,
        SuggestionStatus::New
    );
}
