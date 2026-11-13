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
use noct_db::repositories::settings::Settings;
use noct_db::repositories::workspace_memory::WorkspaceMemory;
use noct_db::repositories::workspace_model_bindings::ModelBindings;
use noct_db::repositories::workspaces::Workspaces;
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
    });

    for key in expected.as_object().expect("object").keys() {
        assert_eq!(
            actual[key], expected[key],
            "`{key}` differs from the TypeScript rows"
        );
    }
}
