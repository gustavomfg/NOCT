//! The review snapshot of a reply, checked against `extractSuggestions` and `suggestionInputSchema` in
//! `shared/suggestions.ts`, and the comparison text against `reviewComparisonMarkdown`.

use noct_core::suggestions::{
    ReviewComparison, ReviewComparisonItem, ReviewSeverityChange, SuggestionCategory,
    SuggestionEvidence, SuggestionLevel, SuggestionSeverity, SuggestionSnapshot,
    extract_suggestions, review_comparison_markdown,
};

fn item(title: &str) -> String {
    format!(
        r#"{{"title":"{title}","description":"A problem","reasoning":"Observed","category":"testing","severity":"medium"}}"#
    )
}

fn fenced(body: &str) -> String {
    format!("```nocturne-suggestions\n{body}\n```")
}

#[test]
fn a_valid_snapshot_is_read_with_the_schema_defaults_and_removed_from_the_reply() {
    let reply = format!(
        "Revisão feita.\n\n{}",
        fenced(
            r#"[
                {"title":"  Missing   tests\u0007 ","description":" No tests for the parser ","reasoning":"Observed",
                 "category":"testing","severity":"medium","evidence":[{"source":"arquivo","detail":"no test file"}]},
                {"title":"Pin the toolchain","description":"d","reasoning":"r","category":"dependency",
                 "severity":"low","confidence":90,"complexity":"high"}
            ]"#
        )
    );
    let extraction = extract_suggestions(&reply);

    assert!(extraction.structured);
    assert_eq!(extraction.snapshot, SuggestionSnapshot::Valid);
    assert_eq!(extraction.content, "Revisão feita.");
    assert_eq!(extraction.suggestions.len(), 2);

    let first = &extraction.suggestions[0];
    assert_eq!(
        first.title, "Missing tests",
        "control characters and runs of spaces are sanitized"
    );
    assert_eq!(first.description, "No tests for the parser");
    assert_eq!(first.category, SuggestionCategory::Testing);
    assert_eq!(first.severity, SuggestionSeverity::Medium);
    assert_eq!(first.affected_files, Vec::<String>::new());
    assert_eq!(first.proposed_changes, "");
    assert_eq!(
        first.complexity,
        SuggestionLevel::Medium,
        "complexity defaults to medium"
    );
    assert_eq!(
        first.risk,
        SuggestionLevel::Medium,
        "risk defaults to medium"
    );
    assert_eq!(first.confidence, Some(60));
    assert_eq!(first.source.as_deref(), Some("Análise do agente"));
    assert_eq!(first.responsible.as_deref(), Some("Agente de revisão"));
    assert_eq!(
        first.evidence,
        Some(vec![SuggestionEvidence {
            source: "arquivo".to_string(),
            detail: "no test file".to_string(),
            location: None,
        }])
    );

    let second = &extraction.suggestions[1];
    assert_eq!(second.confidence, Some(90));
    assert_eq!(second.complexity, SuggestionLevel::High);
    assert_eq!(second.evidence, Some(Vec::new()));
}

#[test]
fn two_markers_are_an_invalid_snapshot_and_the_blocks_stay_in_the_reply() {
    let reply = format!(
        "{}\n{}",
        fenced(&format!("[{}]", item("One"))),
        fenced(&format!("[{}]", item("Two")))
    );
    let extraction = extract_suggestions(&reply);
    assert!(!extraction.structured);
    assert_eq!(extraction.snapshot, SuggestionSnapshot::Invalid);
    assert!(extraction.suggestions.is_empty());
    assert!(
        extraction.content.contains("```nocturne-suggestions"),
        "the reply keeps its blocks"
    );
}

#[test]
fn one_item_that_does_not_match_refuses_the_whole_snapshot() {
    let reply = fenced(&format!(
        "[{}, {{\"title\":\"Only a title\"}}]",
        item("Good")
    ));
    let extraction = extract_suggestions(&reply);
    assert!(!extraction.structured);
    assert_eq!(extraction.snapshot, SuggestionSnapshot::Invalid);
    assert!(
        extraction.suggestions.is_empty(),
        "a strict block keeps none of its items"
    );
}

#[test]
fn a_block_that_is_not_an_array_is_invalid() {
    let extraction = extract_suggestions(&fenced(&item("Alone")));
    assert_eq!(extraction.snapshot, SuggestionSnapshot::Invalid);
}

#[test]
fn a_json_fence_at_the_very_end_is_read_when_there_is_no_marker() {
    let reply = format!(
        "Texto antes.\n```json\n[{}]\n```   \n",
        item("From the fence")
    );
    let extraction = extract_suggestions(&reply);
    assert!(extraction.structured);
    assert_eq!(extraction.content, "Texto antes.");
    assert_eq!(extraction.suggestions[0].title, "From the fence");

    let in_the_middle = format!("```json\n[{}]\n```\nDepois.", item("Not last"));
    let absent = extract_suggestions(&in_the_middle);
    assert_eq!(
        absent.snapshot,
        SuggestionSnapshot::Absent,
        "only a fence at the end counts"
    );
    assert_eq!(absent.content, in_the_middle.trim());
}

#[test]
fn a_marker_must_end_its_word() {
    let reply = format!("```nocturne-suggestionsX\n[{}]\n```", item("Not a marker"));
    let extraction = extract_suggestions(&reply);
    assert_eq!(extraction.snapshot, SuggestionSnapshot::Absent);
}

#[test]
fn the_text_limits_and_the_numbers_of_the_schema_refuse_an_item() {
    let refused = |item_json: String| {
        let extraction = extract_suggestions(&fenced(&format!("[{item_json}]")));
        assert_eq!(
            extraction.snapshot,
            SuggestionSnapshot::Invalid,
            "{item_json}"
        );
    };
    let base = |field: &str, value: &str| {
        format!(
            r#"{{"title":"T","description":"d","reasoning":"r","category":"bug","severity":"high",{field}:{value}}}"#
        )
    };
    refused(base("\"confidence\"", "101"));
    refused(base("\"confidence\"", "50.5"));
    refused(base("\"title\"", "\"   \\u0007  \""));
    refused(base("\"description\"", "\"   \""));
    refused(format!(
        r#"{{"title":"{}","description":"d","reasoning":"r","category":"bug","severity":"high"}}"#,
        "a".repeat(201)
    ));
    refused(base(
        "\"affectedFiles\"",
        &format!("[{}]", vec!["\"f\""; 101].join(",")),
    ));
    refused(base(
        "\"evidence\"",
        &format!(
            "[{}]",
            vec!["{\"source\":\"s\",\"detail\":\"d\"}"; 51].join(",")
        ),
    ));
    refused(base(
        "\"evidence\"",
        "[{\"source\":\"s\",\"detail\":\"d\",\"extra\":1}]",
    ));
    refused(base(
        "\"proposedChanges\"",
        &format!("\"{}\"", "a".repeat(100_001)),
    ));
    refused(base("\"severity\"", "\"urgent\""));
}

#[test]
fn the_limits_at_their_edge_are_accepted_and_an_unknown_key_is_ignored() {
    let at_the_edge = format!(
        r#"[{{"title":"T","description":"d","reasoning":"r","category":"bug","severity":"high",
            "affectedFiles":[{}],"confidence":80.0,"unknownKey":true,"proposedChanges":"  diff  "}}]"#,
        vec!["\"f\""; 100].join(",")
    );
    let extraction = extract_suggestions(&fenced(&at_the_edge));
    assert_eq!(extraction.snapshot, SuggestionSnapshot::Valid);
    let suggestion = &extraction.suggestions[0];
    assert_eq!(suggestion.affected_files.len(), 100);
    assert_eq!(suggestion.confidence, Some(80));
    assert_eq!(
        suggestion.proposed_changes, "  diff  ",
        "the proposed changes are not trimmed"
    );
}

#[test]
fn a_reply_without_a_snapshot_is_only_trimmed() {
    let extraction = extract_suggestions("  Só texto.  \n");
    assert_eq!(extraction.snapshot, SuggestionSnapshot::Absent);
    assert_eq!(extraction.content, "Só texto.");
}

#[test]
fn the_comparison_text_has_the_words_of_the_interface() {
    let comparison = |changes: Vec<ReviewSeverityChange>| ReviewComparison {
        reviewed_at: "2026-11-17T00:00:00.000Z".to_string(),
        new_suggestions: vec![ReviewComparisonItem {
            id: "a".to_string(),
            title: "New".to_string(),
            severity: SuggestionSeverity::Low,
        }],
        persistent_suggestions: Vec::new(),
        resolved_suggestions: vec![
            ReviewComparisonItem {
                id: "b".to_string(),
                title: "Old".to_string(),
                severity: SuggestionSeverity::High,
            },
            ReviewComparisonItem {
                id: "c".to_string(),
                title: "Older".to_string(),
                severity: SuggestionSeverity::Info,
            },
        ],
        severity_changes: changes,
    };

    assert_eq!(
        review_comparison_markdown(&comparison(Vec::new())),
        "### Comparação com a revisão anterior\n\n- Novas: 1\n- Persistentes: 0\n- Resolvidas nesta revisão: 2\n- Mudanças de severidade: nenhuma"
    );
    let changed = comparison(vec![ReviewSeverityChange {
        id: "d".to_string(),
        title: "Shifted".to_string(),
        from: SuggestionSeverity::Low,
        to: SuggestionSeverity::Critical,
    }]);
    assert!(
        review_comparison_markdown(&changed)
            .ends_with("- Mudanças de severidade: Shifted: low → critical"),
        "{}",
        review_comparison_markdown(&changed)
    );
}

#[test]
fn an_evidence_location_is_kept_when_given() {
    let reply = fenced(
        r#"[{"title":"T","description":"d","reasoning":"r","category":"bug","severity":"high",
            "evidence":[{"source":"arquivo","detail":"x","location":"src/a.rs:12"}]}]"#,
    );
    let extraction = extract_suggestions(&reply);
    assert_eq!(
        extraction.suggestions[0]
            .evidence
            .as_ref()
            .expect("evidence")[0]
            .location
            .as_deref(),
        Some("src/a.rs:12")
    );
}
