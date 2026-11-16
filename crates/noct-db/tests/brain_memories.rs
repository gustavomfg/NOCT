//! Brain memories, checked against `BrainMemoryRepository.ts`. The row gate compares the same
//! reads and the same error messages with the TypeScript repository.

use noct_core::brain_memory::{
    BrainMemory, BrainMemoryCandidate, BrainMemoryHistoryAction, BrainMemoryKind, BrainMemoryScope,
    BrainMemorySource, BrainMemoryStatus, CreateBrainMemoryInput, InitialBrainMemoryStatus,
    UpdateBrainMemoryInput,
};
use noct_db::Database;
use noct_db::repositories::brain_memories::BrainMemories;

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";
const ALPHA: &str = "/work/alpha";
const BETA: &str = "/work/beta";

fn open(name: &str) -> Database {
    let db = Database::open_runtime(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) VALUES \
                 ('{ALPHA}','alpha','{AT}','{AT}'),('{BETA}','beta','{AT}','{AT}');\
             INSERT INTO conversations(id,title,workspace,created_at,updated_at) VALUES \
                 ('conv-1','Um','{ALPHA}','{AT}','{AT}'),\
                 ('conv-2','Dois','{ALPHA}','{AT}','{AT}'),\
                 ('conv-3','Três','{BETA}','{AT}','{AT}');"
        ))
        .expect("workspaces and conversations");
    db
}

fn workspace(content: &str) -> CreateBrainMemoryInput {
    CreateBrainMemoryInput {
        conversation_id: None,
        kind: BrainMemoryKind::Fact,
        scope: BrainMemoryScope::Workspace,
        content: content.to_string(),
        confidence: None,
        source_type: None,
        source_id: None,
        status: None,
    }
}

fn in_conversation(conversation: &str, content: &str) -> CreateBrainMemoryInput {
    CreateBrainMemoryInput {
        scope: BrainMemoryScope::Conversation,
        conversation_id: Some(conversation.to_string()),
        ..workspace(content)
    }
}

fn status(status: BrainMemoryStatus) -> UpdateBrainMemoryInput {
    UpdateBrainMemoryInput {
        status: Some(status),
        ..Default::default()
    }
}

fn message<T: std::fmt::Debug>(result: noct_db::Result<T>) -> String {
    result.expect_err("the call must fail").to_string()
}

#[test]
fn creates_candidates_by_default_and_trims_the_content() {
    let db = open("brain-create");
    let repo = BrainMemories::new(db.connection());
    let row = repo
        .create(ALPHA, &workspace("  Texto com espaços.  "))
        .expect("create");

    assert_eq!(row.content, "Texto com espaços.");
    assert_eq!(row.status, BrainMemoryStatus::Candidate);
    assert_eq!(row.confidence, 70);
    assert_eq!(row.source_type, BrainMemorySource::Manual);
    assert_eq!(row.last_confirmed_at, None);
    assert_eq!(row.use_count, 0);

    let history = repo.list_history(&row.id, ALPHA).expect("history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].action, BrainMemoryHistoryAction::Created);
    assert_eq!(history[0].from_status, None);
    assert_eq!(history[0].summary, "Memória criada manualmente.");
}

#[test]
fn an_active_memory_is_confirmed_when_it_is_created() {
    let db = open("brain-active");
    let repo = BrainMemories::new(db.connection());
    let row = repo
        .create(
            ALPHA,
            &CreateBrainMemoryInput {
                status: Some(InitialBrainMemoryStatus::Active),
                ..workspace("Usar SQLite.")
            },
        )
        .expect("create");

    assert_eq!(row.status, BrainMemoryStatus::Active);
    assert_eq!(
        row.last_confirmed_at.as_deref(),
        Some(row.created_at.as_str())
    );
}

#[test]
fn rejects_conversations_outside_the_scope_and_missing_memories() {
    let db = open("brain-scope");
    let repo = BrainMemories::new(db.connection());

    let needs_conversation = CreateBrainMemoryInput {
        scope: BrainMemoryScope::Conversation,
        ..workspace("Sem conversa.")
    };
    assert_eq!(
        message(repo.create(ALPHA, &needs_conversation)),
        "Memória da conversa exige uma conversa válida."
    );
    assert_eq!(
        message(repo.create(ALPHA, &in_conversation("conv-3", "Outra conversa."))),
        "A conversa da memória não pertence ao workspace atual."
    );
    assert_eq!(
        message(repo.update("missing", ALPHA, &status(BrainMemoryStatus::Active))),
        "Memória não encontrada."
    );
}

#[test]
fn a_workspace_memory_never_keeps_a_conversation() {
    let db = open("brain-workspace-scope");
    let repo = BrainMemories::new(db.connection());
    let row = repo
        .create(
            ALPHA,
            &CreateBrainMemoryInput {
                conversation_id: Some("conv-1".to_string()),
                ..workspace("Vale para o workspace.")
            },
        )
        .expect("create");
    assert_eq!(row.conversation_id, None);

    let moved = repo
        .update(
            &row.id,
            ALPHA,
            &UpdateBrainMemoryInput {
                scope: Some(BrainMemoryScope::Conversation),
                conversation_id: Some(Some("conv-2".to_string())),
                ..Default::default()
            },
        )
        .expect("move to a conversation");
    assert_eq!(moved.scope, BrainMemoryScope::Conversation);
    assert_eq!(moved.conversation_id.as_deref(), Some("conv-2"));

    let back = repo
        .update(
            &row.id,
            ALPHA,
            &UpdateBrainMemoryInput {
                scope: Some(BrainMemoryScope::Workspace),
                ..Default::default()
            },
        )
        .expect("back to the workspace");
    assert_eq!(back.conversation_id, None);
}

#[test]
fn follows_the_lifecycle_and_records_each_change_newest_first() {
    let db = open("brain-lifecycle");
    let repo = BrainMemories::new(db.connection());
    let row = repo
        .create(ALPHA, &workspace("Usar SQLite."))
        .expect("create");

    let approved = repo
        .update(&row.id, ALPHA, &status(BrainMemoryStatus::Active))
        .expect("approve");
    assert!(approved.last_confirmed_at.is_some());
    let edited = repo
        .update(
            &row.id,
            ALPHA,
            &UpdateBrainMemoryInput {
                content: Some("  Usar SQLite local.  ".to_string()),
                ..Default::default()
            },
        )
        .expect("edit");
    assert_eq!(edited.content, "Usar SQLite local.");
    repo.update(&row.id, ALPHA, &status(BrainMemoryStatus::Outdated))
        .expect("outdate");
    repo.update(&row.id, ALPHA, &status(BrainMemoryStatus::Archived))
        .expect("archive");
    let restored = repo
        .update(&row.id, ALPHA, &status(BrainMemoryStatus::Active))
        .expect("restore");
    assert_eq!(restored.status, BrainMemoryStatus::Active);

    assert_eq!(
        message(repo.update(&row.id, ALPHA, &status(BrainMemoryStatus::Candidate))),
        "Transição de memória inválida: active → candidate."
    );

    let actions: Vec<BrainMemoryHistoryAction> = repo
        .list_history(&row.id, ALPHA)
        .expect("history")
        .into_iter()
        .map(|entry| entry.action)
        .collect();
    assert_eq!(
        actions,
        [
            BrainMemoryHistoryAction::Restored,
            BrainMemoryHistoryAction::Archived,
            BrainMemoryHistoryAction::MarkedOutdated,
            BrainMemoryHistoryAction::Edited,
            BrainMemoryHistoryAction::Approved,
            BrainMemoryHistoryAction::Created,
        ]
    );
}

#[test]
fn agent_candidates_are_stored_once_per_equivalent_content() {
    let db = open("brain-candidates");
    let repo = BrainMemories::new(db.connection());
    let candidates = [
        BrainMemoryCandidate {
            kind: BrainMemoryKind::Fact,
            scope: BrainMemoryScope::Workspace,
            content: "O projeto usa Electron.".to_string(),
            confidence: 80,
        },
        BrainMemoryCandidate {
            kind: BrainMemoryKind::Constraint,
            scope: BrainMemoryScope::Conversation,
            content: "Não alterar o schema.".to_string(),
            confidence: 75,
        },
        BrainMemoryCandidate {
            kind: BrainMemoryKind::Fact,
            scope: BrainMemoryScope::Workspace,
            content: "  o PROJETO usa electron.  ".to_string(),
            confidence: 60,
        },
        BrainMemoryCandidate {
            kind: BrainMemoryKind::Constraint,
            scope: BrainMemoryScope::Conversation,
            content: "Não alterar o schema.".to_string(),
            confidence: 75,
        },
    ];
    let created = repo
        .create_candidates(ALPHA, "conv-1", &candidates)
        .expect("candidates");

    assert_eq!(created.len(), 2);
    assert!(
        created
            .iter()
            .all(|row| row.source_type == BrainMemorySource::Agent)
    );
    assert!(
        created
            .iter()
            .all(|row| row.status == BrainMemoryStatus::Candidate)
    );
    assert_eq!(created[0].conversation_id, None);
    assert_eq!(created[1].conversation_id.as_deref(), Some("conv-1"));
    let history = repo.list_history(&created[0].id, ALPHA).expect("history");
    assert_eq!(history[0].summary, "Candidata proposta pelo agente.");
}

#[test]
fn pages_order_by_relevance_when_searching_and_by_update_otherwise() {
    let db = open("brain-pages");
    let repo = BrainMemories::new(db.connection());
    repo.create(
        ALPHA,
        &CreateBrainMemoryInput {
            status: Some(InitialBrainMemoryStatus::Active),
            ..workspace("Usar SQLite local.")
        },
    )
    .expect("active");
    repo.create(ALPHA, &workspace("Sem SQLite em produção."))
        .expect("candidate");
    repo.create(ALPHA, &workspace("Interface em português."))
        .expect("other");

    let all = repo.list_page(ALPHA, 0, Some(10), "", None).expect("all");
    assert_eq!(all.items.len(), 3);
    assert!(!all.has_more);

    let first = repo.list_page(ALPHA, 0, Some(1), "", None).expect("first");
    assert_eq!(first.items.len(), 1);
    assert!(first.has_more);

    let active = repo
        .list_page(ALPHA, 0, Some(10), "", Some(BrainMemoryStatus::Active))
        .expect("active");
    assert_eq!(active.items.len(), 1);
    assert_eq!(active.items[0].content, "Usar SQLite local.");

    let searched = repo
        .list_page(ALPHA, 0, Some(10), "sqlite", None)
        .expect("search");
    assert_eq!(searched.items.len(), 2, "search ignores the status filter");
    assert!(
        searched
            .items
            .iter()
            .all(|row| row.content.contains("SQLite"))
    );
}

#[test]
fn retrieval_reads_only_active_memories_of_the_workspace_and_the_conversation() {
    let db = open("brain-retrieve");
    let repo = BrainMemories::new(db.connection());
    let active = CreateBrainMemoryInput {
        status: Some(InitialBrainMemoryStatus::Active),
        ..workspace("Projeto usa SQLite.")
    };
    let shared = repo.create(ALPHA, &active).expect("workspace");
    let mine = repo
        .create(
            ALPHA,
            &CreateBrainMemoryInput {
                status: Some(InitialBrainMemoryStatus::Active),
                ..in_conversation("conv-1", "Preferência por SQLite nesta conversa.")
            },
        )
        .expect("conversation");
    repo.create(
        ALPHA,
        &CreateBrainMemoryInput {
            status: Some(InitialBrainMemoryStatus::Active),
            ..in_conversation("conv-2", "Outra conversa sobre SQLite.")
        },
    )
    .expect("other conversation");
    repo.create(ALPHA, &workspace("Candidata sobre SQLite."))
        .expect("candidate");

    let found: Vec<String> = repo
        .retrieve(ALPHA, "conv-1", "sqlite", None)
        .expect("retrieve")
        .into_iter()
        .map(|row| row.id)
        .collect();
    assert_eq!(found.len(), 2);
    assert!(found.contains(&shared.id) && found.contains(&mine.id));

    assert!(
        repo.retrieve(ALPHA, "conv-1", "!! ?", None)
            .expect("no tokens")
            .is_empty()
    );
}

#[test]
fn usage_counts_each_distinct_memory_once_and_at_most_fifty() {
    let db = open("brain-used");
    let repo = BrainMemories::new(db.connection());
    let ids: Vec<String> = (0..51)
        .map(|index| {
            repo.create(ALPHA, &workspace(&format!("Memória {index}.")))
                .expect("create")
                .id
        })
        .collect();

    let mut repeated = vec![ids[0].clone(), ids[0].clone()];
    repeated.extend(ids.iter().cloned());
    repo.mark_used(&repeated).expect("mark used");

    let counts: Vec<i64> = ids
        .iter()
        .map(|id| repo.get(id, ALPHA).expect("get").expect("memory").use_count)
        .collect();
    assert_eq!(counts[0], 1);
    assert!(counts[1..50].iter().all(|count| *count == 1));
    assert_eq!(
        counts[50], 0,
        "the fifty-first distinct memory is not counted"
    );
}

#[test]
fn deleting_a_memory_removes_its_history() {
    let db = open("brain-delete");
    let repo = BrainMemories::new(db.connection());
    let row = repo.create(ALPHA, &workspace("Remover.")).expect("create");
    repo.update(&row.id, ALPHA, &status(BrainMemoryStatus::Active))
        .expect("approve");

    assert!(repo.delete(&row.id, ALPHA).expect("delete"));
    assert!(!repo.delete(&row.id, ALPHA).expect("second delete"));
    assert!(repo.get(&row.id, ALPHA).expect("get").is_none());
    let remaining: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM brain_memory_history WHERE memory_id=?1",
            [&row.id],
            |row| row.get(0),
        )
        .expect("count history");
    assert_eq!(remaining, 0);
}

#[test]
fn a_memory_of_another_workspace_is_not_visible() {
    let db = open("brain-isolation");
    let repo = BrainMemories::new(db.connection());
    let row: BrainMemory = repo
        .create(BETA, &workspace("Projeto beta."))
        .expect("beta");

    assert!(repo.get(&row.id, ALPHA).expect("get").is_none());
    assert_eq!(
        message(repo.list_history(&row.id, ALPHA)),
        "Memória não encontrada."
    );
    assert_eq!(
        repo.list_page(ALPHA, 0, None, "", None)
            .expect("page")
            .items
            .len(),
        0
    );
}
