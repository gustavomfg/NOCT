//! Row-level gate of phase 2. `scripts/verify-row-gate.mjs` has the TypeScript
//! persistence layer write a database at schema 32 and record the rows its repositories
//! return, in `NOCT_ROW_GATE_DIR`. Rust must open the same file and return the same
//! rows. The test is ignored by default because it needs that directory.

// The expected-rows object is one `json!` literal with many keys, which the default limit rejects.
#![recursion_limit = "256"]

use std::fs;
use std::path::PathBuf;

use noct_core::brain_memory::{
    BrainMemoryKind, BrainMemoryScope, BrainMemoryStatus, CreateBrainMemoryInput,
    UpdateBrainMemoryInput, is_safe_brain_memory_content,
};
use noct_core::suggestions::{
    ReviewComparisonItem, SuggestionCategory, SuggestionInput, sanitize_suggestion_title,
    suggestion_identity,
};
use noct_core::workspace_evidence::{EvidenceKind, EvidenceReference};
use noct_db::Database;
use noct_db::Result;
use noct_db::repositories::Page;
use noct_db::repositories::artifacts::Artifacts;
use noct_db::repositories::brain_memories::BrainMemories;
use noct_db::repositories::change_sets::ChangeSets;
use noct_db::repositories::checkpoints::Checkpoints;
use noct_db::repositories::conversations::Conversations;
use noct_db::repositories::execution_evidence::ExecutionEvidence;
use noct_db::repositories::executions::Executions;
use noct_db::repositories::model_catalog::ModelCatalog;
use noct_db::repositories::operation_recovery::OperationRecovery;
use noct_db::repositories::provider_configurations::ProviderConfigurations;
use noct_db::repositories::settings::Settings;
use noct_db::repositories::suggestions::Suggestions;
use noct_db::repositories::validation_runs::ValidationRuns;
use noct_db::repositories::workspace_evidence::WorkspaceEvidence;
use noct_db::repositories::workspace_memory::WorkspaceMemory;
use noct_db::repositories::workspace_model_bindings::ModelBindings;
use noct_db::repositories::workspaces::Workspaces;
use rusqlite::Connection;
use rusqlite::types::ValueRef;
use serde::Serialize;
use serde_json::{Map, Value, json};

mod common;
use common::scratch;

/// The directory written by `scripts/verify-row-gate.mjs`.
fn gate_directory() -> PathBuf {
    PathBuf::from(
        std::env::var("NOCT_ROW_GATE_DIR")
            .expect("NOCT_ROW_GATE_DIR is not set: run `node scripts/verify-row-gate.mjs`"),
    )
}

fn copy_typescript_database(name: &str) -> PathBuf {
    let directory = scratch(name);
    fs::create_dir_all(&directory).expect("scratch directory");
    fs::copy(
        gate_directory().join("nocturne.db"),
        directory.join("nocturne.db"),
    )
    .expect("copy the TypeScript database");
    directory
}

/// `Page` has no serialized form of its own; the gate compares its items and flag.
fn page_json<T: Serialize>(page: Page<T>) -> Value {
    json!({ "items": page.items, "hasMore": page.has_more })
}

fn expected() -> Value {
    let text = fs::read_to_string(gate_directory().join("expected.json")).expect("expected.json");
    serde_json::from_str(&text).expect("expected.json is JSON")
}

/// Keys each identifier to the rows `read` returns for it, as the fixture does.
fn keyed<T: Serialize>(ids: &[String], read: impl Fn(&str) -> Result<T>) -> Value {
    let mut map = Map::new();
    for id in ids {
        map.insert(
            id.clone(),
            serde_json::to_value(read(id).expect("read")).expect("json"),
        );
    }
    Value::Object(map)
}

#[test]
#[ignore = "needs NOCT_ROW_GATE_DIR; run node scripts/verify-row-gate.mjs"]
fn rust_returns_the_rows_the_typescript_repositories_wrote() {
    let directory = copy_typescript_database("row-gate-typescript-database");
    let db = Database::open(&directory).expect("open the TypeScript database");
    let conn = db.connection();
    let expected = expected();

    let workspaces = Workspaces::new(conn).list().expect("workspaces");
    let conversations = Conversations::new(conn).list().expect("conversations");
    let conversation_ids: Vec<String> = conversations.iter().map(|row| row.id.clone()).collect();
    let workspace_paths: Vec<String> = workspaces.iter().map(|row| row.path.clone()).collect();
    let conversations_api = Conversations::new(conn);
    let artifacts_api = Artifacts::new(conn);
    let memory_api = WorkspaceMemory::new(conn);
    let executions_api = Executions::new(conn);
    let checkpoints_api = Checkpoints::new(conn);
    let change_sets_api = ChangeSets::new(conn);
    let providers_api = ProviderConfigurations::new(conn);
    let provider_ids: Vec<String> = providers_api
        .list()
        .expect("providers")
        .iter()
        .map(|row| row.id.clone())
        .collect();
    let mut provider_references = providers_api
        .credential_references()
        .expect("credential references");
    provider_references.sort();

    let suggestions_api = Suggestions::new(conn);
    let suggestion_ids = keys(&expected, "suggestionById");
    let identities: Map<String, Value> = keys(&expected, "suggestionIdentities")
        .into_iter()
        .map(|title| {
            let identity = suggestion_identity(SuggestionCategory::Bug, &title);
            (title, json!(identity))
        })
        .collect();
    let sanitized: Map<String, Value> = keys(&expected, "sanitizedTitles")
        .into_iter()
        .map(|title| {
            let clean = sanitize_suggestion_title(&title);
            (title, json!(clean))
        })
        .collect();

    let brain = BrainMemories::new(conn);
    let brain_ids = keys(&expected, "brainMemoryById");
    let brain_history_ids = keys(&expected, "brainMemoryHistory");
    let removed_ids = keys(&expected, "brainMemoryAfterDelete");
    let constraint_id: String = conn
        .query_row(
            "SELECT id FROM brain_memories WHERE content = ?1",
            ["Não alterar o schema sem migração."],
            |row| row.get(0),
        )
        .expect("the constraint memory");
    let brain_errors = json!({
        "invalidTransition": error_of(brain.update(&constraint_id, "/work/alpha", &status_update(BrainMemoryStatus::Outdated))),
        "missing": error_of(brain.update("missing-brain", "/work/alpha", &status_update(BrainMemoryStatus::Active))),
        "conversationRequired": error_of(brain.create("/work/alpha", &brain_input(BrainMemoryScope::Conversation, None, "Sem conversa."))),
        "otherWorkspace": error_of(brain.create("/work/beta", &brain_input(BrainMemoryScope::Conversation, Some("conv-1"), "Conversa de outro workspace."))),
    });
    let brain_safety: Vec<Value> = expected["brainSafety"]
        .as_array()
        .expect("brain safety cases")
        .iter()
        .map(|case| {
            let input = case["input"].as_str().expect("the case input");
            json!({ "input": input, "safe": is_safe_brain_memory_content(input) })
        })
        .collect();

    let actual = json!({
        "modelCatalog": ModelCatalog::new(conn).list(None).expect("model catalog"),
        "workspaceModelBindings": ModelBindings::new(conn).list().expect("bindings"),
        "settings": Settings::new(conn).get().expect("settings"),
        "workspaces": workspaces,
        "conversations": conversations,
        "messages": keyed(&conversation_ids, |id| conversations_api.list_messages(id)),
        "recentMessages": keyed(&conversation_ids, |id| conversations_api.list_recent_messages(id, 200)),
        "artifacts": keyed(&conversation_ids, |id| artifacts_api.list(id)),
        "workspaceMemory": keyed(&workspace_paths, |path| memory_api.get(path)),
        "providerConfigurations": providers_api.list().expect("providers"),
        "providerConfigurationById": keyed(&provider_ids, |id| providers_api.get(id)),
        "providerCredentialReferences": provider_references,
        "executions": executions_api.list("/work/alpha", None, 50).expect("executions"),
        "executionById": keyed(&["exec-g1".to_string(), "exec-g2".to_string()], |id| executions_api.get(id, None)),
        "checkpoints": checkpoints_api.list("exec-g1").expect("checkpoints"),
        "checkpointById": keyed(&["ck-g1".to_string()], |id| checkpoints_api.get(id, None)),
        "checkpointFiles": keyed(&["ck-g1".to_string()], |id| checkpoints_api.list_files(id)),
        "changeSets": change_sets_api.list("exec-g1").expect("change sets"),
        "changeSetLatest": change_sets_api.get_by_execution_id("exec-g1").expect("latest"),
        "changeSetById": keyed(&["cs-g1".to_string()], |id| change_sets_api.get_by_id(id, Some("exec-g1"))),
        "changes": change_sets_api.list_changes("cs-g1").expect("changes"),
        "changeById": keyed(&["chg-g1".to_string(), "chg-g2".to_string()], |id| change_sets_api.get_change(id, None)),
        "hunks": change_sets_api.list_hunks("chg-g1").expect("hunks"),
        "hunkById": keyed(&["hk-g1".to_string(), "hk-g2".to_string()], |id| change_sets_api.get_hunk(id)),
        "workspaceEvidence": WorkspaceEvidence::new(conn).list("/work/alpha", None).expect("evidence"),
        "validationRuns": ValidationRuns::new(conn).list("/work/alpha", None).expect("validation runs"),
        "validationRunLatest": ValidationRuns::new(conn).latest("/work/alpha").expect("latest run"),
        "validationRunPage": page_json(ValidationRuns::new(conn).page("/work/alpha", 0, Some(1)).expect("page")),
        "validationRunsBeta": ValidationRuns::new(conn).list("/work/beta", None).expect("beta runs"),
        "commands": keyed(&["exec-g1".to_string()], |id| ExecutionEvidence::new(conn).list_commands(id, None)),
        "errors": keyed(&["exec-g1".to_string()], |id| ExecutionEvidence::new(conn).list_errors(id, None)),
        "validationLinks": keyed(&["exec-g1".to_string()], |id| ExecutionEvidence::new(conn).list_validation_links(id)),
        "suggestionList": suggestions_api.list("conv-1").expect("suggestions"),
        "suggestionPage": page_json(suggestions_api.page("conv-1", 0, Some(2)).expect("suggestion page")),
        "suggestionById": keyed(&suggestion_ids, |id| suggestions_api.get(id, None)),
        "suggestionIdentities": Value::Object(identities),
        "sanitizedTitles": Value::Object(sanitized),
        "brainMemoryById": keyed(&brain_ids, |id| brain.get(id, "/work/alpha")),
        "brainMemoryAfterDelete": keyed(&removed_ids, |id| brain.get(id, "/work/alpha")),
        "brainMemoryHistory": keyed(&brain_history_ids, |id| brain.list_history(id, "/work/alpha")),
        "brainMemoryPage": page_json(brain.list_page("/work/alpha", 0, Some(2), "", None).expect("brain page")),
        "brainMemorySearch": page_json(brain.list_page("/work/alpha", 0, Some(10), "sqlite electron", None).expect("brain search")),
        "brainMemoryStatus": page_json(brain.list_page("/work/alpha", 0, Some(1), "", Some(BrainMemoryStatus::Active)).expect("brain status page")),
        "brainMemoryRetrieve": brain.retrieve("/work/alpha", "conv-1", "sqlite migrações português", None).expect("brain retrieve"),
        "brainMemoryEquivalent": {
            "active": brain.find_equivalent("/work/alpha", BrainMemoryScope::Workspace, None, "  usar sqlite LOCAL para persistência e migrações.  ").expect("active equivalent"),
            "conversation": brain.find_equivalent("/work/alpha", BrainMemoryScope::Conversation, Some("conv-1"), "prefere RESPOSTAS em português.").expect("conversation equivalent"),
            "outdated": brain.find_equivalent("/work/alpha", BrainMemoryScope::Workspace, None, "O projeto usa Electron.").expect("outdated equivalent"),
        },
        "brainMemoryErrors": brain_errors,
        "brainSafety": brain_safety,
    });

    for key in expected.as_object().expect("object").keys() {
        assert_eq!(
            actual[key], expected[key],
            "`{key}` differs from the TypeScript rows"
        );
    }
}

/// The tables startup reconciliation writes, without timestamps. Mirrors
/// `RECONCILE_TABLES` in `scripts/verify-row-gate.mjs`, which records the TypeScript result.
const RECONCILE_TABLES: [(&str, &str); 10] = [
    (
        "executions",
        "SELECT id, status, decision, error FROM executions ORDER BY id",
    ),
    (
        "checkpoints",
        "SELECT id, status, error FROM checkpoints ORDER BY id",
    ),
    ("changes", "SELECT id, status FROM changes ORDER BY id"),
    (
        "change_sets",
        "SELECT id, status FROM change_sets ORDER BY id",
    ),
    (
        "change_decision_operations",
        "SELECT id, status, error FROM change_decision_operations ORDER BY id",
    ),
    (
        "execution_commands",
        "SELECT id, status, output_summary FROM execution_commands ORDER BY id",
    ),
    (
        "semantic_units",
        "SELECT id, status FROM semantic_units ORDER BY id",
    ),
    (
        "project_index_runs",
        "SELECT id, status, error FROM project_index_runs ORDER BY id",
    ),
    (
        "semantic_index_runs",
        "SELECT id, status, error FROM semantic_index_runs ORDER BY id",
    ),
    (
        "interrupted_operations",
        "SELECT id, workspace, execution_id, kind, source_id, original_status, message \
         FROM interrupted_operations ORDER BY id",
    ),
];

/// A stored value as JSON. Blobs are not used by the schema, so they read as null.
/// The error text a call returns, or null when it succeeds, as the TypeScript side records it.
fn error_of<T>(result: Result<T>) -> Value {
    match result {
        Ok(_) => Value::Null,
        Err(error) => json!(error.to_string()),
    }
}

fn status_update(status: BrainMemoryStatus) -> UpdateBrainMemoryInput {
    UpdateBrainMemoryInput {
        status: Some(status),
        ..Default::default()
    }
}

fn brain_input(
    scope: BrainMemoryScope,
    conversation: Option<&str>,
    content: &str,
) -> CreateBrainMemoryInput {
    CreateBrainMemoryInput {
        conversation_id: conversation.map(str::to_string),
        kind: BrainMemoryKind::Fact,
        scope,
        content: content.to_string(),
        confidence: None,
        source_type: None,
        source_id: None,
        status: None,
    }
}

fn value_of(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => json!(value),
        ValueRef::Text(text) => json!(std::str::from_utf8(text).expect("UTF-8 text")),
        ValueRef::Blob(_) => Value::Null,
    }
}

/// The rows of a query as objects keyed by column name.
fn rows_json(conn: &Connection, sql: &str) -> Value {
    let mut statement = conn.prepare(sql).expect("query");
    let names: Vec<String> = statement.column_names().map(str::to_string).collect();
    let rows = statement
        .query_map([], |row| {
            let mut object = Map::new();
            for (index, name) in names.iter().enumerate() {
                object.insert(name.clone(), value_of(row.get_ref(index)?));
            }
            Ok(Value::Object(object))
        })
        .expect("rows")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("rows");
    Value::Array(rows)
}

/// The keys of an object in the expected file, in order.
fn keys(expected: &Value, key: &str) -> Vec<String> {
    expected[key]
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect()
}

fn titles(items: &[ReviewComparisonItem]) -> Value {
    json!(
        items
            .iter()
            .map(|item| item.title.clone())
            .collect::<Vec<_>>()
    )
}

fn reconcile_snapshot(conn: &Connection) -> Value {
    let mut tables = Map::new();
    for (table, sql) in RECONCILE_TABLES {
        let mut statement = conn.prepare(sql).expect("snapshot query");
        let columns = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                let mut values = Vec::with_capacity(columns);
                for index in 0..columns {
                    values.push(value_of(row.get_ref(index)?));
                }
                Ok(Value::Array(values))
            })
            .expect("snapshot rows")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("snapshot rows");
        tables.insert(table.to_string(), Value::Array(rows));
    }
    Value::Object(tables)
}

#[test]
#[ignore = "needs NOCT_ROW_GATE_DIR; run node scripts/verify-row-gate.mjs"]
fn reconciles_like_the_typescript_repository() {
    let directory = scratch("row-gate-reconcile");
    fs::create_dir_all(&directory).expect("scratch directory");
    fs::copy(
        gate_directory().join("reconcile-before.db"),
        directory.join("nocturne.db"),
    )
    .expect("copy the database saved before reconciliation");
    let db = Database::open(&directory).expect("open the copy");

    OperationRecovery::new(db.connection())
        .reconcile()
        .expect("reconcile");

    let text = fs::read_to_string(gate_directory().join("reconcile-expected.json"))
        .expect("reconcile-expected.json");
    let expected: Value = serde_json::from_str(&text).expect("reconcile-expected.json is JSON");
    assert_eq!(reconcile_snapshot(db.connection()), expected);
}

/// A stale mark records the clock of the run that made it, so the value is masked and
/// only its presence is compared. Every other field is compared exactly.
fn mask_stale_clock(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(mask_stale_clock),
        Value::Object(map) => {
            if let Some(stamp) = map.get_mut("staleDetectedAt")
                && !stamp.is_null()
            {
                *stamp = json!("<clock>");
            }
            map.values_mut().for_each(mask_stale_clock);
        }
        _ => {}
    }
}

#[test]
#[ignore = "needs NOCT_ROW_GATE_DIR; run node scripts/verify-row-gate.mjs"]
fn evidence_matches_the_typescript_repository() {
    let directory = scratch("row-gate-evidence");
    fs::create_dir_all(&directory).expect("scratch directory");
    fs::copy(
        gate_directory().join("evidence-before.db"),
        directory.join("nocturne.db"),
    )
    .expect("copy the database saved before the stale mark");
    let db = Database::open(&directory).expect("open the copy");
    let evidence = WorkspaceEvidence::new(db.connection());

    let text = fs::read_to_string(gate_directory().join("evidence-expected.json"))
        .expect("evidence-expected.json");
    let expected: Value = serde_json::from_str(&text).expect("evidence-expected.json is JSON");
    let parent_id = expected["parentId"].as_str().expect("parent id");
    let reason = expected["reason"].as_str().expect("reason");
    evidence.mark_stale(parent_id, reason).expect("mark stale");

    let mut listed = json!(evidence.list("/work/alpha", None).expect("list"));
    let mut by_source = json!(
        evidence
            .by_source(EvidenceKind::ProjectIndex, "pix-gate")
            .expect("by source")
    );
    let references: Vec<EvidenceReference> = evidence
        .index_references("/work/alpha")
        .expect("index references");
    let mut expected_list = expected["list"].clone();
    let mut expected_by_source = expected["bySource"].clone();
    for value in [
        &mut listed,
        &mut by_source,
        &mut expected_list,
        &mut expected_by_source,
    ] {
        mask_stale_clock(value);
    }
    assert_eq!(listed, expected_list, "list differs");
    assert_eq!(by_source, expected_by_source, "bySource differs");
    assert_eq!(
        json!(references),
        expected["indexReferences"],
        "indexReferences differs"
    );
}

#[test]
#[ignore = "needs NOCT_ROW_GATE_DIR; run node scripts/verify-row-gate.mjs"]
fn reconciles_suggestions_like_the_typescript_repository() {
    let directory = scratch("row-gate-suggestions");
    fs::create_dir_all(&directory).expect("scratch directory");
    fs::copy(
        gate_directory().join("suggestions-before.db"),
        directory.join("nocturne.db"),
    )
    .expect("copy the database saved before the review");
    let db = Database::open(&directory).expect("open the copy");
    let conn = db.connection();

    let text = fs::read_to_string(gate_directory().join("suggestion-expected.json"))
        .expect("suggestion-expected.json");
    let expected: Value = serde_json::from_str(&text).expect("suggestion-expected.json is JSON");
    let review: Vec<SuggestionInput> =
        serde_json::from_value(expected["review"].clone()).expect("review input");

    let result = Suggestions::new(conn)
        .reconcile("conv-1", "/work/alpha", &review, true)
        .expect("reconcile");
    let comparison = &result.comparison;

    // Identifiers and the review clock differ per run, so they are not in the snapshot.
    let actual = json!({
        "rows": rows_json(conn, "SELECT title, status, severity, confidence, source, responsible, result FROM suggestions WHERE conversation_id='conv-1' ORDER BY title"),
        "decisions": rows_json(conn, "SELECT s.title AS title, d.status AS status, d.result AS result FROM suggestion_decisions d JOIN suggestions s ON s.id = d.suggestion_id WHERE s.conversation_id='conv-1' ORDER BY s.title, d.status, d.result"),
        "returned": result.suggestions.iter().map(|item| json!({
            "title": item.title,
            "status": serde_json::to_value(item.status).expect("status"),
            "severity": serde_json::to_value(item.severity).expect("severity"),
            "historyStatuses": item.history.iter()
                .map(|entry| serde_json::to_value(entry.status).expect("status"))
                .collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "comparison": {
            "newSuggestions": titles(&comparison.new_suggestions),
            "persistentSuggestions": titles(&comparison.persistent_suggestions),
            "resolvedSuggestions": titles(&comparison.resolved_suggestions),
            "severityChanges": comparison.severity_changes.iter().map(|change| json!({
                "title": change.title,
                "from": serde_json::to_value(change.from).expect("from"),
                "to": serde_json::to_value(change.to).expect("to"),
            })).collect::<Vec<_>>(),
        },
    });
    assert_eq!(actual, expected["snapshot"]);
}
