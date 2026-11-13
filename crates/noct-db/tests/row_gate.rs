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
use noct_core::code_intelligence::ProjectIndexFile;
use noct_core::engineering_intelligence::{
    ENGINEERING_HEALTH_CATEGORIES, EngineeringEvidence, EngineeringEvidenceSource,
    EngineeringHealthCategory, EngineeringHealthSnapshot, EngineeringInsight, EngineeringMetric,
    EngineeringMetricValue, EngineeringSignal, EngineeringSignalSeverity,
    EngineeringSignalSnapshotState, EngineeringSignalSource, EngineeringSignalStatus,
    EngineeringSourceRuns, HealthAssessmentStatus, HealthCategoryResult, HealthEvidenceCoverage,
};
use noct_core::semantic_index::{
    SemanticEmbeddingSpace, SemanticIndexRun, SemanticRunKind, SemanticRunStatus,
    SemanticSearchFilters, SemanticUnit, SemanticUnitKind, SemanticUnitLocation,
    SemanticUnitStatus,
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
use noct_db::repositories::engineering_intelligence::{EngineeringIntelligence, EvaluationOptions};
use noct_db::repositories::execution_evidence::ExecutionEvidence;
use noct_db::repositories::executions::Executions;
use noct_db::repositories::model_catalog::ModelCatalog;
use noct_db::repositories::operation_recovery::OperationRecovery;
use noct_db::repositories::project_index::ProjectIndex;
use noct_db::repositories::provider_configurations::ProviderConfigurations;
use noct_db::repositories::semantic_index::{
    PersistedSemanticUnit, SemanticIndex, SemanticLexicalMatch,
};
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

    let project = ProjectIndex::new(conn);
    let project_file_paths = keys(&expected, "projectFileById");
    let semantic = SemanticIndex::new(conn);
    let engineering = EngineeringIntelligence::new(conn);
    let semantic_ids = keys(&expected, "semanticUnitById");
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
        "projectRuns": project.list_runs("/work/alpha", None).expect("project runs"),
        "projectRunsLimited": project.list_runs("/work/alpha", Some(1)).expect("limited runs"),
        "projectRunLatest": project.latest_run("/work/alpha").expect("latest run"),
        "projectFiles": project.list_files("/work/alpha", None).expect("project files"),
        "projectFilesLimited": project.list_files("/work/alpha", Some(2)).expect("limited files"),
        "projectFileById": keyed(&project_file_paths, |path| project.get_file("/work/alpha", path)),
        "projectExclusions": project.list_exclusions("/work/alpha").expect("exclusions"),
        "projectStackEvidence": project.list_stack_evidence("/work/alpha").expect("stack evidence"),
        "projectSymbols": {
            "all": project.list_symbols("/work/alpha", "", None).expect("symbols"),
            "query": project.list_symbols("/work/alpha", "  RUN  ", None).expect("symbol query"),
            "limited": project.list_symbols("/work/alpha", "", Some(2)).expect("limited symbols"),
        },
        "projectSymbolsForFile": {
            "app": project.list_symbols_for_file("/work/alpha", "src/app.ts", None).expect("file symbols"),
            "missing": project.list_symbols_for_file("/work/alpha", "src/missing.ts", None).expect("missing file symbols"),
        },
        "projectImports": {
            "all": project.list_imports("/work/alpha", None).expect("imports"),
            "app": project.list_imports("/work/alpha", Some("src/app.ts")).expect("file imports"),
        },
        "projectExports": {
            "all": project.list_exports("/work/alpha", None).expect("exports"),
            "util": project.list_exports("/work/alpha", Some("src/util.ts")).expect("file exports"),
        },
        "projectSummary": project.summary("/work/alpha").expect("summary"),
        "semanticRunLatest": semantic.latest_run("/work/alpha").expect("latest semantic run"),
        "semanticSummary": semantic.summary("/work/alpha").expect("semantic summary"),
        "semanticUnitById": keyed(&semantic_ids, |id| semantic.get_unit("/work/alpha", id)),
        "semanticUnits": semantic.list_units("/work/alpha", &SemanticSearchFilters::default(), None).expect("semantic units"),
        "semanticUnitsFiltered": semantic.list_units("/work/alpha", &SemanticSearchFilters {
            paths: vec!["src/app.ts".to_string()],
            kinds: vec![SemanticUnitKind::Symbol],
            symbols: vec!["main".to_string(), "helper".to_string()],
            ..Default::default()
        }, None).expect("filtered semantic units"),
        "semanticUnitsLimited": semantic.list_units("/work/alpha", &SemanticSearchFilters::default(), Some(2)).expect("limited semantic units"),
        "semanticFileUnits": semantic.list_file_units("/work/alpha", "src/app.ts").expect("file semantic units"),
        "semanticEmbeddings": semantic.list_indexed_embeddings("/work/alpha", &semantic_space(), &SemanticSearchFilters::default()).expect("embeddings"),
        "semanticEmbeddingsVersioned": semantic.list_indexed_embeddings("/work/alpha", &SemanticEmbeddingSpace {
            model_version: Some("2".to_string()),
            dimensions: 2,
            ..semantic_space()
        }, &SemanticSearchFilters::default()).expect("versioned embeddings"),
        "semanticLexical": {
            "helper": ranked(semantic.search_lexical("/work/alpha", "helper", &SemanticSearchFilters::default(), None)),
            "phrase": ranked(semantic.search_lexical("/work/alpha", "  módulo  principal ", &SemanticSearchFilters::default(), None)),
            "filtered": ranked(semantic.search_lexical("/work/alpha", "helper", &SemanticSearchFilters {
                paths: vec!["src/util.ts".to_string()],
                ..Default::default()
            }, None)),
            "limited": ranked(semantic.search_lexical("/work/alpha", "helper", &SemanticSearchFilters::default(), Some(1))),
            "failed": ranked(semantic.search_lexical("/work/alpha", "texto", &SemanticSearchFilters::default(), None)),
            "blank": ranked(semantic.search_lexical("/work/alpha", "   ", &SemanticSearchFilters::default(), None)),
        },
        "engineeringSignals": engineering.list_signals(ENGINEERING_WORKSPACE, None, None).expect("engineering signals"),
        "engineeringActiveSignals": engineering.list_signals(ENGINEERING_WORKSPACE, Some(EngineeringSignalStatus::Active), None).expect("active engineering signals"),
        "engineeringResolvedSignals": engineering.list_signals(ENGINEERING_WORKSPACE, Some(EngineeringSignalStatus::Resolved), Some(1)).expect("resolved engineering signals"),
        "engineeringSignalByFingerprint": engineering.get_signal(ENGINEERING_WORKSPACE, &fingerprint('b')).expect("engineering signal"),
        "engineeringMissingSignal": engineering.get_signal(ENGINEERING_WORKSPACE, &fingerprint('f')).expect("missing engineering signal"),
        "engineeringSnapshots": engineering.list_snapshots(ENGINEERING_WORKSPACE, None).expect("engineering snapshots"),
        "engineeringSnapshotsLimited": engineering.list_snapshots(ENGINEERING_WORKSPACE, Some(1)).expect("limited engineering snapshots"),
        "engineeringLatestSnapshot": engineering.latest_snapshot(ENGINEERING_WORKSPACE).expect("latest engineering snapshot"),
        "engineeringInsights": engineering.list_insights(ENGINEERING_WORKSPACE, None, None).expect("engineering insights"),
        "engineeringResolvedInsights": engineering.list_insights(ENGINEERING_WORKSPACE, Some(EngineeringSignalStatus::Resolved), None).expect("resolved engineering insights"),
    });

    for key in expected.as_object().expect("object").keys() {
        assert_eq!(
            numbers_as_integers(&actual[key]),
            numbers_as_integers(&expected[key]),
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
/// JSON has one number type. A whole float is compared as an integer, because a REAL column
/// serializes as `1000.0` in Rust and as `1000` in the TypeScript output of the same value.
fn numbers_as_integers(value: &Value) -> Value {
    match value {
        Value::Number(number) if number.is_f64() => match number.as_f64() {
            Some(float) if float.fract() == 0.0 && float.abs() < 9.0e15 => json!(float as i64),
            _ => value.clone(),
        },
        Value::Array(items) => Value::Array(items.iter().map(numbers_as_integers).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), numbers_as_integers(item)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

/// Lexical matches as the gate compares them. The rank keeps 12 significant digits, as the
/// TypeScript side does: SQLite computes it, and the two builds differ in its last bit.
fn ranked(matches: Result<Vec<SemanticLexicalMatch>>) -> Value {
    let matches = matches.expect("lexical search");
    Value::Array(
        matches
            .into_iter()
            .map(|found| {
                json!({
                    "unit": found.unit,
                    "rank": format!("{:.11e}", found.rank).parse::<f64>().expect("rank"),
                })
            })
            .collect(),
    )
}

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

#[test]
#[ignore = "needs NOCT_ROW_GATE_DIR; run node scripts/verify-row-gate.mjs"]
fn removes_changed_files_like_the_typescript_repository() {
    let directory = scratch("row-gate-change");
    fs::create_dir_all(&directory).expect("scratch directory");
    fs::copy(
        gate_directory().join("project-before.db"),
        directory.join("nocturne.db"),
    )
    .expect("copy the database saved before the change");
    let db = Database::open(&directory).expect("open the copy");
    let conn = db.connection();

    let text = fs::read_to_string(gate_directory().join("change-expected.json"))
        .expect("change-expected.json");
    let expected: Value = serde_json::from_str(&text).expect("change-expected.json is JSON");
    let project = ProjectIndex::new(conn);
    for file in expected["files"].as_array().expect("files") {
        let file: ProjectIndexFile = serde_json::from_value(file.clone()).expect("file record");
        project
            .upsert_file(&file)
            .expect("upsert the scenario file");
    }
    let requested: Vec<String> =
        serde_json::from_value(expected["requested"].clone()).expect("requested paths");
    let present: Vec<String> =
        serde_json::from_value(expected["present"].clone()).expect("present paths");
    project
        .remove_missing_for_change("/work/alpha", &requested, &present)
        .expect("remove missing files");

    let mut statement = conn
        .prepare("SELECT relative_path FROM project_index_files WHERE workspace=?1 ORDER BY relative_path")
        .expect("prepare");
    let remaining: Vec<String> = statement
        .query_map(["/work/alpha"], |row| row.get(0))
        .expect("query")
        .collect::<rusqlite::Result<_>>()
        .expect("rows");
    assert_eq!(json!(remaining), expected["remaining"]);
}

/// The queries that read the state of the semantic write scenario. Mirrors `SEMANTIC_STATE` in
/// `scripts/verify-row-gate.mjs`, which records the state the TypeScript repository leaves.
const SEMANTIC_STATE: [(&str, &str); 4] = [
    (
        "runs",
        "SELECT id, workspace, kind, status, total_files, processed_files, indexed_units, lexical_only_units, failed_files, excluded_files, started_at, completed_at, error FROM semantic_index_runs ORDER BY id",
    ),
    (
        "units",
        "SELECT id, relative_path, kind, language, symbol_name, status, error, start_line, end_line, start_offset, end_offset, normalized_text, created_at, indexed_at, embedding_provider_id, embedding_model_id, embedding_model_version, embedding_dimensions, hex(embedding) AS embedding FROM semantic_units WHERE workspace='/work/alpha' ORDER BY id",
    ),
    (
        "searchable",
        "SELECT semantic_units.id FROM semantic_units_fts JOIN semantic_units ON semantic_units.rowid=semantic_units_fts.rowid WHERE semantic_units_fts MATCH 'changed OR gamma OR alpha' ORDER BY semantic_units.id",
    ),
    (
        "evidence",
        "SELECT kind, source_id FROM workspace_evidence WHERE workspace='/work/alpha' AND kind='semantic-index' ORDER BY source_id",
    ),
];

/// The timestamp `at(second)` gives in the gate script: 12:00:00 UTC plus the seconds, rolled over
/// the way `Date.UTC` rolls them over.
fn at(second: u64) -> String {
    let total = 12 * 3600 + second;
    format!(
        "2026-01-01T{:02}:{:02}:{:02}.000Z",
        (total / 3600) % 24,
        (total / 60) % 60,
        total % 60
    )
}

/// The embedding space of the semantic fixtures in `scripts/verify-row-gate.mjs`.
fn semantic_space() -> SemanticEmbeddingSpace {
    SemanticEmbeddingSpace {
        provider_id: "local".to_string(),
        model_id: "mini".to_string(),
        model_version: None,
        dimensions: 3,
    }
}

/// A unit with the defaults `semanticUnit` gives in the gate script, in `src/app.ts`.
fn fixture_unit(id: &str, status: SemanticUnitStatus, text: &str, created: &str) -> SemanticUnit {
    SemanticUnit {
        id: id.to_string(),
        workspace: "/work/alpha".to_string(),
        relative_path: "src/app.ts".to_string(),
        kind: SemanticUnitKind::Symbol,
        language: Some("TypeScript".to_string()),
        symbol_id: None,
        symbol_name: None,
        location: SemanticUnitLocation {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 10,
            start_offset: None,
            end_offset: None,
        },
        source_hash: "a".repeat(64),
        chunk_hash: "b".repeat(64),
        normalized_text: text.to_string(),
        chunk_strategy_version: "symbols-v2".to_string(),
        embedding_space: None,
        status,
        error: None,
        created_at: created.to_string(),
        updated_at: created.to_string(),
        indexed_at: None,
    }
}

fn persisted(unit: SemanticUnit, embedding: Option<Vec<f64>>) -> PersistedSemanticUnit {
    PersistedSemanticUnit { unit, embedding }
}

/// The write scenario of `scripts/verify-row-gate.mjs`, replayed on the copy saved before it with
/// the same steps and values. The state it leaves must equal the one the TypeScript repository
/// left, and the refused replacement must have changed nothing.
#[test]
#[ignore = "needs NOCT_ROW_GATE_DIR; run node scripts/verify-row-gate.mjs"]
fn applies_semantic_writes_like_the_typescript_repository() {
    const ALPHA: &str = "/work/alpha";
    const APP: &str = "src/app.ts";
    let directory = scratch("row-gate-semantic");
    fs::create_dir_all(&directory).expect("scratch directory");
    fs::copy(
        gate_directory().join("semantic-before.db"),
        directory.join("nocturne.db"),
    )
    .expect("copy the database saved before the writes");
    let db = Database::open(&directory).expect("open the copy");
    let conn = db.connection();
    let index = SemanticIndex::new(conn);

    index
        .create_run(&SemanticIndexRun {
            id: "sem-w1".to_string(),
            workspace: ALPHA.to_string(),
            kind: SemanticRunKind::Incremental,
            status: SemanticRunStatus::Running,
            total_files: 0,
            processed_files: 0,
            indexed_units: 0,
            lexical_only_units: 0,
            failed_files: 0,
            excluded_files: 0,
            started_at: at(700),
            completed_at: None,
            updated_at: at(700),
            error: None,
        })
        .expect("create run");

    let mut w_a = fixture_unit("w-a", SemanticUnitStatus::Indexed, "alpha text", &at(701));
    w_a.location = SemanticUnitLocation {
        start_line: 1,
        start_column: 1,
        end_line: 2,
        end_column: 4,
        start_offset: Some(0),
        end_offset: Some(20),
    };
    w_a.indexed_at = Some(at(701));
    w_a.embedding_space = Some(semantic_space());
    let mut w_b = fixture_unit(
        "w-b",
        SemanticUnitStatus::LexicalOnly,
        "beta text",
        &at(701),
    );
    w_b.kind = SemanticUnitKind::Text;
    w_b.language = None;
    index
        .replace_file_units(
            ALPHA,
            APP,
            &[
                persisted(w_a, Some(vec![0.5, -0.25, 1.5])),
                persisted(w_b, None),
            ],
        )
        .expect("first replacement");

    let mut changed = fixture_unit(
        "w-b",
        SemanticUnitStatus::LexicalOnly,
        "beta changed",
        &at(701),
    );
    changed.kind = SemanticUnitKind::Text;
    changed.language = None;
    changed.updated_at = at(702);
    index
        .replace_file_units(
            ALPHA,
            APP,
            &[
                persisted(changed, None),
                persisted(
                    fixture_unit("w-c", SemanticUnitStatus::Pending, "gamma", &at(702)),
                    None,
                ),
            ],
        )
        .expect("second replacement");

    let mut w_d = fixture_unit("w-d", SemanticUnitStatus::Indexed, "delta", &at(703));
    w_d.indexed_at = Some(at(703));
    w_d.embedding_space = Some(semantic_space());
    let mut w_e = fixture_unit("w-e", SemanticUnitStatus::Indexed, "epsilon", &at(703));
    w_e.embedding_space = Some(semantic_space());
    let refused = index.replace_file_units(
        ALPHA,
        APP,
        &[
            persisted(w_d, Some(vec![1.0, 2.0, 3.0])),
            persisted(w_e, Some(Vec::new())),
        ],
    );

    index
        .replace_file_units(ALPHA, "src/util.ts", &[])
        .expect("clear the units of the util file");
    index
        .mark_file_stale(ALPHA, APP, None)
        .expect("mark the file stale");
    index
        .delete_file(ALPHA, "src/broken.ts")
        .expect("delete the broken file");
    index
        .update_run(&SemanticIndexRun {
            id: "sem-w1".to_string(),
            workspace: ALPHA.to_string(),
            kind: SemanticRunKind::Incremental,
            status: SemanticRunStatus::Completed,
            total_files: 2,
            processed_files: 2,
            indexed_units: 0,
            lexical_only_units: 1,
            failed_files: 0,
            excluded_files: 0,
            started_at: at(700),
            completed_at: Some(at(702)),
            updated_at: at(702),
            error: None,
        })
        .expect("complete the run");

    let mut actual = Map::new();
    for (name, sql) in SEMANTIC_STATE {
        actual.insert(name.to_string(), rows_json(conn, sql));
    }
    actual.insert(
        "errors".to_string(),
        json!({ "invalidEmbedding": error_of(refused) }),
    );
    let text = fs::read_to_string(gate_directory().join("semantic-write-expected.json"))
        .expect("semantic-write-expected.json");
    let expected: Value =
        serde_json::from_str(&text).expect("semantic-write-expected.json is JSON");
    assert_eq!(Value::Object(actual), expected);
}

/// The workspace of the engineering fixtures in `scripts/verify-row-gate.mjs`.
const ENGINEERING_WORKSPACE: &str = "/work/alpha";

fn fingerprint(character: char) -> String {
    character.to_string().repeat(64)
}

fn engineering_evidence() -> EngineeringEvidence {
    EngineeringEvidence {
        id: "eng-ev-app".to_string(),
        source: EngineeringEvidenceSource::ProjectIndexFile,
        source_id: "project-file-app".to_string(),
        relative_path: Some("src/app.ts".to_string()),
        source_hash: Some(fingerprint('a')),
        run_id: None,
        location: None,
        validity: None,
        detail: "Arquivo indexado pelo Project Index.".to_string(),
        observed_at: at(620),
    }
}

/// The architecture signal of the write scenario, after its severity and sighting changed.
fn updated_architecture() -> EngineeringSignal {
    EngineeringSignal {
        id: "eng-sig-w-architecture".to_string(),
        workspace: ENGINEERING_WORKSPACE.to_string(),
        category: EngineeringHealthCategory::Architecture,
        kind: "coupling".to_string(),
        title: "Acoplamento alto".to_string(),
        description: "O módulo depende de muitos arquivos.".to_string(),
        severity: EngineeringSignalSeverity::Low,
        confidence: 90,
        evidence: vec![engineering_evidence()],
        metric: Some(EngineeringMetric {
            name: "affectedFiles".to_string(),
            value: EngineeringMetricValue::Number(2.0),
            unit: Some("files".to_string()),
        }),
        source: EngineeringSignalSource::Deterministic,
        policy_version: 2,
        detected_at: at(650),
        fingerprint: fingerprint('a'),
        status: EngineeringSignalStatus::Active,
        first_seen_at: at(650),
        last_seen_at: at(650),
        resolved_at: None,
    }
}

/// The security signal that the third evaluation adds.
fn security_signal() -> EngineeringSignal {
    EngineeringSignal {
        id: "eng-sig-security".to_string(),
        workspace: ENGINEERING_WORKSPACE.to_string(),
        category: EngineeringHealthCategory::Security,
        kind: "dependency-audit".to_string(),
        title: "Dependência com alerta".to_string(),
        description: "Um alerta de segurança exige revisão.".to_string(),
        severity: EngineeringSignalSeverity::Critical,
        confidence: 95,
        evidence: vec![engineering_evidence()],
        metric: Some(EngineeringMetric {
            name: "advisory".to_string(),
            value: EngineeringMetricValue::Text("GHSA-exemplo".to_string()),
            unit: None,
        }),
        source: EngineeringSignalSource::Deterministic,
        policy_version: 2,
        detected_at: at(650),
        fingerprint: fingerprint('e'),
        status: EngineeringSignalStatus::Active,
        first_seen_at: at(650),
        last_seen_at: at(651),
        resolved_at: None,
    }
}

/// `engineeringSnapshot` of the gate script: every category not assessed, unless it is named.
fn engineering_snapshot(id: &str, evaluated_at: &str, state: char) -> EngineeringHealthSnapshot {
    EngineeringHealthSnapshot {
        id: id.to_string(),
        workspace: ENGINEERING_WORKSPACE.to_string(),
        policy_version: 2,
        evaluated_at: evaluated_at.to_string(),
        previous_snapshot_id: None,
        sources: EngineeringSourceRuns {
            project_index_run_id: Some("project-run-1".to_string()),
            semantic_index_run_id: Some("sem-g1".to_string()),
            validation_run_ids: Vec::new(),
            execution_ids: Vec::new(),
            change_set_ids: Vec::new(),
        },
        categories: ENGINEERING_HEALTH_CATEGORIES
            .into_iter()
            .map(|category| HealthCategoryResult {
                category,
                status: HealthAssessmentStatus::NotAssessed,
                score: None,
                coverage: HealthEvidenceCoverage {
                    available: 0,
                    expected: None,
                    percent: None,
                },
                signal_ids: Vec::new(),
                evidence: Vec::new(),
                evaluated_at: evaluated_at.to_string(),
            })
            .collect(),
        signal_fingerprints: Vec::new(),
        signal_states: Vec::new(),
        state_fingerprint: fingerprint(state),
    }
}

/// Marks a category as assessed with its score and coverage, as the gate script does.
fn assess(
    snapshot: &mut EngineeringHealthSnapshot,
    category: EngineeringHealthCategory,
    score: f64,
    coverage: HealthEvidenceCoverage,
) {
    let result = snapshot
        .categories
        .iter_mut()
        .find(|result| result.category == category)
        .expect("category");
    result.status = HealthAssessmentStatus::Assessed;
    result.score = Some(score);
    result.coverage = coverage;
}

/// The third evaluation of the write scenario: architecture and security assessed, with its sources.
fn third_snapshot(id: &str, evaluated_at: &str) -> EngineeringHealthSnapshot {
    let mut snapshot = engineering_snapshot(id, evaluated_at, 'd');
    snapshot.previous_snapshot_id = Some("eng-snap-2".to_string());
    snapshot.sources = EngineeringSourceRuns {
        project_index_run_id: Some("project-run-2".to_string()),
        semantic_index_run_id: None,
        validation_run_ids: vec!["validation-1".to_string()],
        execution_ids: vec!["execution-1".to_string()],
        change_set_ids: vec!["change-set-1".to_string()],
    };
    assess(
        &mut snapshot,
        EngineeringHealthCategory::Architecture,
        70.0,
        HealthEvidenceCoverage {
            available: 3,
            expected: Some(4),
            percent: Some(75.0),
        },
    );
    assess(
        &mut snapshot,
        EngineeringHealthCategory::Security,
        40.0,
        HealthEvidenceCoverage {
            available: 1,
            expected: Some(1),
            percent: Some(100.0),
        },
    );
    snapshot.signal_fingerprints = vec![fingerprint('a'), fingerprint('e')];
    snapshot.signal_states = vec![
        EngineeringSignalSnapshotState {
            fingerprint: fingerprint('a'),
            category: EngineeringHealthCategory::Architecture,
            severity: EngineeringSignalSeverity::Low,
            status: EngineeringSignalStatus::Active,
        },
        EngineeringSignalSnapshotState {
            fingerprint: fingerprint('e'),
            category: EngineeringHealthCategory::Security,
            severity: EngineeringSignalSeverity::Critical,
            status: EngineeringSignalStatus::Active,
        },
    ];
    snapshot
}

fn revised_insight() -> EngineeringInsight {
    EngineeringInsight {
        id: "eng-ins-3".to_string(),
        workspace: ENGINEERING_WORKSPACE.to_string(),
        fingerprint: fingerprint('c'),
        title: "Acoplamento revisado".to_string(),
        explanation: "Explicação revisada.".to_string(),
        related_signal_ids: vec!["eng-sig-architecture".to_string()],
        evidence: vec![engineering_evidence()],
        suggested_action: Some("Dividir o módulo.".to_string()),
        confidence: 85,
        status: EngineeringSignalStatus::Active,
        created_at: at(670),
        updated_at: at(670),
        last_seen_at: at(670),
    }
}

/// The write scenario of the engineering intelligence. Mirrors `engineeringWriteParity` in
/// `scripts/verify-row-gate.mjs`, which records the TypeScript state.
#[test]
#[ignore = "needs NOCT_ROW_GATE_DIR; run node scripts/verify-row-gate.mjs"]
fn applies_engineering_writes_like_the_typescript_repository() {
    let directory = scratch("row-gate-engineering");
    fs::create_dir_all(&directory).expect("scratch directory");
    fs::copy(
        gate_directory().join("engineering-before.db"),
        directory.join("nocturne.db"),
    )
    .expect("copy the database saved before the writes");
    let db = Database::open(&directory).expect("open the copy");
    let conn = db.connection();
    let store = EngineeringIntelligence::new(conn);

    let saved = store
        .save_evaluation(
            &[updated_architecture(), security_signal()],
            &third_snapshot("eng-snap-3", &at(650)),
            &EvaluationOptions {
                skip_category_resolution: vec![EngineeringHealthCategory::Testing],
                resolve_signal_fingerprints: vec![fingerprint('b')],
            },
        )
        .expect("third evaluation");
    let duplicate = store
        .save_evaluation(
            &[updated_architecture(), security_signal()],
            &third_snapshot("eng-snap-4", &at(660)),
            &EvaluationOptions::default(),
        )
        .expect("duplicate evaluation");
    let refused = store
        .save_evaluation(
            &[EngineeringSignal {
                confidence: 101,
                ..updated_architecture()
            }],
            &engineering_snapshot("eng-snap-5", &at(670), 'e'),
            &EvaluationOptions::default(),
        )
        .is_err();
    let saved_insight = store
        .save_insight(&revised_insight())
        .expect("revised insight");
    let resolved_insight = store
        .resolve_insight(ENGINEERING_WORKSPACE, "eng-ins-1", &at(675))
        .expect("resolve insight");
    let resolved_again = store
        .resolve_insight(ENGINEERING_WORKSPACE, "eng-ins-1", &at(676))
        .expect("resolve again");

    let mut actual = Map::new();
    actual.insert(
        "signals".to_string(),
        json!(
            store
                .list_signals(ENGINEERING_WORKSPACE, None, None)
                .expect("signals")
        ),
    );
    actual.insert(
        "resolvedSignals".to_string(),
        json!(
            store
                .list_signals(
                    ENGINEERING_WORKSPACE,
                    Some(EngineeringSignalStatus::Resolved),
                    None
                )
                .expect("resolved signals")
        ),
    );
    actual.insert(
        "snapshots".to_string(),
        json!(
            store
                .list_snapshots(ENGINEERING_WORKSPACE, None)
                .expect("snapshots")
        ),
    );
    actual.insert(
        "latestSnapshot".to_string(),
        json!(
            store
                .latest_snapshot(ENGINEERING_WORKSPACE)
                .expect("latest snapshot")
        ),
    );
    actual.insert(
        "insights".to_string(),
        json!(
            store
                .list_insights(ENGINEERING_WORKSPACE, None, None)
                .expect("insights")
        ),
    );
    actual.insert(
        "evidence".to_string(),
        rows_json(
            conn,
            "SELECT source_id AS sourceId, json_extract(manifest_json, '$.references') AS refs \
             FROM workspace_evidence WHERE workspace='/work/alpha' AND kind='engineering' \
             ORDER BY source_id",
        ),
    );
    actual.insert(
        "outcomes".to_string(),
        json!({
            "saved": saved,
            "duplicate": duplicate,
            "refusedSignal": refused,
            "savedInsight": saved_insight,
            "resolvedInsight": resolved_insight,
            "resolvedAgain": resolved_again,
        }),
    );
    let text = fs::read_to_string(gate_directory().join("engineering-write-expected.json"))
        .expect("engineering-write-expected.json");
    let expected: Value =
        serde_json::from_str(&text).expect("engineering-write-expected.json is JSON");
    let actual = numbers_as_integers(&Value::Object(actual));
    let expected = numbers_as_integers(&expected);
    for key in expected.as_object().expect("object").keys() {
        assert_eq!(
            actual[key], expected[key],
            "`{key}` differs from the TypeScript state"
        );
    }
}
