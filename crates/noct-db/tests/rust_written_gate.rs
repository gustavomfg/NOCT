//! Rust writes, TypeScript reads and writes, Rust reads back. `scripts/verify-rust-written-gate.mjs`
//! runs these tests in order around the compiled TypeScript repositories, in `NOCT_RUST_WRITTEN_DIR`.
//! The tests are ignored by default because they need that directory.
//!
//! The rows are read with the same repositories on both sides and compared as JSON, in the
//! camelCase form that the TypeScript repositories return. The workspace paths are synthetic.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use noct_db::Database;
use noct_db::repositories::conversations::{Conversations, MessageRole};
use noct_db::repositories::settings::Settings;
use noct_db::repositories::workspaces::Workspaces;
use serde_json::{Map, Value, json};

/// The synthetic workspace paths the script expects. They are not real folders.
const WORKSPACE_ALPHA: &str = "/synthetic/alpha";
const WORKSPACE_BETA: &str = "/synthetic/beta";

/// The directory written and read by `scripts/verify-rust-written-gate.mjs`.
fn gate_directory() -> PathBuf {
    PathBuf::from(std::env::var("NOCT_RUST_WRITTEN_DIR").expect(
        "NOCT_RUST_WRITTEN_DIR is not set: run `node scripts/verify-rust-written-gate.mjs`",
    ))
}

/// The rows the TypeScript side reads back, through the Rust repositories.
fn logical_state(db: &Database) -> Value {
    let conn = db.connection();
    let conversations = Conversations::new(conn).list().expect("conversations");
    let mut messages = Map::new();
    for conversation in &conversations {
        let rows = Conversations::new(conn)
            .list_messages(&conversation.id)
            .expect("messages");
        messages.insert(
            conversation.id.clone(),
            serde_json::to_value(rows).expect("json"),
        );
    }
    json!({
        "workspaces": serde_json::to_value(Workspaces::new(conn).list().expect("workspaces")).expect("json"),
        "settings": serde_json::to_value(Settings::new(conn).get().expect("settings")).expect("json"),
        "conversations": serde_json::to_value(conversations).expect("json"),
        "messages": Value::Object(messages),
    })
}

#[test]
#[ignore = "needs NOCT_RUST_WRITTEN_DIR; run node scripts/verify-rust-written-gate.mjs"]
fn rust_writes_the_rows_the_typescript_repositories_read() {
    let directory = gate_directory();
    fs::create_dir_all(&directory).expect("gate directory");
    let db = Database::open_runtime(&directory).expect("create the database with Rust");
    let conn = db.connection();

    let workspaces = Workspaces::new(conn);
    workspaces.touch(WORKSPACE_ALPHA).expect("touch alpha");
    workspaces.touch(WORKSPACE_BETA).expect("touch beta");
    workspaces
        .set_favorite(WORKSPACE_BETA, true)
        .expect("favorite beta");

    let mut values = BTreeMap::new();
    values.insert("theme".to_string(), "light".to_string());
    values.insert("language".to_string(), "en".to_string());
    Settings::new(conn).set(&values).expect("settings");

    let conversations = Conversations::new(conn);
    let first = conversations
        .create(WORKSPACE_ALPHA)
        .expect("first conversation");
    conversations
        .add_message(
            &first.id,
            MessageRole::User,
            "Pergunta escrita pelo Rust.",
            None,
        )
        .expect("user message");
    conversations
        .add_message(
            &first.id,
            MessageRole::Assistant,
            "Resposta com metadados escritos pelo Rust.",
            Some(&json!({ "model": "local", "tokens": 12 })),
        )
        .expect("assistant message");
    conversations
        .create(WORKSPACE_BETA)
        .expect("second conversation");

    let expected = logical_state(&db);
    drop(db);
    fs::write(
        gate_directory().join("rust-written-expected.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&expected).expect("json")
        ),
    )
    .expect("write the expected rows");
}

#[test]
#[ignore = "needs NOCT_RUST_WRITTEN_DIR; run node scripts/verify-rust-written-gate.mjs"]
fn rust_reads_what_the_typescript_repositories_wrote() {
    let directory = gate_directory();
    let text = fs::read_to_string(directory.join("typescript-written-expected.json"))
        .expect("typescript-written-expected.json");
    let expected: Value = serde_json::from_str(&text).expect("the expected rows are JSON");

    let db = Database::open_runtime(&directory).expect("open the database TypeScript wrote to");
    assert_eq!(logical_state(&db), expected);
}
