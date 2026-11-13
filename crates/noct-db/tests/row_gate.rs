//! Row-level gate of phase 2. `scripts/verify-row-gate.mjs` has the TypeScript
//! persistence layer write a database at schema 32 and record the rows its repositories
//! return, in `NOCT_ROW_GATE_DIR`. Rust must open the same file and return the same
//! rows. The test is ignored by default because it needs that directory.

use std::fs;
use std::path::PathBuf;

use noct_db::Database;
use noct_db::Result;
use noct_db::repositories::artifacts::Artifacts;
use noct_db::repositories::conversations::Conversations;
use noct_db::repositories::model_catalog::ModelCatalog;
use noct_db::repositories::operation_recovery::OperationRecovery;
use noct_db::repositories::provider_configurations::ProviderConfigurations;
use noct_db::repositories::settings::Settings;
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

fn reconcile_snapshot(conn: &Connection) -> Value {
    let mut tables = Map::new();
    for (table, sql) in RECONCILE_TABLES {
        let mut statement = conn.prepare(sql).expect("snapshot query");
        let columns = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                let mut values = Vec::with_capacity(columns);
                for index in 0..columns {
                    values.push(match row.get_ref(index)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(value) => json!(value),
                        ValueRef::Real(value) => json!(value),
                        ValueRef::Text(text) => {
                            json!(std::str::from_utf8(text).expect("UTF-8 text"))
                        }
                        ValueRef::Blob(_) => Value::Null,
                    });
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
