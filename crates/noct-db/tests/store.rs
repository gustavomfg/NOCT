//! The persistence facade, checked against the workflows of `electron/database/Database.ts`:
//! the reconciliation at open, the conversation that opens its workspace, and the assistant
//! turn that is stored whole or not at all.

use noct_core::limits::persistence::METADATA_CHARACTERS;
use noct_db::store::Store;
use noct_db::store::{AssistantTurn, TurnArtifact};
use rusqlite::params;
use serde_json::json;

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";

fn count(store: &Store, sql: &str) -> i64 {
    store
        .connection()
        .query_row(sql, [], |row| row.get(0))
        .expect("count")
}

#[test]
fn opening_records_the_operations_the_last_run_left_running() {
    let directory = scratch("store-reconcile");
    let store = Store::open(&directory).expect("open");
    let conversation = store
        .create_conversation("/work/alpha")
        .expect("conversation")
        .id;
    store
        .connection()
        .execute(
            "INSERT INTO executions(id,workspace,conversation_id,prompt,mode,status,decision,started_at) \
             VALUES('exec-1','/work/alpha',?1,'Tarefa','build','running','pending',?2)",
            params![conversation, AT],
        )
        .expect("running execution");
    store.close().expect("close");

    let reopened = Store::open(&directory).expect("reopen");

    let status: String = reopened
        .connection()
        .query_row(
            "SELECT status FROM executions WHERE id='exec-1'",
            [],
            |row| row.get(0),
        )
        .expect("status");
    assert_eq!(status, "failed");
    assert_eq!(
        count(&reopened, "SELECT COUNT(*) FROM interrupted_operations"),
        1
    );
    reopened.close().expect("close");

    let again = Store::open(&directory).expect("open again");
    assert_eq!(
        count(&again, "SELECT COUNT(*) FROM interrupted_operations"),
        1
    );
}

#[test]
fn creating_a_conversation_opens_its_workspace_first() {
    let directory = scratch("store-conversation");
    let store = Store::open(&directory).expect("open");

    let conversation = store
        .create_conversation("/work/alpha")
        .expect("conversation");

    assert_eq!(conversation.workspace, "/work/alpha");
    let workspaces = store.workspaces().list().expect("workspaces");
    let paths: Vec<&str> = workspaces.iter().map(|item| item.path.as_str()).collect();
    assert_eq!(paths, vec!["/work/alpha"]);
}

#[test]
fn an_assistant_turn_is_stored_with_its_markdown_copy_and_artifacts() {
    let directory = scratch("store-turn");
    let store = Store::open(&directory).expect("open");
    let conversation = store
        .create_conversation("/work/alpha")
        .expect("conversation")
        .id;
    let diagram = json!({ "language": "mermaid" });
    let turn = AssistantTurn {
        conversation_id: &conversation,
        workspace: "/work/alpha",
        content: "Resposta",
        metadata: None,
        markdown_title: "Resposta · 2026",
        artifacts: &[TurnArtifact {
            kind: "diagram",
            title: "Diagrama",
            file_path: None,
            content: Some("graph TD"),
            metadata: Some(&diagram),
        }],
    };

    let message = store.save_assistant_turn(&turn).expect("turn");

    assert_eq!(message.role, "assistant");
    assert_eq!(
        store
            .conversations()
            .list_messages(&conversation)
            .expect("messages")
            .len(),
        1
    );
    let mut titles: Vec<String> = store
        .artifacts()
        .list(&conversation)
        .expect("artifacts")
        .into_iter()
        .map(|artifact| artifact.title)
        .collect();
    titles.sort();
    assert_eq!(titles, vec!["Diagrama", "Resposta · 2026"]);
}

#[test]
fn a_refused_artifact_rolls_back_the_whole_turn() {
    let directory = scratch("store-turn-refused");
    let store = Store::open(&directory).expect("open");
    let conversation = store
        .create_conversation("/work/alpha")
        .expect("conversation")
        .id;
    let oversized = json!({ "text": "a".repeat(METADATA_CHARACTERS as usize + 1) });
    let turn = AssistantTurn {
        conversation_id: &conversation,
        workspace: "/work/alpha",
        content: "Resposta",
        metadata: None,
        markdown_title: "Resposta · 2026",
        artifacts: &[TurnArtifact {
            kind: "diagram",
            title: "Grande",
            file_path: None,
            content: None,
            metadata: Some(&oversized),
        }],
    };

    assert!(store.save_assistant_turn(&turn).is_err());

    assert_eq!(
        store
            .conversations()
            .list_messages(&conversation)
            .expect("messages")
            .len(),
        0
    );
    assert!(
        store
            .artifacts()
            .list(&conversation)
            .expect("artifacts")
            .is_empty()
    );
}
