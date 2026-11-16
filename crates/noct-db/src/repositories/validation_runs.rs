//! Validation runs. Port of `ValidationRepository.ts`. Outcomes are stored as structured,
//! bounded records; raw process logs are never kept.

use noct_core::limits::code_intelligence::MAX_QUERY_RESULTS;
use noct_core::limits::collection_page::VALIDATION as VALIDATION_PAGE;
use noct_core::validation_run::{
    ValidationArtifact, ValidationKind, ValidationRun, ValidationRunStatus,
};
use rusqlite::{Connection, OptionalExtension, params};

use crate::codec::{enum_text, enum_value};
use crate::repositories::Page;
use crate::repositories::workspace_evidence::{EvidenceInput, WorkspaceEvidence};
use crate::{Error, Result, transaction};
use noct_core::workspace_evidence::EvidenceKind;

const INVALID_ARGS: &str = "A execução de validação persistida possui argumentos inválidos.";
const INVALID_ARTIFACTS: &str = "A execução de validação persistida possui artefatos inválidos.";
const INVALID_ARTIFACT: &str = "A execução de validação persistida possui um artefato inválido.";
const INVALID_RUN: &str = "A execução de validação persistida está inválida.";

const SELECT: &str = "SELECT id, workspace, execution_id, kind, command, args_json, status, exit_code, \
    duration_ms, output_summary, artifacts_json, started_at, completed_at, error FROM validation_runs";

pub struct ValidationRuns<'a> {
    conn: &'a Connection,
}

impl<'a> ValidationRuns<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Records the run's evidence and stores the run, as one unit of work.
    pub fn create(&self, run: &ValidationRun) -> Result<()> {
        transaction::run(self.conn, |conn| {
            let evidence = WorkspaceEvidence::new(conn);
            let mut references = evidence.index_references(&run.workspace)?;
            references.extend(evidence.checkpoint_references(run.execution_id.as_deref())?);
            evidence.record(EvidenceInput {
                kind: EvidenceKind::Validation,
                workspace: run.workspace.clone(),
                source_id: run.id.clone(),
                execution_id: run.execution_id.clone(),
                started_at: Some(run.started_at.clone()),
                paths: Vec::new(),
                references,
            })?;
            let values = Values::from(run)?;
            let bound = values.bind();
            conn.execute(
                "INSERT INTO validation_runs(id,workspace,execution_id,kind,command,args_json,status,exit_code,\
                 duration_ms,output_summary,artifacts_json,started_at,completed_at,error) \
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                bound.as_slice(),
            )?;
            Ok(())
        })
    }

    pub fn update(&self, run: &ValidationRun) -> Result<()> {
        let values = Values::from(run)?;
        let bound = values.bind();
        self.conn.execute(
            "UPDATE validation_runs SET execution_id=?3,command=?5,args_json=?6,status=?7,exit_code=?8,\
             duration_ms=?9,output_summary=?10,artifacts_json=?11,completed_at=?13,error=?14 \
             WHERE id=?1 AND workspace=?2",
            bound.as_slice(),
        )?;
        Ok(())
    }

    pub fn save(&self, run: &ValidationRun) -> Result<()> {
        transaction::run(self.conn, |_| {
            let exists: Option<i64> = self
                .conn
                .query_row(
                    "SELECT 1 FROM validation_runs WHERE id=?1 AND workspace=?2",
                    params![run.id, run.workspace],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_some() {
                self.update(run)
            } else {
                self.create(run)
            }
        })
    }

    pub fn latest(&self, workspace: &str) -> Result<Option<ValidationRun>> {
        let row = self
            .conn
            .query_row(
                &format!("{SELECT} WHERE workspace=?1 ORDER BY started_at DESC, id DESC LIMIT 1"),
                [workspace],
                read_row,
            )
            .optional()?;
        row.map(run).transpose()
    }

    /// The most recent runs of a workspace, at most `limit` and never more than 100.
    pub fn list(&self, workspace: &str, limit: Option<i64>) -> Result<Vec<ValidationRun>> {
        let limit = limit
            .unwrap_or(MAX_QUERY_RESULTS as i64)
            .clamp(1, MAX_QUERY_RESULTS as i64);
        let mut statement = self.conn.prepare(&format!(
            "{SELECT} WHERE workspace=?1 ORDER BY started_at DESC, id DESC LIMIT ?2"
        ))?;
        let rows = statement
            .query_map(params![workspace, limit], read_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(run).collect()
    }

    /// One page of runs. `has_more` is set when a run exists beyond this page.
    pub fn page(
        &self,
        workspace: &str,
        offset: i64,
        limit: Option<i64>,
    ) -> Result<Page<ValidationRun>> {
        let offset = offset.max(0);
        let limit = limit
            .unwrap_or(VALIDATION_PAGE as i64)
            .clamp(1, MAX_QUERY_RESULTS as i64);
        let mut statement = self.conn.prepare(&format!(
            "{SELECT} WHERE workspace=?1 ORDER BY started_at DESC, id DESC LIMIT ?2 OFFSET ?3"
        ))?;
        let mut rows = statement
            .query_map(params![workspace, limit + 1, offset], read_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let has_more = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        Ok(Page {
            items: rows.into_iter().map(run).collect::<Result<Vec<_>>>()?,
            has_more,
        })
    }
}

struct Values {
    id: String,
    workspace: String,
    execution_id: Option<String>,
    kind: String,
    command: String,
    args_json: String,
    status: String,
    exit_code: Option<i64>,
    duration_ms: Option<i64>,
    output_summary: String,
    artifacts_json: String,
    started_at: String,
    completed_at: Option<String>,
    error: Option<String>,
}

impl Values {
    fn from(run: &ValidationRun) -> Result<Self> {
        Ok(Self {
            id: run.id.clone(),
            workspace: run.workspace.clone(),
            execution_id: run.execution_id.clone(),
            kind: enum_text(&run.kind)?,
            command: run.command.clone(),
            args_json: serde_json::to_string(&run.args).map_err(|_| invalid())?,
            status: enum_text(&run.status)?,
            exit_code: run.exit_code,
            duration_ms: run.duration_ms,
            output_summary: run.output_summary.clone(),
            artifacts_json: serde_json::to_string(&run.artifacts).map_err(|_| invalid())?,
            started_at: run.started_at.clone(),
            completed_at: run.completed_at.clone(),
            error: run.error.clone(),
        })
    }

    fn bind(&self) -> Vec<&dyn rusqlite::types::ToSql> {
        vec![
            &self.id,
            &self.workspace,
            &self.execution_id,
            &self.kind,
            &self.command,
            &self.args_json,
            &self.status,
            &self.exit_code,
            &self.duration_ms,
            &self.output_summary,
            &self.artifacts_json,
            &self.started_at,
            &self.completed_at,
            &self.error,
        ]
    }
}

struct Row {
    id: String,
    workspace: String,
    execution_id: Option<String>,
    kind: String,
    command: String,
    args_json: String,
    status: String,
    exit_code: Option<i64>,
    duration_ms: Option<i64>,
    output_summary: String,
    artifacts_json: String,
    started_at: String,
    completed_at: Option<String>,
    error: Option<String>,
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    Ok(Row {
        id: row.get(0)?,
        workspace: row.get(1)?,
        execution_id: row.get(2)?,
        kind: row.get(3)?,
        command: row.get(4)?,
        args_json: row.get(5)?,
        status: row.get(6)?,
        exit_code: row.get(7)?,
        duration_ms: row.get(8)?,
        output_summary: row.get(9)?,
        artifacts_json: row.get(10)?,
        started_at: row.get(11)?,
        completed_at: row.get(12)?,
        error: row.get(13)?,
    })
}

fn run(row: Row) -> Result<ValidationRun> {
    let args: Vec<String> = serde_json::from_str(&row.args_json)
        .map_err(|_| Error::Message(INVALID_ARGS.to_string()))?;
    let artifacts = artifacts(&row.artifacts_json)?;
    Ok(ValidationRun {
        id: row.id,
        workspace: row.workspace,
        execution_id: row.execution_id.filter(|id| !id.is_empty()),
        kind: enum_value::<ValidationKind>(&row.kind).ok_or_else(invalid)?,
        command: row.command,
        args,
        status: enum_value::<ValidationRunStatus>(&row.status).ok_or_else(invalid)?,
        exit_code: row.exit_code,
        duration_ms: row.duration_ms,
        output_summary: row.output_summary,
        artifacts,
        started_at: row.started_at,
        completed_at: row.completed_at,
        error: row.error,
    })
}

/// Decodes the stored artifacts one by one, so a bad entry is reported as such.
fn artifacts(value: &str) -> Result<Vec<ValidationArtifact>> {
    let serde_json::Value::Array(items) =
        serde_json::from_str(value).map_err(|_| Error::Message(INVALID_ARTIFACTS.to_string()))?
    else {
        return Err(Error::Message(INVALID_ARTIFACTS.to_string()));
    };
    items
        .into_iter()
        .map(|item| {
            serde_json::from_value(item).map_err(|_| Error::Message(INVALID_ARTIFACT.to_string()))
        })
        .collect()
}

fn invalid() -> Error {
    Error::Message(INVALID_RUN.to_string())
}
