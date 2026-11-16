//! Change sets, their file changes and hunk annotations. Port of `ChangeSetRepository.ts`.
//!
//! A change set is the file-level result of one agent mutation attempt. Hunks annotate a
//! review and never change the bytes of the workspace.

use noct_core::change_control::{
    ChangeHunkRecord, ChangeOperation, ChangeOrigin, ChangePolicy, ChangeRecord, ChangeSetRecord,
    ChangeSetStatus, ChangeStatus, HunkStatus, ReviewDecision, ValidationStatus,
};
use noct_core::workspace_evidence::{EvidenceKind, EvidenceReference, KnownWorkspacePath};
use rusqlite::types::ToSql;
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::clock::iso_now;
use crate::codec::{enum_text, enum_value};
use crate::repositories::workspace_evidence::{EvidenceInput, WorkspaceEvidence};
use crate::{Error, Result, transaction};

const INVALID_CHANGE_SET: &str = "O ChangeSet persistido possui um estado inválido.";
const INVALID_CHANGE: &str = "A mudança persistida possui um estado inválido.";
const INVALID_HUNK: &str = "O hunk persistido possui um estado inválido.";
const MISSING_EXECUTION: &str = "Execução não encontrada para registrar a decisão.";

const CHANGE_SET_COLUMNS: &str = "id, execution_id, before_checkpoint_id, after_checkpoint_id, \
    status, created_at, updated_at";

const CHANGE_COLUMNS: &str = "id, change_set_id, execution_id, checkpoint_id, relative_path, \
    original_path, operation, origin, before_hash, after_hash, before_size, after_size, status, \
    validation_status, created_at, updated_at, policy, policy_reason";

const HUNK_COLUMNS: &str = "id, change_id, sequence, base_hash, original_patch, final_patch, \
    status, start_line, end_line, decision_at";

pub struct ChangeSets<'a> {
    conn: &'a Connection,
}

impl<'a> ChangeSets<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Records that a review decision is about to change a file. Returns the operation id.
    pub fn reserve_decision(
        &self,
        execution_id: &str,
        change_id: &str,
        decision: ReviewDecision,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO change_decision_operations(id,execution_id,change_id,decision,status,started_at) \
             VALUES(?1,?2,?3,?4,'running',?5)",
            params![id, execution_id, change_id, enum_text(&decision)?, iso_now()?],
        )?;
        Ok(id)
    }

    pub fn fail_decision(&self, id: &str, error: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE change_decision_operations SET status='conflicted',error=?1,finished_at=?2 WHERE id=?3",
            params![error, iso_now()?, id],
        )?;
        Ok(())
    }

    pub fn create(&self, change_set: &ChangeSetRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO change_sets(id,execution_id,before_checkpoint_id,after_checkpoint_id,status,\
             created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                change_set.id,
                change_set.execution_id,
                change_set.before_checkpoint_id,
                change_set.after_checkpoint_id,
                enum_text(&change_set.status)?,
                change_set.created_at,
                change_set.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn update(&self, change_set: &ChangeSetRecord) -> Result<()> {
        self.conn.execute(
            "UPDATE change_sets SET status=?1,updated_at=?2 WHERE id=?3 AND execution_id=?4",
            params![
                enum_text(&change_set.status)?,
                change_set.updated_at,
                change_set.id,
                change_set.execution_id,
            ],
        )?;
        Ok(())
    }

    /// Replaces the file changes of a change set in one unit of work.
    pub fn replace_changes(&self, change_set_id: &str, changes: &[ChangeRecord]) -> Result<()> {
        transaction::run(self.conn, |conn| {
            conn.execute(
                "DELETE FROM changes WHERE change_set_id=?1",
                [change_set_id],
            )?;
            insert_changes(conn, changes)
        })
    }

    /// Creates or updates the change set, then replaces its file changes.
    pub fn save(&self, change_set: &ChangeSetRecord, changes: &[ChangeRecord]) -> Result<()> {
        transaction::run(self.conn, |conn| {
            let exists: Option<i64> = conn
                .query_row(
                    "SELECT 1 FROM change_sets WHERE id=?1 AND execution_id=?2",
                    params![change_set.id, change_set.execution_id],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_some() {
                self.update(change_set)?;
            } else {
                self.create(change_set)?;
            }
            conn.execute(
                "DELETE FROM changes WHERE change_set_id=?1",
                [&change_set.id],
            )?;
            insert_changes(conn, changes)
        })
    }

    /// The change set with this identifier, optionally only within one execution.
    pub fn get_by_id(
        &self,
        id: &str,
        execution_id: Option<&str>,
    ) -> Result<Option<ChangeSetRecord>> {
        let row = self
            .conn
            .query_row(
                &format!(
                    "SELECT {CHANGE_SET_COLUMNS} FROM change_sets WHERE id=?1 AND (?2 IS NULL OR execution_id=?2)"
                ),
                params![id, execution_id],
                read_change_set,
            )
            .optional()?;
        row.map(change_set).transpose()
    }

    /// The most recently updated change set of an execution. An execution may have more
    /// than one capture after a retry, and the review surface shows the latest.
    pub fn get_by_execution_id(&self, execution_id: &str) -> Result<Option<ChangeSetRecord>> {
        let row = self
            .conn
            .query_row(
                &format!(
                    "SELECT {CHANGE_SET_COLUMNS} FROM change_sets WHERE execution_id=?1 \
                     ORDER BY updated_at DESC, created_at DESC, rowid DESC LIMIT 1"
                ),
                [execution_id],
                read_change_set,
            )
            .optional()?;
        row.map(change_set).transpose()
    }

    pub fn list(&self, execution_id: &str) -> Result<Vec<ChangeSetRecord>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {CHANGE_SET_COLUMNS} FROM change_sets WHERE execution_id=?1 \
             ORDER BY updated_at DESC, created_at DESC, rowid DESC"
        ))?;
        let rows = statement
            .query_map([execution_id], read_change_set)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(change_set).collect()
    }

    pub fn list_changes(&self, change_set_id: &str) -> Result<Vec<ChangeRecord>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {CHANGE_COLUMNS} FROM changes WHERE change_set_id=?1 ORDER BY relative_path"
        ))?;
        let rows = statement
            .query_map([change_set_id], read_change)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(change).collect()
    }

    pub fn get_change(&self, id: &str, execution_id: Option<&str>) -> Result<Option<ChangeRecord>> {
        let row = self
            .conn
            .query_row(
                &format!(
                    "SELECT {CHANGE_COLUMNS} FROM changes WHERE id=?1 AND (?2 IS NULL OR execution_id=?2)"
                ),
                params![id, execution_id],
                read_change,
            )
            .optional()?;
        row.map(change).transpose()
    }

    pub fn update_change(&self, record: &ChangeRecord) -> Result<()> {
        self.conn.execute(
            "UPDATE changes SET status=?1,validation_status=?2,after_hash=?3,after_size=?4,updated_at=?5 \
             WHERE id=?6 AND change_set_id=?7 AND execution_id=?8",
            params![
                enum_text(&record.status)?,
                enum_text(&record.validation_status)?,
                record.after_hash,
                record.after_size,
                record.updated_at,
                record.id,
                record.change_set_id,
                record.execution_id,
            ],
        )?;
        Ok(())
    }

    pub fn create_hunk(&self, hunk: &ChangeHunkRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO change_hunks(id,change_id,sequence,base_hash,original_patch,final_patch,status,\
             start_line,end_line,decision_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                hunk.id,
                hunk.change_id,
                hunk.sequence,
                hunk.base_hash,
                hunk.original_patch,
                hunk.final_patch,
                enum_text(&hunk.status)?,
                hunk.start_line,
                hunk.end_line,
                hunk.decision_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_hunks(&self, change_id: &str) -> Result<Vec<ChangeHunkRecord>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {HUNK_COLUMNS} FROM change_hunks WHERE change_id=?1 ORDER BY sequence"
        ))?;
        let rows = statement
            .query_map([change_id], read_hunk)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(hunk).collect()
    }

    pub fn get_hunk(&self, id: &str) -> Result<Option<ChangeHunkRecord>> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT {HUNK_COLUMNS} FROM change_hunks WHERE id=?1"),
                [id],
                read_hunk,
            )
            .optional()?;
        row.map(hunk).transpose()
    }

    pub fn update_hunk(&self, record: &ChangeHunkRecord) -> Result<()> {
        self.conn.execute(
            "UPDATE change_hunks SET final_patch=?1,status=?2,decision_at=?3 WHERE id=?4 AND change_id=?5",
            params![
                record.final_patch,
                enum_text(&record.status)?,
                record.decision_at,
                record.id,
                record.change_id,
            ],
        )?;
        Ok(())
    }

    /// Stores a decision on one change: the change set, the change, the execution's decision,
    /// the decision evidence and the completed operation, as one unit of work.
    pub fn save_decision(
        &self,
        change_set: &ChangeSetRecord,
        change: &ChangeRecord,
        operation_id: Option<&str>,
    ) -> Result<()> {
        transaction::run(self.conn, |conn| {
            self.update(change_set)?;
            self.update_change(change)?;
            conn.execute(
                "UPDATE executions SET decision=?1 WHERE id=?2",
                params![enum_text(&change_set.status)?, change_set.execution_id],
            )?;
            if let Some(operation_id) = operation_id {
                let workspace: Option<String> = conn
                    .query_row(
                        "SELECT workspace FROM executions WHERE id=?1",
                        [&change_set.execution_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                let workspace =
                    workspace.ok_or_else(|| Error::Message(MISSING_EXECUTION.to_string()))?;
                let hash = match change.status {
                    ChangeStatus::Rejected => change.before_hash.clone(),
                    _ => change.after_hash.clone(),
                };
                WorkspaceEvidence::new(conn).record(EvidenceInput {
                    kind: EvidenceKind::Decision,
                    workspace,
                    source_id: operation_id.to_string(),
                    execution_id: Some(change_set.execution_id.clone()),
                    started_at: None,
                    paths: vec![KnownWorkspacePath {
                        path: change.relative_path.clone(),
                        hash,
                        exists: None,
                    }],
                    references: vec![
                        reference("before", &change_set.before_checkpoint_id),
                        reference("after", &change_set.after_checkpoint_id),
                        reference("change", &change.id),
                    ],
                })?;
                conn.execute(
                    "UPDATE change_decision_operations SET status='completed',finished_at=?1 \
                     WHERE id=?2 AND status='running'",
                    params![change.updated_at, operation_id],
                )?;
            }
            Ok(())
        })
    }
}

fn reference(kind: &str, id: &str) -> EvidenceReference {
    EvidenceReference {
        kind: kind.to_string(),
        id: id.to_string(),
        payload_hash: None,
    }
}

/// The values bound to one file change, in the column order of `CHANGE_INSERT`.
struct ChangeValues {
    id: String,
    change_set_id: String,
    execution_id: String,
    checkpoint_id: Option<String>,
    relative_path: String,
    original_path: Option<String>,
    operation: String,
    origin: String,
    before_hash: Option<String>,
    after_hash: Option<String>,
    before_size: Option<i64>,
    after_size: Option<i64>,
    status: String,
    validation_status: String,
    created_at: String,
    updated_at: String,
    policy: String,
    policy_reason: Option<String>,
}

impl ChangeValues {
    fn from(record: &ChangeRecord) -> Result<Self> {
        Ok(Self {
            id: record.id.clone(),
            change_set_id: record.change_set_id.clone(),
            execution_id: record.execution_id.clone(),
            checkpoint_id: record.checkpoint_id.clone(),
            relative_path: record.relative_path.clone(),
            original_path: record.original_path.clone(),
            operation: enum_text(&record.operation)?,
            origin: enum_text(&record.origin)?,
            before_hash: record.before_hash.clone(),
            after_hash: record.after_hash.clone(),
            before_size: record.before_size,
            after_size: record.after_size,
            status: enum_text(&record.status)?,
            validation_status: enum_text(&record.validation_status)?,
            created_at: record.created_at.clone(),
            updated_at: record.updated_at.clone(),
            policy: enum_text(&record.policy)?,
            policy_reason: record.policy_reason.clone(),
        })
    }

    fn bind(&self) -> Vec<&dyn ToSql> {
        vec![
            &self.id,
            &self.change_set_id,
            &self.execution_id,
            &self.checkpoint_id,
            &self.relative_path,
            &self.original_path,
            &self.operation,
            &self.origin,
            &self.before_hash,
            &self.after_hash,
            &self.before_size,
            &self.after_size,
            &self.status,
            &self.validation_status,
            &self.created_at,
            &self.updated_at,
            &self.policy,
            &self.policy_reason,
        ]
    }
}

fn insert_changes(conn: &Connection, changes: &[ChangeRecord]) -> Result<()> {
    for record in changes {
        let values = ChangeValues::from(record)?;
        let bound = values.bind();
        conn.execute(
            "INSERT INTO changes(id,change_set_id,execution_id,checkpoint_id,relative_path,original_path,\
             operation,origin,before_hash,after_hash,before_size,after_size,status,validation_status,\
             created_at,updated_at,policy,policy_reason) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
            bound.as_slice(),
        )?;
    }
    Ok(())
}

struct ChangeSetRow {
    id: String,
    execution_id: String,
    before_checkpoint_id: String,
    after_checkpoint_id: String,
    status: String,
    created_at: String,
    updated_at: String,
}

fn read_change_set(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChangeSetRow> {
    Ok(ChangeSetRow {
        id: row.get(0)?,
        execution_id: row.get(1)?,
        before_checkpoint_id: row.get(2)?,
        after_checkpoint_id: row.get(3)?,
        status: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn change_set(row: ChangeSetRow) -> Result<ChangeSetRecord> {
    let invalid = || Error::Message(INVALID_CHANGE_SET.to_string());
    Ok(ChangeSetRecord {
        id: row.id,
        execution_id: row.execution_id,
        before_checkpoint_id: row.before_checkpoint_id,
        after_checkpoint_id: row.after_checkpoint_id,
        status: enum_value::<ChangeSetStatus>(&row.status).ok_or_else(invalid)?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

struct ChangeRow {
    id: String,
    change_set_id: String,
    execution_id: String,
    checkpoint_id: Option<String>,
    relative_path: String,
    original_path: Option<String>,
    operation: String,
    origin: String,
    before_hash: Option<String>,
    after_hash: Option<String>,
    before_size: Option<i64>,
    after_size: Option<i64>,
    status: String,
    validation_status: String,
    created_at: String,
    updated_at: String,
    policy: String,
    policy_reason: Option<String>,
}

fn read_change(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChangeRow> {
    Ok(ChangeRow {
        id: row.get(0)?,
        change_set_id: row.get(1)?,
        execution_id: row.get(2)?,
        checkpoint_id: row.get(3)?,
        relative_path: row.get(4)?,
        original_path: row.get(5)?,
        operation: row.get(6)?,
        origin: row.get(7)?,
        before_hash: row.get(8)?,
        after_hash: row.get(9)?,
        before_size: row.get(10)?,
        after_size: row.get(11)?,
        status: row.get(12)?,
        validation_status: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
        policy: row.get(16)?,
        policy_reason: row.get(17)?,
    })
}

fn change(row: ChangeRow) -> Result<ChangeRecord> {
    let invalid = || Error::Message(INVALID_CHANGE.to_string());
    Ok(ChangeRecord {
        id: row.id,
        execution_id: row.execution_id,
        change_set_id: row.change_set_id,
        checkpoint_id: row.checkpoint_id,
        relative_path: row.relative_path,
        original_path: row.original_path,
        operation: enum_value::<ChangeOperation>(&row.operation).ok_or_else(invalid)?,
        origin: enum_value::<ChangeOrigin>(&row.origin).ok_or_else(invalid)?,
        before_hash: row.before_hash,
        after_hash: row.after_hash,
        before_size: row.before_size,
        after_size: row.after_size,
        status: enum_value::<ChangeStatus>(&row.status).ok_or_else(invalid)?,
        validation_status: enum_value::<ValidationStatus>(&row.validation_status)
            .ok_or_else(invalid)?,
        policy: enum_value::<ChangePolicy>(&row.policy).ok_or_else(invalid)?,
        policy_reason: row.policy_reason,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

struct HunkRow {
    id: String,
    change_id: String,
    sequence: i64,
    base_hash: String,
    original_patch: String,
    final_patch: String,
    status: String,
    start_line: i64,
    end_line: i64,
    decision_at: Option<String>,
}

fn read_hunk(row: &rusqlite::Row<'_>) -> rusqlite::Result<HunkRow> {
    Ok(HunkRow {
        id: row.get(0)?,
        change_id: row.get(1)?,
        sequence: row.get(2)?,
        base_hash: row.get(3)?,
        original_patch: row.get(4)?,
        final_patch: row.get(5)?,
        status: row.get(6)?,
        start_line: row.get(7)?,
        end_line: row.get(8)?,
        decision_at: row.get(9)?,
    })
}

fn hunk(row: HunkRow) -> Result<ChangeHunkRecord> {
    let invalid = || Error::Message(INVALID_HUNK.to_string());
    Ok(ChangeHunkRecord {
        id: row.id,
        change_id: row.change_id,
        sequence: row.sequence,
        base_hash: row.base_hash,
        original_patch: row.original_patch,
        final_patch: row.final_patch,
        status: enum_value::<HunkStatus>(&row.status).ok_or_else(invalid)?,
        start_line: row.start_line,
        end_line: row.end_line,
        decision_at: row.decision_at,
    })
}
