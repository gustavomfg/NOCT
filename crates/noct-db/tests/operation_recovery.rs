//! Startup reconciliation of interrupted operations, checked against
//! `OperationRecoveryRepository.ts`. Fixtures are plain SQL: the execution, checkpoint
//! and change repositories are not ported yet, so their rows are written here directly.

use noct_db::Database;
use noct_db::repositories::operation_recovery::{OperationRecovery, PendingDecision};
use rusqlite::{Connection, params};

mod common;
use common::scratch;

const INTERRUPTED: &str = "Operação interrompida; o estado precisa ser reconciliado. Nenhum trabalho foi retomado automaticamente.";
const AWAITING: &str = "Decisão pendente recuperada; revise os arquivos atuais antes de decidir. Nenhuma decisão foi aplicada automaticamente.";
const AT: &str = "2026-01-01T00:00:00.000Z";

fn open(name: &str) -> Database {
    let db = Database::open(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) \
                 VALUES('/work/alpha','alpha','{AT}','{AT}');\
             INSERT INTO conversations(id,title,workspace,created_at,updated_at) \
                 VALUES('conv-1','Conversa','/work/alpha','{AT}','{AT}');"
        ))
        .expect("workspace and conversation");
    db
}

fn execution(conn: &Connection, id: &str, status: &str, decision: &str) {
    conn.execute(
        "INSERT INTO executions(id,workspace,conversation_id,prompt,mode,status,decision,started_at) \
         VALUES(?1,'/work/alpha','conv-1','Tarefa','build',?2,?3,?4)",
        params![id, status, decision, AT],
    )
    .expect("execution");
}

fn checkpoint(conn: &Connection, id: &str, execution_id: &str) {
    conn.execute(
        "INSERT INTO checkpoints(id,execution_id,workspace,phase,status,captured_at,root_path) \
         VALUES(?1,?2,'/work/alpha','before','ready',?3,'/work/alpha')",
        params![id, execution_id, AT],
    )
    .expect("checkpoint");
}

fn change_set(conn: &Connection, id: &str, execution_id: &str, status: &str) {
    checkpoint(conn, &format!("{id}-before"), execution_id);
    checkpoint(conn, &format!("{id}-after"), execution_id);
    conn.execute(
        "INSERT INTO change_sets(id,execution_id,before_checkpoint_id,after_checkpoint_id,status,created_at,updated_at) \
         VALUES(?1,?2,?3,?4,?5,?6,?6)",
        params![
            id,
            execution_id,
            format!("{id}-before"),
            format!("{id}-after"),
            status,
            AT
        ],
    )
    .expect("change set");
}

fn change(conn: &Connection, id: &str, change_set_id: &str, execution_id: &str) {
    conn.execute(
        "INSERT INTO changes(id,change_set_id,execution_id,relative_path,operation,origin,status,\
         validation_status,created_at,updated_at) \
         VALUES(?1,?2,?3,'src/a.ts','modify','agent-file-change','pending','unknown',?4,?4)",
        params![id, change_set_id, execution_id, AT],
    )
    .expect("change");
}

fn scalar(conn: &Connection, sql: &str, id: &str) -> String {
    conn.query_row(sql, [id], |row| row.get::<_, Option<String>>(0))
        .expect("row")
        .unwrap_or_default()
}

#[test]
fn a_running_execution_is_failed_and_recorded_once() {
    let db = open("recovery-execution");
    let conn = db.connection();
    execution(conn, "exec-1", "running", "pending");
    execution(conn, "exec-2", "completed", "accepted");
    let recovery = OperationRecovery::new(conn);

    recovery.reconcile().expect("reconcile");
    recovery
        .reconcile()
        .expect("second reconcile changes nothing");

    assert_eq!(
        scalar(conn, "SELECT status FROM executions WHERE id=?1", "exec-1"),
        "failed"
    );
    assert_eq!(
        scalar(conn, "SELECT error FROM executions WHERE id=?1", "exec-1"),
        INTERRUPTED
    );
    assert_ne!(
        scalar(
            conn,
            "SELECT finished_at FROM executions WHERE id=?1",
            "exec-1"
        ),
        ""
    );
    assert_eq!(
        scalar(conn, "SELECT status FROM executions WHERE id=?1", "exec-2"),
        "completed",
        "a finished execution is left alone"
    );

    let listed = recovery.list().expect("list");
    assert_eq!(listed.len(), 1);
    let operation = &listed[0];
    assert_eq!(operation.id, "execution:exec-1");
    assert_eq!(operation.kind, "execution");
    assert_eq!(operation.source_id, "exec-1");
    assert_eq!(operation.execution_id.as_deref(), Some("exec-1"));
    assert_eq!(operation.original_status, "running");
    assert_eq!(operation.message, INTERRUPTED);
}

#[test]
fn an_interrupted_decision_conflicts_its_change_set_and_execution() {
    let db = open("recovery-decision");
    let conn = db.connection();
    execution(conn, "exec-1", "completed", "pending");
    change_set(conn, "set-1", "exec-1", "pending");
    change(conn, "chg-1", "set-1", "exec-1");
    conn.execute(
        "INSERT INTO change_decision_operations(id,execution_id,change_id,decision,status,started_at) \
         VALUES('op-1','exec-1','chg-1','accepted','running',?1)",
        [AT],
    )
    .expect("decision operation");
    let recovery = OperationRecovery::new(conn);

    recovery.reconcile().expect("reconcile");

    assert_eq!(
        scalar(conn, "SELECT status FROM changes WHERE id=?1", "chg-1"),
        "conflicted"
    );
    assert_eq!(
        scalar(conn, "SELECT status FROM change_sets WHERE id=?1", "set-1"),
        "conflicted"
    );
    assert_eq!(
        scalar(
            conn,
            "SELECT decision FROM executions WHERE id=?1",
            "exec-1"
        ),
        "conflicted"
    );
    assert_eq!(
        scalar(
            conn,
            "SELECT status FROM change_decision_operations WHERE id=?1",
            "op-1"
        ),
        "interrupted"
    );
    assert_eq!(
        scalar(
            conn,
            "SELECT error FROM change_decision_operations WHERE id=?1",
            "op-1"
        ),
        INTERRUPTED
    );

    let listed = recovery.list().expect("list");
    let decision = listed
        .iter()
        .find(|row| row.id == "decision:op-1")
        .expect("decision recorded");
    assert_eq!(decision.kind, "decision");
    assert_eq!(decision.original_status, "running");
    assert_eq!(decision.execution_id.as_deref(), Some("exec-1"));
    assert!(
        listed.iter().any(|row| row.id == "awaiting-decision:set-1"),
        "a conflicted change set waits for a new decision"
    );
}

#[test]
fn a_running_command_is_failed_with_the_recovery_message() {
    let db = open("recovery-command");
    let conn = db.connection();
    execution(conn, "exec-1", "completed", "pending");
    conn.execute(
        "INSERT INTO execution_commands(id,execution_id,command,args_json,source,status,output_summary,started_at) \
         VALUES('cmd-1','exec-1','npm test','[]','validation','running','',?1)",
        [AT],
    )
    .expect("command");
    let recovery = OperationRecovery::new(conn);

    recovery.reconcile().expect("reconcile");

    assert_eq!(
        scalar(
            conn,
            "SELECT status FROM execution_commands WHERE id=?1",
            "cmd-1"
        ),
        "failed"
    );
    assert_eq!(
        scalar(
            conn,
            "SELECT output_summary FROM execution_commands WHERE id=?1",
            "cmd-1"
        ),
        INTERRUPTED
    );
    let listed = recovery.list().expect("list");
    assert!(
        listed
            .iter()
            .any(|row| row.id == "command:cmd-1" && row.kind == "command"),
        "{listed:?}"
    );
}

#[test]
fn pending_decisions_wait_for_the_user_and_are_not_applied() {
    let db = open("recovery-pending");
    let conn = db.connection();
    execution(conn, "exec-1", "completed", "pending");
    change_set(conn, "set-1", "exec-1", "pending");
    let recovery = OperationRecovery::new(conn);

    recovery.reconcile().expect("reconcile");

    assert_eq!(
        scalar(conn, "SELECT status FROM change_sets WHERE id=?1", "set-1"),
        "pending",
        "reconciliation never decides a change set"
    );
    assert_eq!(
        recovery.pending_decisions().expect("pending"),
        vec![PendingDecision {
            execution_id: "exec-1".to_string(),
            workspace: "/work/alpha".to_string(),
        }],
    );
    let awaiting = recovery
        .list()
        .expect("list")
        .into_iter()
        .find(|row| row.id == "awaiting-decision:set-1")
        .expect("awaiting decision recorded");
    assert_eq!(awaiting.message, AWAITING);
    assert_eq!(awaiting.original_status, "pending");
}

#[test]
fn running_index_runs_and_units_are_closed_without_an_execution() {
    let db = open("recovery-indexes");
    let conn = db.connection();
    conn.execute(
        "INSERT INTO project_index_runs(id,workspace,index_version,kind,status,phase,started_at,updated_at) \
         VALUES('pix-1','/work/alpha',1,'initial','running','hashing',?1,?1)",
        [AT],
    )
    .expect("project index run");
    conn.execute(
        "INSERT INTO semantic_index_runs(id,workspace,index_version,kind,status,started_at,updated_at) \
         VALUES('six-1','/work/alpha',1,'initial','running',?1,?1)",
        [AT],
    )
    .expect("semantic index run");
    conn.execute(
        "INSERT INTO project_index_files(workspace,relative_path,classification,language,extension,size,\
         mtime_ms,ctime_ms,mode,state,excluded,discovered_at) \
         VALUES('/work/alpha','src/a.ts','source','typescript','.ts',10,1,1,420,'indexed',0,?1)",
        [AT],
    )
    .expect("project file");
    conn.execute(
        "INSERT INTO semantic_units(id,workspace,relative_path,kind,start_line,start_column,end_line,end_column,\
         source_hash,chunk_hash,normalized_text,chunk_strategy_version,status,created_at,updated_at) \
         VALUES('unit-1','/work/alpha','src/a.ts','text',1,1,1,1,?1,?1,'texto',  'v1','processing',?2,?2)",
        params!["a".repeat(64), AT],
    )
    .expect("semantic unit");
    let recovery = OperationRecovery::new(conn);

    recovery.reconcile().expect("reconcile");

    assert_eq!(
        scalar(
            conn,
            "SELECT status FROM project_index_runs WHERE id=?1",
            "pix-1"
        ),
        "failed"
    );
    assert_eq!(
        scalar(
            conn,
            "SELECT status FROM semantic_index_runs WHERE id=?1",
            "six-1"
        ),
        "failed"
    );
    assert_eq!(
        scalar(
            conn,
            "SELECT status FROM semantic_units WHERE id=?1",
            "unit-1"
        ),
        "stale"
    );
    let listed = recovery.list().expect("list");
    let project = listed
        .iter()
        .find(|row| row.id == "project-index:pix-1")
        .expect("project index recorded");
    assert_eq!(project.kind, "project-index");
    assert_eq!(project.execution_id, None);
    assert!(listed.iter().any(|row| row.id == "semantic-index:six-1"));
}

#[test]
fn a_clean_database_records_nothing() {
    let db = open("recovery-clean");
    let recovery = OperationRecovery::new(db.connection());

    recovery.reconcile().expect("reconcile");

    assert!(recovery.list().expect("list").is_empty());
    assert!(recovery.pending_decisions().expect("pending").is_empty());
}
