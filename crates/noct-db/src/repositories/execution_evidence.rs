//! Bounded execution evidence: commands, errors and validation links. Port of
//! `ExecutionEvidenceRepository.ts`. Every text is cut to its stored limit.

use noct_core::change_control::{
    ExecutionCommandRecord, ExecutionCommandSource, ExecutionCommandStatus, ExecutionErrorRecord,
    ExecutionErrorStage, ExecutionValidationLink, ExecutionValidationPhase,
};
use noct_core::limits::persistence::METADATA_CHARACTERS;
use noct_core::text::utf16_prefix;
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::clock::iso_now;
use crate::codec::{enum_text, enum_value};
use crate::{Error, Result, transaction};

const PATH_CHARACTERS: usize = 4_096;
const ARGUMENT_COUNT: usize = 100;
const DEFAULT_LIST_LIMIT: i64 = 100;
const MAX_LIST_LIMIT: i64 = 500;
const INVALID_COMMAND: &str = "O histórico de comandos possui argumentos inválidos.";
const INVALID_ROW: &str = "A evidência de execução persistida está inválida.";

pub struct ExecutionEvidence<'a> {
    conn: &'a Connection,
}

impl<'a> ExecutionEvidence<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create_command(&self, record: &ExecutionCommandRecord) -> Result<()> {
        let values = CommandValues::from(record)?;
        let bound = values.bind();
        self.conn.execute(
            "INSERT INTO execution_commands(id,execution_id,command,args_json,source,status,exit_code,\
             duration_ms,output_summary,started_at,finished_at) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            bound.as_slice(),
        )?;
        Ok(())
    }

    pub fn update_command(&self, record: &ExecutionCommandRecord) -> Result<()> {
        let values = CommandValues::from(record)?;
        let bound = values.bind();
        self.conn.execute(
            "UPDATE execution_commands SET command=?3,args_json=?4,source=?5,status=?6,exit_code=?7,\
             duration_ms=?8,output_summary=?9,started_at=?10,finished_at=?11 \
             WHERE id=?1 AND execution_id=?2",
            bound.as_slice(),
        )?;
        Ok(())
    }

    pub fn save_command(&self, record: &ExecutionCommandRecord) -> Result<()> {
        transaction::run(self.conn, |_| {
            let exists: Option<i64> = self
                .conn
                .query_row(
                    "SELECT 1 FROM execution_commands WHERE id=?1 AND execution_id=?2",
                    params![record.id, record.execution_id],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_some() {
                self.update_command(record)
            } else {
                self.create_command(record)
            }
        })
    }

    /// Commands of an execution in start order. As in the TypeScript repository, the default
    /// limit is 100 and the largest accepted limit is 500.
    pub fn list_commands(
        &self,
        execution_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<ExecutionCommandRecord>> {
        let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT);
        let mut statement = self.conn.prepare(
            "SELECT id, execution_id, command, args_json, source, status, exit_code, duration_ms, \
             output_summary, started_at, finished_at FROM execution_commands \
             WHERE execution_id=?1 ORDER BY started_at LIMIT ?2",
        )?;
        let rows = statement
            .query_map(params![execution_id, limit], |row| {
                Ok(CommandRow {
                    id: row.get(0)?,
                    execution_id: row.get(1)?,
                    command: row.get(2)?,
                    args_json: row.get(3)?,
                    source: row.get(4)?,
                    status: row.get(5)?,
                    exit_code: row.get(6)?,
                    duration_ms: row.get(7)?,
                    output_summary: row.get(8)?,
                    started_at: row.get(9)?,
                    finished_at: row.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(command).collect()
    }

    pub fn create_error(&self, record: &ExecutionErrorRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO execution_errors(id,execution_id,stage,message,path,created_at) \
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                record.id,
                record.execution_id,
                enum_text(&record.stage)?,
                utf16_prefix(&record.message, METADATA_CHARACTERS as usize),
                record
                    .path
                    .as_deref()
                    .map(|path| utf16_prefix(path, PATH_CHARACTERS)),
                record.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_errors(
        &self,
        execution_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<ExecutionErrorRecord>> {
        let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT);
        let mut statement = self.conn.prepare(
            "SELECT id, execution_id, stage, message, path, created_at FROM execution_errors \
             WHERE execution_id=?1 ORDER BY created_at LIMIT ?2",
        )?;
        let rows = statement
            .query_map(params![execution_id, limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(id, execution_id, stage, message, path, created_at)| {
                Ok(ExecutionErrorRecord {
                    id,
                    execution_id,
                    stage: enum_value::<ExecutionErrorStage>(&stage)
                        .ok_or_else(|| Error::Message(INVALID_ROW.to_string()))?,
                    message,
                    path,
                    created_at,
                })
            })
            .collect()
    }

    /// Links a validation to an execution, once for each execution, validation, phase and change.
    pub fn link_validation(&self, link: &ExecutionValidationLink) -> Result<()> {
        let phase = enum_text(&link.phase)?;
        let exists: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM execution_validation_links \
                 WHERE execution_id=?1 AND validation_id=?2 AND phase=?3 AND (change_id IS ?4 OR change_id=?4)",
                params![link.execution_id, link.validation_id, phase, link.change_id],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO execution_validation_links(id,execution_id,change_id,validation_id,phase,created_at) \
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                Uuid::new_v4().to_string(),
                link.execution_id,
                link.change_id,
                link.validation_id,
                phase,
                iso_now()?,
            ],
        )?;
        Ok(())
    }

    pub fn list_validation_links(
        &self,
        execution_id: &str,
    ) -> Result<Vec<ExecutionValidationLink>> {
        let mut statement = self.conn.prepare(
            "SELECT execution_id, change_id, validation_id, phase FROM execution_validation_links \
             WHERE execution_id=?1 ORDER BY created_at",
        )?;
        let rows = statement
            .query_map([execution_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(execution_id, change_id, validation_id, phase)| {
                Ok(ExecutionValidationLink {
                    execution_id,
                    change_id,
                    validation_id,
                    phase: enum_value::<ExecutionValidationPhase>(&phase)
                        .ok_or_else(|| Error::Message(INVALID_ROW.to_string()))?,
                })
            })
            .collect()
    }
}

struct CommandValues {
    id: String,
    execution_id: String,
    command: String,
    args_json: String,
    source: String,
    status: String,
    exit_code: Option<i64>,
    duration_ms: Option<i64>,
    output_summary: String,
    started_at: String,
    finished_at: Option<String>,
}

impl CommandValues {
    fn from(record: &ExecutionCommandRecord) -> Result<Self> {
        let args: Vec<String> = record
            .args
            .iter()
            .take(ARGUMENT_COUNT)
            .map(|arg| utf16_prefix(arg, PATH_CHARACTERS))
            .collect();
        Ok(Self {
            id: record.id.clone(),
            execution_id: record.execution_id.clone(),
            command: utf16_prefix(&record.command, PATH_CHARACTERS),
            args_json: serde_json::to_string(&args)
                .map_err(|_| Error::Message(INVALID_COMMAND.to_string()))?,
            source: enum_text(&record.source)?,
            status: enum_text(&record.status)?,
            exit_code: record.exit_code,
            duration_ms: record.duration_ms,
            output_summary: utf16_prefix(&record.output_summary, METADATA_CHARACTERS as usize),
            started_at: record.started_at.clone(),
            finished_at: record.finished_at.clone(),
        })
    }

    fn bind(&self) -> Vec<&dyn rusqlite::types::ToSql> {
        vec![
            &self.id,
            &self.execution_id,
            &self.command,
            &self.args_json,
            &self.source,
            &self.status,
            &self.exit_code,
            &self.duration_ms,
            &self.output_summary,
            &self.started_at,
            &self.finished_at,
        ]
    }
}

struct CommandRow {
    id: String,
    execution_id: String,
    command: String,
    args_json: String,
    source: String,
    status: String,
    exit_code: Option<i64>,
    duration_ms: Option<i64>,
    output_summary: String,
    started_at: String,
    finished_at: Option<String>,
}

fn command(row: CommandRow) -> Result<ExecutionCommandRecord> {
    let args: Vec<String> = serde_json::from_str(&row.args_json)
        .map_err(|_| Error::Message(INVALID_COMMAND.to_string()))?;
    Ok(ExecutionCommandRecord {
        id: row.id,
        execution_id: row.execution_id,
        command: row.command,
        args,
        source: enum_value::<ExecutionCommandSource>(&row.source)
            .ok_or_else(|| Error::Message(INVALID_ROW.to_string()))?,
        status: enum_value::<ExecutionCommandStatus>(&row.status)
            .ok_or_else(|| Error::Message(INVALID_ROW.to_string()))?,
        exit_code: row.exit_code,
        duration_ms: row.duration_ms,
        output_summary: row.output_summary,
        started_at: row.started_at,
        finished_at: row.finished_at,
    })
}
