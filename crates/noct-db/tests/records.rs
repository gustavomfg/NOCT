//! Behaviour of the artifact, approval and workspace-memory repositories, checked
//! against `ArtifactRepository.ts`, `ApprovalRepository.ts` and
//! `WorkspaceMemoryRepository.ts`.

use std::thread;
use std::time::Duration;

use noct_db::repositories::approvals::Approvals;
use noct_db::repositories::artifacts::{Artifacts, NewArtifact};
use noct_db::repositories::conversations::{ConversationRow, Conversations};
use noct_db::repositories::workspace_memory::WorkspaceMemory;
use noct_db::{Database, Error};
use serde_json::{Value, json};

mod common;
use common::scratch;

fn tick() {
    thread::sleep(Duration::from_millis(3));
}

fn conversation(db: &Database, workspace: &str) -> ConversationRow {
    Conversations::new(db.connection())
        .create(workspace)
        .expect("conversation")
}

fn artifact<'a>(conversation_id: &'a str, title: &'a str) -> NewArtifact<'a> {
    NewArtifact {
        conversation_id,
        workspace: "/work/project",
        kind: "document",
        title,
        file_path: None,
        content: Some("conteúdo"),
        metadata: None,
    }
}

#[test]
fn artifacts_list_newest_first_and_page_with_a_lookahead() {
    let db = Database::open_runtime(&scratch("artifacts-page")).unwrap();
    let chat = conversation(&db, "/work/project");
    let artifacts = Artifacts::new(db.connection());
    for title in ["primeiro", "segundo", "terceiro"] {
        artifacts.add(&artifact(&chat.id, title)).unwrap();
        tick();
    }

    let titles: Vec<String> = artifacts
        .list(&chat.id)
        .unwrap()
        .into_iter()
        .map(|row| row.title)
        .collect();
    assert_eq!(titles, ["terceiro", "segundo", "primeiro"]);

    let first = artifacts.page(&chat.id, 0, 2).unwrap();
    assert_eq!(first.items.len(), 2);
    assert!(first.has_more);
    let last = artifacts.page(&chat.id, 2, 2).unwrap();
    assert_eq!(last.items.len(), 1);
    assert!(!last.has_more);
}

#[test]
fn a_known_file_path_updates_the_artifact_and_keeps_its_identity() {
    let db = Database::open_runtime(&scratch("artifacts-upsert")).unwrap();
    let chat = conversation(&db, "/work/project");
    let artifacts = Artifacts::new(db.connection());
    let mut input = artifact(&chat.id, "relatório");
    input.file_path = Some("docs/relatorio.md");
    let first = artifacts.add(&input).unwrap();
    tick();
    input.content = Some("versão nova");
    let second = artifacts.add(&input).unwrap();

    assert_eq!(second.id, first.id);
    assert_eq!(second.created_at, first.created_at);
    assert_ne!(second.updated_at, first.updated_at);
    let stored = artifacts.list(&chat.id).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].content.as_deref(), Some("versão nova"));
}

#[test]
fn an_empty_file_path_never_matches_an_existing_artifact() {
    let db = Database::open_runtime(&scratch("artifacts-empty-path")).unwrap();
    let chat = conversation(&db, "/work/project");
    let artifacts = Artifacts::new(db.connection());
    let mut input = artifact(&chat.id, "sem caminho");
    input.file_path = Some("");
    let first = artifacts.add(&input).unwrap();
    let second = artifacts.add(&input).unwrap();

    assert_ne!(first.id, second.id);
    assert_eq!(artifacts.list(&chat.id).unwrap().len(), 2);
}

#[test]
fn artifact_metadata_is_stored_as_json_text_in_insertion_order() {
    let db = Database::open_runtime(&scratch("artifacts-metadata")).unwrap();
    let chat = conversation(&db, "/work/project");
    let artifacts = Artifacts::new(db.connection());
    let metadata = json!({ "zeta": 1, "alpha": [true, null, "x"] });
    let mut input = artifact(&chat.id, "com metadados");
    input.metadata = Some(&metadata);
    let row = artifacts.add(&input).unwrap();

    assert_eq!(
        row.metadata.as_deref(),
        Some(r#"{"zeta":1,"alpha":[true,null,"x"]}"#),
    );
}

#[test]
fn artifact_metadata_is_bounded_by_size_and_depth() {
    let db = Database::open_runtime(&scratch("artifacts-metadata-limits")).unwrap();
    let chat = conversation(&db, "/work/project");
    let artifacts = Artifacts::new(db.connection());

    let oversized = Value::String("a".repeat(500_000));
    let mut input = artifact(&chat.id, "grande");
    input.metadata = Some(&oversized);
    assert!(matches!(artifacts.add(&input), Err(Error::Message(_))));

    let mut deep = Value::Null;
    for _ in 0..40 {
        deep = Value::Array(vec![deep]);
    }
    input.metadata = Some(&deep);
    assert!(matches!(artifacts.add(&input), Err(Error::Message(_))));
    assert!(artifacts.list(&chat.id).unwrap().is_empty());
}

#[test]
fn deleting_an_artifact_requires_its_own_conversation() {
    let db = Database::open_runtime(&scratch("artifacts-delete")).unwrap();
    let chat = conversation(&db, "/work/project");
    let other = conversation(&db, "/work/other");
    let artifacts = Artifacts::new(db.connection());
    let row = artifacts.add(&artifact(&chat.id, "removível")).unwrap();

    assert!(!artifacts.delete(&row.id, &other.id).unwrap());
    assert!(artifacts.delete(&row.id, &chat.id).unwrap());
    assert!(!artifacts.delete(&row.id, &chat.id).unwrap());
}

#[test]
fn approvals_record_the_decision_and_truncate_long_commands() {
    let db = Database::open_runtime(&scratch("approvals")).unwrap();
    let approvals = Approvals::new(db.connection());
    let long_command = "a".repeat(5_000);
    approvals
        .record("exec-1", true, Some(&long_command), Some("medium"))
        .unwrap();
    approvals.record("exec-2", false, None, None).unwrap();

    let rows: Vec<(String, String, Option<String>, Option<String>)> = db
        .connection()
        .prepare(
            "SELECT approval_key, decision, command, risk FROM approval_audit ORDER BY approval_key",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

    assert_eq!(rows[0].0, "exec-1");
    assert_eq!(rows[0].1, "accepted");
    assert_eq!(rows[0].2.as_deref().map(str::len), Some(4_000));
    assert_eq!(rows[0].3.as_deref(), Some("medium"));
    assert_eq!(rows[1].1, "declined");
    assert_eq!(rows[1].2, None);
    assert_eq!(rows[1].3, None);
}

#[test]
fn workspace_memory_defaults_to_empty_and_round_trips() {
    let db = Database::open_runtime(&scratch("workspace-memory")).unwrap();
    let memory = WorkspaceMemory::new(db.connection());

    let empty = memory.get("/work/project").unwrap();
    assert_eq!(empty.content, "");
    assert_eq!(empty.updated_at, "");

    let saved = memory.set("/work/project", "Usar Vitest.").unwrap();
    assert_eq!(saved.content, "Usar Vitest.");
    let read = memory.get("/work/project").unwrap();
    assert_eq!(read, saved);

    memory.set("/work/project", "Usar cargo test.").unwrap();
    assert_eq!(
        memory.get("/work/project").unwrap().content,
        "Usar cargo test."
    );
}
