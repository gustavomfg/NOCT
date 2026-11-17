//! The persistence facade, checked against the workflows of `electron/database/Database.ts`:
//! the reconciliation at open, the conversation that opens its workspace, and the assistant
//! turn that is stored whole or not at all.

use std::path::Path;

use noct_core::limits::persistence::METADATA_CHARACTERS;
use noct_db::runtime::{Database, OpenOptions};
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

/// Leaves a database whose completed execution has a change set still waiting for a decision.
fn leave_a_change_set_pending(directory: &Path) {
    let store = Store::open(directory).expect("open");
    let conversation = store
        .create_conversation("/work/alpha")
        .expect("conversation")
        .id;
    let connection = store.connection();
    connection
        .execute(
            "INSERT INTO executions(id,workspace,conversation_id,prompt,mode,status,decision,started_at,finished_at) \
             VALUES('exec-1','/work/alpha',?1,'Tarefa','build','completed','pending',?2,?2)",
            params![conversation, AT],
        )
        .expect("completed execution");
    for (id, phase) in [("cp-before", "before"), ("cp-after", "after")] {
        connection
            .execute(
                "INSERT INTO checkpoints(id,execution_id,workspace,phase,status,captured_at,root_path) \
                 VALUES(?1,'exec-1','/work/alpha',?2,'ready',?3,'/work/alpha')",
                params![id, phase, AT],
            )
            .expect("checkpoint");
    }
    connection
        .execute(
            "INSERT INTO change_sets(id,execution_id,before_checkpoint_id,after_checkpoint_id,status,created_at,updated_at) \
             VALUES('cs-1','exec-1','cp-before','cp-after','pending',?1,?1)",
            params![AT],
        )
        .expect("change set");
    store.close().expect("close");
}

/// The public path reconciles a change set still waiting for a decision. The runtime-level
/// open does not: it migrates and maintains the file, nothing more. This is the divergence
/// the parity run found in 1.0.1 and 1.0.2, so the contract is pinned here.
#[test]
fn the_public_open_records_pending_decisions_and_the_runtime_open_does_not() {
    let directory = scratch("store-reconcile-decision");
    leave_a_change_set_pending(&directory);

    let runtime = Database::open_runtime(&directory).expect("runtime open");
    let recorded: i64 = runtime
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM interrupted_operations WHERE kind='awaiting-decision'",
            [],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(recorded, 0, "the runtime open must not reconcile");
    runtime.close().expect("close");

    let public = Store::open(&directory).expect("public open");
    assert_eq!(
        count(
            &public,
            "SELECT COUNT(*) FROM interrupted_operations WHERE kind='awaiting-decision'"
        ),
        1
    );
    public.close().expect("close");
}

#[test]
fn opening_with_options_reconciles_like_the_default_open() {
    let directory = scratch("store-reconcile-options");
    leave_a_change_set_pending(&directory);

    let store = Store::open_with(&directory, OpenOptions::default()).expect("open with options");
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM interrupted_operations WHERE kind='awaiting-decision'"
        ),
        1
    );
    store.close().expect("close");
}

#[test]
fn opening_with_options_refuses_a_newer_schema_and_keeps_the_file() {
    let directory = scratch("store-open-with-newer-schema");
    let path = directory.join("nocturne.db");
    {
        let connection = rusqlite::Connection::open(&path).expect("create the file");
        connection
            .pragma_update(None, "user_version", 33)
            .expect("a newer schema");
    }
    let before = std::fs::read(&path).expect("file bytes");

    let error = Store::open_with(&directory, OpenOptions::default())
        .err()
        .expect("a newer schema is refused");

    assert!(error.to_string().contains("schema 33"), "{error}");
    assert_eq!(std::fs::read(&path).expect("file bytes"), before);
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

#[test]
fn a_refused_conversation_insert_leaves_the_workspace_unsaved() {
    // The workspace row and the conversation are written in one transaction. The trigger refuses the second
    // insert, so the first must be rolled back as well.
    let directory = scratch("store-create-conversation-rollback");
    let store = Store::open(&directory).expect("open");
    store
        .connection()
        .execute_batch(
            "CREATE TRIGGER refuse_conversations BEFORE INSERT ON conversations
             BEGIN SELECT RAISE(ABORT, 'refused by the test'); END;",
        )
        .expect("the trigger is installed");

    let error = store
        .create_conversation("/work/project")
        .expect_err("the refused insert is an error");
    assert!(
        error.to_string().contains("refused by the test"),
        "the trigger is the cause: {error}"
    );

    let saved: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM workspaces", [], |row| row.get(0))
        .expect("the count is read");
    assert_eq!(
        saved, 0,
        "the workspace opened by the same call is rolled back"
    );
}

#[test]
fn a_failed_brain_history_write_leaves_no_memory_behind() {
    // The memory and its first history entry are written in one transaction. The trigger refuses the history
    // entry, so the memory must be rolled back as well.
    let directory = scratch("store-brain-create-rollback");
    let store = Store::open(&directory).expect("open");
    store
        .workspaces()
        .touch("/work/project")
        .expect("the workspace the memory belongs to is saved");
    store
        .connection()
        .execute_batch(
            "CREATE TRIGGER refuse_brain_history BEFORE INSERT ON brain_memory_history
             BEGIN SELECT RAISE(ABORT, 'refused by the test'); END;",
        )
        .expect("the trigger is installed");
    let input = noct_core::brain_memory::CreateBrainMemoryInput {
        conversation_id: None,
        kind: noct_core::brain_memory::BrainMemoryKind::Fact,
        scope: noct_core::brain_memory::BrainMemoryScope::Workspace,
        content: "A note".to_string(),
        confidence: None,
        source_type: None,
        source_id: None,
        status: None,
    };

    // The workspace exists, so the memory insert succeeds and only the history insert is refused. The message
    // must name the trigger, or the test would pass for another reason.
    let error = store
        .brain_memories()
        .create("/work/project", &input)
        .expect_err("the refused history write is an error");
    assert!(
        error.to_string().contains("refused by the test"),
        "the trigger is the cause: {error}"
    );

    let stored: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM brain_memories", [], |row| row.get(0))
        .expect("the count is read");
    assert_eq!(
        stored, 0,
        "the memory written by the same call is rolled back"
    );
}

#[test]
fn a_failed_brain_history_write_on_update_leaves_the_memory_unchanged() {
    let directory = scratch("store-brain-update-rollback");
    let store = Store::open(&directory).expect("open");
    store
        .workspaces()
        .touch("/work/project")
        .expect("the workspace the memory belongs to is saved");
    let created = store
        .brain_memories()
        .create(
            "/work/project",
            &noct_core::brain_memory::CreateBrainMemoryInput {
                conversation_id: None,
                kind: noct_core::brain_memory::BrainMemoryKind::Fact,
                scope: noct_core::brain_memory::BrainMemoryScope::Workspace,
                content: "A note".to_string(),
                confidence: None,
                source_type: None,
                source_id: None,
                status: None,
            },
        )
        .expect("the memory is created");
    store
        .connection()
        .execute_batch(
            "CREATE TRIGGER refuse_brain_update_history BEFORE INSERT ON brain_memory_history
             BEGIN SELECT RAISE(ABORT, 'refused by the test'); END;",
        )
        .expect("the trigger is installed");
    let update = noct_core::brain_memory::UpdateBrainMemoryInput {
        content: Some("A changed note".to_string()),
        ..Default::default()
    };

    let error = store
        .brain_memories()
        .update(&created.id, "/work/project", &update)
        .expect_err("the refused history write is an error");
    assert!(
        error.to_string().contains("refused by the test"),
        "the trigger is the cause: {error}"
    );

    let stored = store
        .brain_memories()
        .get(&created.id, "/work/project")
        .expect("the memory is read")
        .expect("the memory is stored");
    assert_eq!(
        stored.content, "A note",
        "the change is rolled back with its history entry"
    );
}

#[test]
fn a_refused_candidate_leaves_none_of_the_batch_stored() {
    // The candidates of one reply are stored as one unit. The second one is refused by the trigger, so the first
    // must be rolled back as well.
    let directory = scratch("store-candidates-unit");
    let store = Store::open(&directory).expect("open");
    store
        .workspaces()
        .touch("/work/project")
        .expect("the workspace is saved");
    store
        .connection()
        .execute_batch(
            "CREATE TRIGGER refuse_second_candidate BEFORE INSERT ON brain_memories
             WHEN NEW.content = 'refuse me'
             BEGIN SELECT RAISE(ABORT, 'refused by the test'); END;",
        )
        .expect("the trigger is installed");
    let candidates = vec![
        noct_core::brain_memory::BrainMemoryCandidate {
            kind: noct_core::brain_memory::BrainMemoryKind::Fact,
            scope: noct_core::brain_memory::BrainMemoryScope::Workspace,
            content: "Store me first".to_string(),
            confidence: 60,
        },
        noct_core::brain_memory::BrainMemoryCandidate {
            kind: noct_core::brain_memory::BrainMemoryKind::Fact,
            scope: noct_core::brain_memory::BrainMemoryScope::Workspace,
            content: "refuse me".to_string(),
            confidence: 60,
        },
    ];

    let error = store
        .brain_memories()
        .create_candidates("/work/project", "unused", &candidates)
        .expect_err("the second candidate is refused");
    assert!(error.to_string().contains("refused by the test"), "{error}");

    let stored: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM brain_memories", [], |row| row.get(0))
        .expect("the count is read");
    assert_eq!(
        stored, 0,
        "the first candidate is rolled back with the batch"
    );
}

#[test]
fn a_failed_suggestion_decision_write_leaves_no_suggestion_behind() {
    // A review's suggestions and their first decision entries are written in one transaction. The trigger refuses the
    // decision entry, so the suggestion must be rolled back as well.
    let directory = scratch("store-suggestions-rollback");
    let store = Store::open(&directory).expect("open");
    let conversation = store
        .create_conversation("/work/project")
        .expect("the conversation is created");
    store
        .connection()
        .execute_batch(
            "CREATE TRIGGER refuse_suggestion_decisions BEFORE INSERT ON suggestion_decisions
             BEGIN SELECT RAISE(ABORT, 'refused by the test'); END;",
        )
        .expect("the trigger is installed");
    let input = noct_core::suggestions::SuggestionInput {
        title: "Missing tests".to_string(),
        description: "d".to_string(),
        reasoning: "r".to_string(),
        category: noct_core::suggestions::SuggestionCategory::Testing,
        severity: noct_core::suggestions::SuggestionSeverity::Medium,
        affected_files: Vec::new(),
        proposed_changes: String::new(),
        expected_benefits: Vec::new(),
        complexity: noct_core::suggestions::SuggestionLevel::Medium,
        risk: noct_core::suggestions::SuggestionLevel::Medium,
        evidence: None,
        confidence: None,
        source: None,
        responsible: None,
    };

    let error = store
        .suggestions()
        .reconcile(&conversation.id, "/work/project", &[input], false)
        .expect_err("the refused decision entry is an error");
    assert!(
        error.to_string().contains("refused by the test"),
        "the trigger is the cause: {error}"
    );

    let stored: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM suggestions", [], |row| row.get(0))
        .expect("the count is read");
    assert_eq!(
        stored, 0,
        "the suggestion written by the same call is rolled back"
    );
}
