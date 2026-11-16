//! Executions and checkpoints, checked against `ExecutionRepository.ts` and
//! `CheckpointRepository.ts`. The gate in `scripts/verify-row-gate.mjs` compares the same
//! reads against rows the TypeScript repositories wrote.

use noct_core::agent::AgentMode;
use noct_core::change_control::{
    CheckpointFileKind, CheckpointFileRecord, CheckpointPhase, CheckpointRecord, CheckpointStatus,
    ExecutionDecision, ExecutionRecord, ExecutionStatus,
};
use noct_core::workspace_evidence::EvidenceKind;
use noct_db::Database;
use noct_db::repositories::checkpoints::Checkpoints;
use noct_db::repositories::executions::Executions;
use noct_db::repositories::workspace_evidence::WorkspaceEvidence;

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";

fn open(name: &str) -> Database {
    let db = Database::open_runtime(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) VALUES('/work/alpha','alpha','{AT}','{AT}');\
             INSERT INTO conversations(id,title,workspace,created_at,updated_at) VALUES\
                 ('conv-1','Um','/work/alpha','{AT}','{AT}'),('conv-2','Dois','/work/alpha','{AT}','{AT}');"
        ))
        .expect("workspace and conversations");
    db
}

fn execution(id: &str, conversation: &str, started_at: &str) -> ExecutionRecord {
    ExecutionRecord {
        id: id.to_string(),
        workspace: "/work/alpha".to_string(),
        conversation_id: conversation.to_string(),
        prompt: "Corrigir o teste".to_string(),
        mode: AgentMode::Build,
        status: ExecutionStatus::Running,
        decision: ExecutionDecision::Pending,
        retry_of: None,
        started_at: started_at.to_string(),
        finished_at: None,
        error: None,
    }
}

fn checkpoint(id: &str, execution_id: &str, status: CheckpointStatus) -> CheckpointRecord {
    CheckpointRecord {
        id: id.to_string(),
        execution_id: execution_id.to_string(),
        workspace: "/work/alpha".to_string(),
        phase: CheckpointPhase::Before,
        status,
        captured_at: AT.to_string(),
        root_path: "/work/alpha".to_string(),
        error: None,
    }
}

fn file(id: &str, checkpoint_id: &str, path: &str, exists: bool) -> CheckpointFileRecord {
    CheckpointFileRecord {
        id: id.to_string(),
        checkpoint_id: checkpoint_id.to_string(),
        relative_path: path.to_string(),
        exists,
        kind: if exists {
            CheckpointFileKind::File
        } else {
            CheckpointFileKind::Missing
        },
        size: exists.then_some(12),
        mode: exists.then_some(420),
        hash: exists.then(|| "a".repeat(64)),
        content_path: None,
    }
}

#[test]
fn an_execution_is_read_back_with_zero_counts_until_work_is_linked() {
    let db = open("executions-create");
    let executions = Executions::new(db.connection());
    let record = execution("exec-1", "conv-1", AT);

    executions.create(&record).expect("create");

    let overview = executions
        .get("exec-1", Some("/work/alpha"))
        .expect("get")
        .expect("present");
    assert_eq!(overview.execution, record);
    assert_eq!(overview.change_count, 0);
    assert_eq!(overview.validation_count, 0);
    assert_eq!(overview.checkpoint_count, 0);
    assert!(
        executions
            .get("exec-1", Some("/work/other"))
            .expect("scoped")
            .is_none()
    );
}

#[test]
fn prompt_and_error_are_cut_to_the_stored_limits_in_utf16_units() {
    let db = open("executions-limits");
    let executions = Executions::new(db.connection());
    let mut record = execution("exec-1", "conv-1", AT);
    // Each emoji is two UTF-16 units, so 1,000,001 of them exceed the 2,000,000 limit.
    record.prompt = "😀".repeat(1_000_001);
    record.error = Some("e".repeat(500_001));

    executions.create(&record).expect("create");

    let stored = executions
        .get("exec-1", None)
        .expect("get")
        .expect("present")
        .execution;
    assert_eq!(stored.prompt.encode_utf16().count(), 2_000_000);
    assert_eq!(stored.error.map(|error| error.len()), Some(500_000));
}

#[test]
fn a_terminal_outcome_is_immutable_but_may_be_restated() {
    let db = open("executions-terminal");
    let executions = Executions::new(db.connection());
    let mut record = execution("exec-1", "conv-1", AT);
    record.status = ExecutionStatus::Completed;
    executions.create(&record).expect("create");

    let mut reopened = record.clone();
    reopened.status = ExecutionStatus::Running;
    let error = executions.update(&reopened).expect_err("terminal change");
    assert_eq!(
        error.to_string(),
        "O resultado terminal de uma execução é imutável."
    );

    let mut restated = record.clone();
    restated.error = Some("Ajuste sem mudar o estado.".to_string());
    executions.update(&restated).expect("same terminal status");
    assert_eq!(
        executions
            .get("exec-1", None)
            .expect("get")
            .expect("present")
            .execution
            .error
            .as_deref(),
        Some("Ajuste sem mudar o estado.")
    );
}

#[test]
fn save_creates_once_and_then_updates_the_same_execution() {
    let db = open("executions-save");
    let executions = Executions::new(db.connection());
    let mut record = execution("exec-1", "conv-1", AT);

    executions.save(&record).expect("create through save");
    record.status = ExecutionStatus::Failed;
    record.finished_at = Some(AT.to_string());
    record.error = Some("Falhou.".to_string());
    executions.save(&record).expect("update through save");

    let listed = executions.list("/work/alpha", None, 50).expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].execution, record);
}

#[test]
fn listing_is_filtered_by_conversation_and_bounded() {
    let db = open("executions-list");
    let executions = Executions::new(db.connection());
    for index in 0..3 {
        executions
            .create(&execution(
                &format!("exec-a{index}"),
                "conv-1",
                &format!("2026-01-01T00:00:0{index}.000Z"),
            ))
            .expect("create");
    }
    executions
        .create(&execution("exec-b", "conv-2", AT))
        .expect("other conversation");

    let first_conversation = executions
        .list("/work/alpha", Some("conv-1"), 2)
        .expect("filtered");
    assert_eq!(first_conversation.len(), 2);
    assert!(
        first_conversation
            .iter()
            .all(|row| row.execution.conversation_id == "conv-1")
    );
    assert_eq!(
        first_conversation[0].execution.id, "exec-a2",
        "newest first"
    );

    assert_eq!(
        executions
            .list("/work/alpha", None, 0)
            .expect("floor")
            .len(),
        1
    );
    assert_eq!(
        executions
            .list("/work/alpha", None, 1_000)
            .expect("ceiling")
            .len(),
        4
    );
}

#[test]
fn an_update_under_another_identity_changes_nothing() {
    let db = open("executions-identity");
    let executions = Executions::new(db.connection());
    executions
        .create(&execution("exec-1", "conv-1", AT))
        .expect("create");
    let mut moved = execution("exec-1", "conv-2", AT);
    moved.error = Some("Não deve gravar.".to_string());

    executions.update(&moved).expect("no row matches, no error");

    let stored = executions
        .get("exec-1", None)
        .expect("get")
        .expect("present")
        .execution;
    assert_eq!(stored.conversation_id, "conv-1");
    assert_eq!(stored.error, None);
}

#[test]
fn a_completed_capture_stores_its_files_and_evidence_as_one_unit() {
    let db = open("checkpoints-capture");
    let conn = db.connection();
    Executions::new(conn)
        .create(&execution("exec-1", "conv-1", AT))
        .expect("execution");
    let checkpoints = Checkpoints::new(conn);
    let mut record = checkpoint("ck-1", "exec-1", CheckpointStatus::Capturing);
    checkpoints.save(&record).expect("save capturing");
    record.status = CheckpointStatus::Ready;
    let files = vec![
        file("cf-1", "ck-1", "src/a.ts", true),
        file("cf-2", "ck-1", "src/gone.ts", false),
    ];

    let manifest = checkpoints
        .complete_capture(&record, &files)
        .expect("complete");

    assert_eq!(manifest.kind, EvidenceKind::Before);
    assert_eq!(manifest.source_id, "ck-1");
    assert_eq!(manifest.execution_id.as_deref(), Some("exec-1"));
    assert_eq!(
        checkpoints.get("ck-1", Some("exec-1")).expect("get"),
        Some(record.clone())
    );
    assert_eq!(checkpoints.list_files("ck-1").expect("files"), files);
    let evidence = WorkspaceEvidence::new(conn)
        .by_source(EvidenceKind::Before, "ck-1")
        .expect("evidence")
        .expect("present");
    assert_eq!(evidence.paths.len(), 2);
    assert_eq!(evidence.paths[1].exists, Some(false));
    let overview = Executions::new(conn)
        .get("exec-1", None)
        .expect("get")
        .expect("present");
    assert_eq!(overview.checkpoint_count, 1);
}

#[test]
fn a_checkpoint_is_only_found_within_its_execution_when_asked() {
    let db = open("checkpoints-scope");
    let conn = db.connection();
    let executions = Executions::new(conn);
    executions
        .create(&execution("exec-1", "conv-1", AT))
        .expect("first");
    executions
        .create(&execution("exec-2", "conv-1", AT))
        .expect("second");
    let checkpoints = Checkpoints::new(conn);
    checkpoints
        .create(&checkpoint("ck-1", "exec-1", CheckpointStatus::Ready))
        .expect("create");

    assert!(
        checkpoints
            .get("ck-1", Some("exec-2"))
            .expect("scoped")
            .is_none()
    );
    assert!(checkpoints.get("ck-1", None).expect("unscoped").is_some());
    assert_eq!(checkpoints.list("exec-1").expect("list").len(), 1);
    assert!(checkpoints.list("exec-2").expect("list").is_empty());
}

#[test]
fn replacing_files_drops_the_previous_manifest() {
    let db = open("checkpoints-replace");
    let conn = db.connection();
    Executions::new(conn)
        .create(&execution("exec-1", "conv-1", AT))
        .expect("execution");
    let checkpoints = Checkpoints::new(conn);
    checkpoints
        .create(&checkpoint("ck-1", "exec-1", CheckpointStatus::Ready))
        .expect("create");
    checkpoints
        .replace_files("ck-1", &[file("cf-1", "ck-1", "old.ts", true)])
        .expect("first manifest");

    checkpoints
        .replace_files("ck-1", &[file("cf-2", "ck-1", "new.ts", true)])
        .expect("second manifest");

    let files = checkpoints.list_files("ck-1").expect("files");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, "new.ts");
}
