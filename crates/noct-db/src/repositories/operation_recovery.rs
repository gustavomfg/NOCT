//! Startup classification of interrupted operations. Port of
//! `OperationRecoveryRepository.ts`.
//!
//! Reconciliation changes database state only. It touches no file, restarts no process,
//! and never assumes that an interrupted operation succeeded.

use noct_core::operation_recovery::InterruptedOperation;
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::clock::iso_now;
use crate::{Result, transaction};

const INTERRUPTED: &str = "Operação interrompida; o estado precisa ser reconciliado. Nenhum trabalho foi retomado automaticamente.";
const AWAITING_DECISION: &str = "Decisão pendente recuperada; revise os arquivos atuais antes de decidir. Nenhuma decisão foi aplicada automaticamente.";

/// A table whose unfinished rows are marked failed at startup. The strings are fixed
/// here, never taken from input, so building the statements from them is safe.
struct Source {
    table: &'static str,
    kind: &'static str,
    predicate: &'static str,
    /// The column that names the owning execution, or `NULL` when there is none.
    execution: &'static str,
    /// The column that records when the row finished, when the table has one.
    finished: Option<&'static str>,
}

const SOURCES: [Source; 5] = [
    Source {
        table: "executions",
        kind: "execution",
        predicate: "status IN ('created','planning','running','validating','awaiting-review')",
        execution: "id",
        finished: Some("finished_at"),
    },
    Source {
        table: "checkpoints",
        kind: "checkpoint",
        predicate: "status='capturing'",
        execution: "execution_id",
        finished: None,
    },
    Source {
        table: "validation_runs",
        kind: "validation",
        predicate: "status IN ('queued','running')",
        execution: "execution_id",
        finished: Some("completed_at"),
    },
    Source {
        table: "project_index_runs",
        kind: "project-index",
        predicate: "status IN ('queued','running')",
        execution: "NULL",
        finished: Some("completed_at"),
    },
    Source {
        table: "semantic_index_runs",
        kind: "semantic-index",
        predicate: "status IN ('queued','running')",
        execution: "NULL",
        finished: Some("completed_at"),
    },
];

/// A change set that still waits for a decision, with the execution it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDecision {
    pub execution_id: String,
    pub workspace: String,
}

pub struct OperationRecovery<'a> {
    conn: &'a Connection,
}

impl<'a> OperationRecovery<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Records and closes every operation that was running when the application stopped.
    /// Running again on a reconciled database changes nothing.
    pub fn reconcile(&self) -> Result<()> {
        transaction::run(self.conn, |conn| {
            let now = iso_now()?;
            for source in &SOURCES {
                conn.execute(
                    &format!(
                        "INSERT OR IGNORE INTO interrupted_operations\
                         (id,workspace,execution_id,kind,source_id,original_status,detected_at,message) \
                         SELECT ?1 || ':' || id, workspace, {execution}, ?2, id, status, ?3, ?4 \
                         FROM {table} WHERE {predicate}",
                        execution = source.execution,
                        table = source.table,
                        predicate = source.predicate,
                    ),
                    params![source.kind, source.kind, now, INTERRUPTED],
                )?;
                match source.finished {
                    Some(column) => conn.execute(
                        &format!(
                            "UPDATE {table} SET status='failed',error=?1,{column}=?2 WHERE {predicate}",
                            table = source.table,
                            predicate = source.predicate,
                        ),
                        params![INTERRUPTED, now],
                    )?,
                    None => conn.execute(
                        &format!(
                            "UPDATE {table} SET status='failed',error=?1 WHERE {predicate}",
                            table = source.table,
                            predicate = source.predicate,
                        ),
                        params![INTERRUPTED],
                    )?,
                };
            }

            conn.execute(
                "INSERT OR IGNORE INTO interrupted_operations\
                 (id,workspace,execution_id,kind,source_id,original_status,detected_at,message) \
                 SELECT 'decision:' || o.id, e.workspace, o.execution_id, 'decision', o.id, o.status, ?1, ?2 \
                 FROM change_decision_operations o JOIN executions e ON e.id=o.execution_id \
                 WHERE o.status='running'",
                params![now, INTERRUPTED],
            )?;
            conn.execute_batch(
                "UPDATE changes SET status='conflicted' \
                     WHERE id IN (SELECT change_id FROM change_decision_operations WHERE status='running');\
                 UPDATE change_sets SET status='conflicted' \
                     WHERE id IN (SELECT change_set_id FROM changes \
                         WHERE id IN (SELECT change_id FROM change_decision_operations WHERE status='running'));\
                 UPDATE executions SET decision='conflicted' \
                     WHERE id IN (SELECT execution_id FROM change_decision_operations WHERE status='running');",
            )?;
            conn.execute(
                "UPDATE change_decision_operations SET status='interrupted',finished_at=?1,error=?2 \
                 WHERE status='running'",
                params![now, INTERRUPTED],
            )?;

            conn.execute(
                "INSERT OR IGNORE INTO interrupted_operations\
                 (id,workspace,execution_id,kind,source_id,original_status,detected_at,message) \
                 SELECT 'command:' || c.id, e.workspace, c.execution_id, 'command', c.id, c.status, ?1, ?2 \
                 FROM execution_commands c JOIN executions e ON e.id=c.execution_id \
                 WHERE c.status='running'",
                params![now, INTERRUPTED],
            )?;
            conn.execute(
                "UPDATE execution_commands SET status='failed',finished_at=?1,output_summary=?2 \
                 WHERE status='running'",
                params![now, INTERRUPTED],
            )?;

            // Units being embedded are not known to be finished, so they are stale.
            conn.execute_batch(
                "UPDATE semantic_units SET status='stale' WHERE status='processing'",
            )?;

            conn.execute(
                "INSERT OR IGNORE INTO interrupted_operations\
                 (id,workspace,execution_id,kind,source_id,original_status,detected_at,message) \
                 SELECT 'awaiting-decision:' || c.id, e.workspace, c.execution_id, 'awaiting-decision', \
                     c.id, c.status, ?1, ?2 \
                 FROM change_sets c JOIN executions e ON e.id=c.execution_id \
                 WHERE c.status IN ('pending','partially-accepted','conflicted')",
                params![now, AWAITING_DECISION],
            )?;
            Ok(())
        })
    }

    /// The most recent interrupted operations, newest first, at most 200.
    pub fn list(&self) -> Result<Vec<InterruptedOperation>> {
        let mut statement = self.conn.prepare(
            "SELECT id, workspace, execution_id, kind, source_id, original_status, detected_at, message \
             FROM interrupted_operations ORDER BY detected_at DESC, id LIMIT 200",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(InterruptedOperation {
                    id: row.get(0)?,
                    workspace: row.get(1)?,
                    execution_id: row.get(2)?,
                    kind: row.get(3)?,
                    source_id: row.get(4)?,
                    original_status: row.get(5)?,
                    detected_at: row.get(6)?,
                    message: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn pending_decisions(&self) -> Result<Vec<PendingDecision>> {
        let mut statement = self.conn.prepare(
            "SELECT DISTINCT c.execution_id, e.workspace FROM change_sets c \
             JOIN executions e ON e.id=c.execution_id \
             WHERE c.status IN ('pending','partially-accepted','conflicted')",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(PendingDecision {
                    execution_id: row.get(0)?,
                    workspace: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
