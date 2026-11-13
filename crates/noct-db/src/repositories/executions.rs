//! Agent executions. Port of `ExecutionRepository.ts`: the durable identity and outcome of
//! each attempt. A terminal outcome can no longer change.

use noct_core::agent::AgentMode;
use noct_core::change_control::{
    ExecutionDecision, ExecutionOverview, ExecutionRecord, ExecutionStatus,
};
use noct_core::limits::persistence;
use noct_core::text::utf16_prefix;
use rusqlite::types::ToSql;
use rusqlite::{Connection, OptionalExtension, params};

use crate::codec::{enum_text, enum_value};
use crate::{Error, Result, transaction};

const TERMINAL_CHANGE: &str = "O resultado terminal de uma execução é imutável.";
const INVALID_EXECUTION: &str = "A execução persistida possui um estado inválido.";

const OVERVIEW: &str = "SELECT e.id, e.workspace, e.conversation_id, e.prompt, e.mode, e.status, \
    e.decision, e.retry_of, e.started_at, e.finished_at, e.error, \
    (SELECT COUNT(*) FROM changes c WHERE c.execution_id=e.id), \
    (SELECT COUNT(DISTINCT validation_id) FROM execution_validation_links vl WHERE vl.execution_id=e.id), \
    (SELECT COUNT(*) FROM checkpoints cp WHERE cp.execution_id=e.id) \
    FROM executions e";

pub struct Executions<'a> {
    conn: &'a Connection,
}

impl<'a> Executions<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create(&self, record: &ExecutionRecord) -> Result<()> {
        let parameters = Parameters::from(record)?;
        let bound = parameters.bind();
        self.conn.execute(
            "INSERT INTO executions(id,workspace,conversation_id,prompt,mode,status,decision,retry_of,\
             started_at,finished_at,error) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            bound.as_slice(),
        )?;
        Ok(())
    }

    pub fn update(&self, record: &ExecutionRecord) -> Result<()> {
        if let Some(current) = self.get(&record.id, Some(&record.workspace))? {
            let status = current.execution.status;
            if status.is_terminal() && status != record.status {
                return Err(Error::Message(TERMINAL_CHANGE.to_string()));
            }
        }
        let parameters = Parameters::from(record)?;
        let bound = parameters.bind();
        self.conn.execute(
            "UPDATE executions SET prompt=?4,mode=?5,status=?6,decision=?7,retry_of=?8,\
             started_at=?9,finished_at=?10,error=?11 \
             WHERE id=?1 AND workspace=?2 AND conversation_id=?3",
            bound.as_slice(),
        )?;
        Ok(())
    }

    /// Creates the execution, or updates it when the identity is already stored.
    pub fn save(&self, record: &ExecutionRecord) -> Result<()> {
        transaction::run(self.conn, |_| {
            let exists: Option<i64> = self
                .conn
                .query_row(
                    "SELECT 1 FROM executions WHERE id=?1 AND workspace=?2",
                    params![record.id, record.workspace],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_some() {
                self.update(record)
            } else {
                self.create(record)
            }
        })
    }

    pub fn get(&self, id: &str, workspace: Option<&str>) -> Result<Option<ExecutionOverview>> {
        let row = self
            .conn
            .query_row(
                &format!("{OVERVIEW} WHERE e.id=?1 AND (?2 IS NULL OR e.workspace=?2)"),
                params![id, workspace],
                read_row,
            )
            .optional()?;
        row.map(overview).transpose()
    }

    pub fn list(
        &self,
        workspace: &str,
        conversation_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ExecutionOverview>> {
        let limit = limit.clamp(1, 200);
        let mut statement = self.conn.prepare(&format!(
            "{OVERVIEW} WHERE e.workspace=?1 AND (?2 IS NULL OR e.conversation_id=?2) \
             ORDER BY e.started_at DESC LIMIT ?3"
        ))?;
        let rows = statement
            .query_map(params![workspace, conversation_id, limit], read_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(overview).collect()
    }
}

/// The values bound to an execution, with prompt and error cut to their stored limits.
struct Parameters {
    id: String,
    workspace: String,
    conversation_id: String,
    prompt: String,
    mode: String,
    status: String,
    decision: String,
    retry_of: Option<String>,
    started_at: String,
    finished_at: Option<String>,
    error: Option<String>,
}

impl Parameters {
    fn from(record: &ExecutionRecord) -> Result<Self> {
        Ok(Self {
            id: record.id.clone(),
            workspace: record.workspace.clone(),
            conversation_id: record.conversation_id.clone(),
            prompt: utf16_prefix(&record.prompt, persistence::ASSISTANT_CHARACTERS as usize),
            mode: enum_text(&record.mode)?,
            status: enum_text(&record.status)?,
            decision: enum_text(&record.decision)?,
            retry_of: record.retry_of.clone(),
            started_at: record.started_at.clone(),
            finished_at: record.finished_at.clone(),
            error: record
                .error
                .as_deref()
                .map(|error| utf16_prefix(error, persistence::METADATA_CHARACTERS as usize)),
        })
    }

    fn bind(&self) -> Vec<&dyn ToSql> {
        vec![
            &self.id,
            &self.workspace,
            &self.conversation_id,
            &self.prompt,
            &self.mode,
            &self.status,
            &self.decision,
            &self.retry_of,
            &self.started_at,
            &self.finished_at,
            &self.error,
        ]
    }
}

struct Row {
    id: String,
    workspace: String,
    conversation_id: String,
    prompt: String,
    mode: String,
    status: String,
    decision: String,
    retry_of: Option<String>,
    started_at: String,
    finished_at: Option<String>,
    error: Option<String>,
    change_count: i64,
    validation_count: i64,
    checkpoint_count: i64,
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    Ok(Row {
        id: row.get(0)?,
        workspace: row.get(1)?,
        conversation_id: row.get(2)?,
        prompt: row.get(3)?,
        mode: row.get(4)?,
        status: row.get(5)?,
        decision: row.get(6)?,
        retry_of: row.get(7)?,
        started_at: row.get(8)?,
        finished_at: row.get(9)?,
        error: row.get(10)?,
        change_count: row.get(11)?,
        validation_count: row.get(12)?,
        checkpoint_count: row.get(13)?,
    })
}

fn overview(row: Row) -> Result<ExecutionOverview> {
    let invalid = || Error::Message(INVALID_EXECUTION.to_string());
    let status: ExecutionStatus = enum_value(&row.status).ok_or_else(invalid)?;
    let decision: ExecutionDecision = enum_value(&row.decision).ok_or_else(invalid)?;
    let mode: AgentMode = enum_value(&row.mode).ok_or_else(invalid)?;
    Ok(ExecutionOverview {
        execution: ExecutionRecord {
            id: row.id,
            workspace: row.workspace,
            conversation_id: row.conversation_id,
            prompt: row.prompt,
            mode,
            status,
            decision,
            retry_of: row.retry_of,
            started_at: row.started_at,
            finished_at: row.finished_at,
            error: row.error,
        },
        change_count: count(row.change_count).ok_or_else(invalid)?,
        validation_count: count(row.validation_count).ok_or_else(invalid)?,
        checkpoint_count: count(row.checkpoint_count).ok_or_else(invalid)?,
    })
}

fn count(value: i64) -> Option<u64> {
    u64::try_from(value).ok()
}
