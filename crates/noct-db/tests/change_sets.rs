//! Change sets, file changes, hunks and review decisions, checked against
//! `ChangeSetRepository.ts`. The gate in `scripts/verify-row-gate.mjs` compares the same
//! reads against rows the TypeScript repository wrote.

use noct_core::agent::AgentMode;
use noct_core::change_control::{
    ChangeHunkRecord, ChangeOperation, ChangeOrigin, ChangePolicy, ChangeRecord, ChangeSetRecord,
    ChangeSetStatus, ChangeStatus, CheckpointPhase, CheckpointRecord, CheckpointStatus,
    ExecutionDecision, ExecutionRecord, ExecutionStatus, HunkStatus, ReviewDecision,
    ValidationStatus,
};
use noct_core::workspace_evidence::EvidenceKind;
use noct_db::Database;
use noct_db::repositories::change_sets::ChangeSets;
use noct_db::repositories::checkpoints::Checkpoints;
use noct_db::repositories::executions::Executions;
use noct_db::repositories::workspace_evidence::WorkspaceEvidence;

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";
const LATER: &str = "2026-01-01T00:00:05.000Z";

fn open(name: &str) -> Database {
    let db = Database::open_runtime(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) VALUES('/work/alpha','alpha','{AT}','{AT}');\
             INSERT INTO conversations(id,title,workspace,created_at,updated_at) VALUES('conv-1','Um','/work/alpha','{AT}','{AT}');"
        ))
        .expect("workspace and conversation");
    let conn = db.connection();
    Executions::new(conn)
        .create(&ExecutionRecord {
            id: "exec-1".to_string(),
            workspace: "/work/alpha".to_string(),
            conversation_id: "conv-1".to_string(),
            prompt: "Alterar".to_string(),
            mode: AgentMode::Build,
            status: ExecutionStatus::Completed,
            decision: ExecutionDecision::Pending,
            retry_of: None,
            started_at: AT.to_string(),
            finished_at: Some(AT.to_string()),
            error: None,
        })
        .expect("execution");
    let checkpoints = Checkpoints::new(conn);
    for (id, phase) in [
        ("ck-before", CheckpointPhase::Before),
        ("ck-after", CheckpointPhase::After),
    ] {
        checkpoints
            .create(&CheckpointRecord {
                id: id.to_string(),
                execution_id: "exec-1".to_string(),
                workspace: "/work/alpha".to_string(),
                phase,
                status: CheckpointStatus::Ready,
                captured_at: AT.to_string(),
                root_path: "/work/alpha".to_string(),
                error: None,
            })
            .expect("checkpoint");
    }
    db
}

fn change_set(status: ChangeSetStatus, updated_at: &str) -> ChangeSetRecord {
    ChangeSetRecord {
        id: "cs-1".to_string(),
        execution_id: "exec-1".to_string(),
        before_checkpoint_id: "ck-before".to_string(),
        after_checkpoint_id: "ck-after".to_string(),
        status,
        created_at: AT.to_string(),
        updated_at: updated_at.to_string(),
    }
}

fn change(id: &str, path: &str, operation: ChangeOperation, status: ChangeStatus) -> ChangeRecord {
    ChangeRecord {
        id: id.to_string(),
        execution_id: "exec-1".to_string(),
        change_set_id: "cs-1".to_string(),
        checkpoint_id: Some("ck-after".to_string()),
        relative_path: path.to_string(),
        original_path: None,
        operation,
        origin: ChangeOrigin::AgentFileChange,
        before_hash: Some("a".repeat(64)),
        after_hash: Some("b".repeat(64)),
        before_size: Some(12),
        after_size: Some(14),
        status,
        validation_status: ValidationStatus::Unknown,
        policy: ChangePolicy::Allowed,
        policy_reason: None,
        created_at: AT.to_string(),
        updated_at: AT.to_string(),
    }
}

#[test]
fn a_saved_change_set_reads_back_with_its_changes_in_path_order() {
    let db = open("change-sets-save");
    let sets = ChangeSets::new(db.connection());
    let changes = vec![
        change(
            "chg-b",
            "src/b.ts",
            ChangeOperation::Modify,
            ChangeStatus::Pending,
        ),
        change(
            "chg-a",
            "src/a.ts",
            ChangeOperation::Create,
            ChangeStatus::Pending,
        ),
    ];

    sets.save(&change_set(ChangeSetStatus::Pending, AT), &changes)
        .expect("save");

    assert_eq!(
        sets.get_by_id("cs-1", Some("exec-1")).expect("by id"),
        Some(change_set(ChangeSetStatus::Pending, AT))
    );
    let listed = sets.list_changes("cs-1").expect("changes");
    assert_eq!(
        listed
            .iter()
            .map(|row| row.relative_path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/a.ts", "src/b.ts"]
    );
    assert_eq!(listed[0].operation, ChangeOperation::Create);
    assert_eq!(
        sets.get_change("chg-a", None).expect("change"),
        Some(listed[0].clone())
    );
}

#[test]
fn replacing_changes_drops_the_earlier_file_set() {
    let db = open("change-sets-replace");
    let sets = ChangeSets::new(db.connection());
    sets.save(
        &change_set(ChangeSetStatus::Pending, AT),
        &[change(
            "chg-old",
            "old.ts",
            ChangeOperation::Modify,
            ChangeStatus::Pending,
        )],
    )
    .expect("first");

    sets.replace_changes(
        "cs-1",
        &[change(
            "chg-new",
            "new.ts",
            ChangeOperation::Delete,
            ChangeStatus::Pending,
        )],
    )
    .expect("replace");

    let listed = sets.list_changes("cs-1").expect("changes");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "chg-new");
}

#[test]
fn the_latest_capture_of_an_execution_is_the_one_returned() {
    let db = open("change-sets-latest");
    let sets = ChangeSets::new(db.connection());
    sets.create(&change_set(ChangeSetStatus::Pending, AT))
        .expect("older");
    let mut newer = change_set(ChangeSetStatus::Pending, LATER);
    newer.id = "cs-2".to_string();
    sets.create(&newer).expect("newer");

    assert_eq!(
        sets.get_by_execution_id("exec-1")
            .expect("latest")
            .map(|row| row.id),
        Some("cs-2".to_string())
    );
    assert_eq!(sets.list("exec-1").expect("list").len(), 2);
    assert!(
        sets.get_by_id("cs-1", Some("exec-other"))
            .expect("scoped")
            .is_none()
    );
}

#[test]
fn an_accepted_decision_updates_the_change_set_the_execution_and_its_evidence() {
    let db = open("change-sets-decision");
    let conn = db.connection();
    let sets = ChangeSets::new(conn);
    sets.save(
        &change_set(ChangeSetStatus::Pending, AT),
        &[change(
            "chg-a",
            "src/a.ts",
            ChangeOperation::Modify,
            ChangeStatus::Pending,
        )],
    )
    .expect("save");
    let operation = sets
        .reserve_decision("exec-1", "chg-a", ReviewDecision::Accepted)
        .expect("reserve");
    let mut decided = change(
        "chg-a",
        "src/a.ts",
        ChangeOperation::Modify,
        ChangeStatus::Accepted,
    );
    decided.validation_status = ValidationStatus::Pending;
    decided.updated_at = LATER.to_string();

    sets.save_decision(
        &change_set(ChangeSetStatus::PartiallyAccepted, LATER),
        &decided,
        Some(&operation),
    )
    .expect("decision");

    assert_eq!(
        sets.get_by_id("cs-1", None)
            .expect("set")
            .expect("present")
            .status,
        ChangeSetStatus::PartiallyAccepted
    );
    assert_eq!(
        sets.get_change("chg-a", None)
            .expect("change")
            .expect("present")
            .status,
        ChangeStatus::Accepted
    );
    let overview = Executions::new(conn)
        .get("exec-1", None)
        .expect("get")
        .expect("present");
    assert_eq!(
        overview.execution.decision,
        ExecutionDecision::PartiallyAccepted
    );
    assert_eq!(overview.change_count, 1);

    let evidence = WorkspaceEvidence::new(conn)
        .by_source(EvidenceKind::Decision, &operation)
        .expect("evidence")
        .expect("recorded");
    assert_eq!(
        evidence.paths[0].hash.as_deref(),
        Some("b".repeat(64).as_str())
    );
    let references: Vec<&str> = evidence
        .references
        .iter()
        .map(|item| item.kind.as_str())
        .collect();
    assert_eq!(references, vec!["before", "after", "change"]);

    let status: String = conn
        .query_row(
            "SELECT status FROM change_decision_operations WHERE id=?1",
            [&operation],
            |row| row.get(0),
        )
        .expect("operation");
    assert_eq!(status, "completed");
}

#[test]
fn a_rejected_decision_keeps_the_before_hash_as_evidence() {
    let db = open("change-sets-reject");
    let conn = db.connection();
    let sets = ChangeSets::new(conn);
    sets.save(
        &change_set(ChangeSetStatus::Pending, AT),
        &[change(
            "chg-a",
            "src/a.ts",
            ChangeOperation::Modify,
            ChangeStatus::Pending,
        )],
    )
    .expect("save");
    let operation = sets
        .reserve_decision("exec-1", "chg-a", ReviewDecision::Rejected)
        .expect("reserve");
    let mut rejected = change(
        "chg-a",
        "src/a.ts",
        ChangeOperation::Modify,
        ChangeStatus::Rejected,
    );
    rejected.updated_at = LATER.to_string();

    sets.save_decision(
        &change_set(ChangeSetStatus::Rejected, LATER),
        &rejected,
        Some(&operation),
    )
    .expect("decision");

    let evidence = WorkspaceEvidence::new(conn)
        .by_source(EvidenceKind::Decision, &operation)
        .expect("evidence")
        .expect("recorded");
    assert_eq!(
        evidence.paths[0].hash.as_deref(),
        Some("a".repeat(64).as_str())
    );
}

#[test]
fn a_failed_decision_is_recorded_as_conflicted_with_its_error() {
    let db = open("change-sets-fail");
    let conn = db.connection();
    let sets = ChangeSets::new(conn);
    sets.save(
        &change_set(ChangeSetStatus::Pending, AT),
        &[change(
            "chg-a",
            "src/a.ts",
            ChangeOperation::Modify,
            ChangeStatus::Pending,
        )],
    )
    .expect("save");
    let operation = sets
        .reserve_decision("exec-1", "chg-a", ReviewDecision::Accepted)
        .expect("reserve");

    sets.fail_decision(&operation, "O arquivo mudou.")
        .expect("fail");

    let (status, error): (String, String) = conn
        .query_row(
            "SELECT status, error FROM change_decision_operations WHERE id=?1",
            [&operation],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("operation");
    assert_eq!(
        (status.as_str(), error.as_str()),
        ("conflicted", "O arquivo mudou.")
    );
}

#[test]
fn hunks_are_listed_in_sequence_and_can_be_annotated() {
    let db = open("change-sets-hunks");
    let sets = ChangeSets::new(db.connection());
    sets.save(
        &change_set(ChangeSetStatus::Pending, AT),
        &[change(
            "chg-a",
            "src/a.ts",
            ChangeOperation::Modify,
            ChangeStatus::Pending,
        )],
    )
    .expect("save");
    let hunk = |id: &str, sequence: i64| ChangeHunkRecord {
        id: id.to_string(),
        change_id: "chg-a".to_string(),
        sequence,
        base_hash: "a".repeat(64),
        original_patch: "@@ -1 +1 @@".to_string(),
        final_patch: "@@ -1 +1 @@".to_string(),
        status: HunkStatus::Pending,
        start_line: 1,
        end_line: 1,
        decision_at: None,
    };
    sets.create_hunk(&hunk("hk-2", 2)).expect("second");
    sets.create_hunk(&hunk("hk-1", 1)).expect("first");

    let mut edited = hunk("hk-1", 1);
    edited.final_patch = "@@ -1 +1,2 @@".to_string();
    edited.status = HunkStatus::Edited;
    edited.decision_at = Some(LATER.to_string());
    sets.update_hunk(&edited).expect("update");

    let listed = sets.list_hunks("chg-a").expect("hunks");
    assert_eq!(
        listed.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec!["hk-1", "hk-2"]
    );
    assert_eq!(sets.get_hunk("hk-1").expect("hunk"), Some(edited));
}
