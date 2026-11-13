//! Checkpoint manifests. Port of `CheckpointRepository.ts`. File bytes live in the private
//! application data store; this repository keeps only their manifest.

use noct_core::change_control::{
    CheckpointFileKind, CheckpointFileRecord, CheckpointPhase, CheckpointRecord, CheckpointStatus,
};
use noct_core::workspace_evidence::{EvidenceKind, KnownWorkspacePath, WorkspaceStateManifest};
use rusqlite::{Connection, OptionalExtension, params};

use crate::codec::{enum_text, enum_value};
use crate::repositories::workspace_evidence::{EvidenceInput, WorkspaceEvidence};
use crate::{Error, Result, transaction};

const INVALID_CHECKPOINT: &str = "O checkpoint persistido está inválido.";

const SELECT: &str = "SELECT id, execution_id, workspace, phase, status, captured_at, root_path, error \
    FROM checkpoints";

pub struct Checkpoints<'a> {
    conn: &'a Connection,
}

impl<'a> Checkpoints<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create(&self, record: &CheckpointRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO checkpoints(id,execution_id,workspace,phase,status,captured_at,root_path,error) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                record.id,
                record.execution_id,
                record.workspace,
                enum_text(&record.phase)?,
                enum_text(&record.status)?,
                record.captured_at,
                record.root_path,
                record.error,
            ],
        )?;
        Ok(())
    }

    pub fn update(&self, record: &CheckpointRecord) -> Result<()> {
        self.conn.execute(
            "UPDATE checkpoints SET status=?1,captured_at=?2,root_path=?3,error=?4 \
             WHERE id=?5 AND execution_id=?6",
            params![
                enum_text(&record.status)?,
                record.captured_at,
                record.root_path,
                record.error,
                record.id,
                record.execution_id,
            ],
        )?;
        Ok(())
    }

    /// Creates the checkpoint, or updates it when the identity is already stored.
    pub fn save(&self, record: &CheckpointRecord) -> Result<()> {
        transaction::run(self.conn, |_| {
            let exists: Option<i64> = self
                .conn
                .query_row(
                    "SELECT 1 FROM checkpoints WHERE id=?1 AND execution_id=?2",
                    params![record.id, record.execution_id],
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

    /// Replaces the file manifest of a checkpoint in one unit of work.
    pub fn replace_files(&self, checkpoint_id: &str, files: &[CheckpointFileRecord]) -> Result<()> {
        transaction::run(self.conn, |conn| {
            conn.execute(
                "DELETE FROM checkpoint_files WHERE checkpoint_id=?1",
                [checkpoint_id],
            )?;
            for file in files {
                conn.execute(
                    "INSERT INTO checkpoint_files(id,checkpoint_id,relative_path,exists_flag,kind,size,mode,hash,content_path) \
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    params![
                        file.id,
                        file.checkpoint_id,
                        file.relative_path,
                        i64::from(file.exists),
                        enum_text(&file.kind)?,
                        file.size,
                        file.mode,
                        file.hash,
                        file.content_path,
                    ],
                )?;
            }
            Ok(())
        })
    }

    /// Records the evidence of a checkpoint: the files it observed, under the phase name.
    pub fn record_evidence(
        &self,
        checkpoint: &CheckpointRecord,
        files: &[CheckpointFileRecord],
    ) -> Result<WorkspaceStateManifest> {
        let kind = match checkpoint.phase {
            CheckpointPhase::Before => EvidenceKind::Before,
            CheckpointPhase::After => EvidenceKind::After,
        };
        let paths = files
            .iter()
            .map(|file| KnownWorkspacePath {
                path: file.relative_path.clone(),
                hash: file.hash.clone(),
                exists: Some(file.exists),
            })
            .collect();
        WorkspaceEvidence::new(self.conn).record(EvidenceInput {
            kind,
            workspace: checkpoint.workspace.clone(),
            source_id: checkpoint.id.clone(),
            execution_id: Some(checkpoint.execution_id.clone()),
            started_at: Some(checkpoint.captured_at.clone()),
            paths,
            references: Vec::new(),
        })
    }

    /// Stores the completed manifest, its files and their evidence as one unit of work.
    pub fn complete_capture(
        &self,
        checkpoint: &CheckpointRecord,
        files: &[CheckpointFileRecord],
    ) -> Result<WorkspaceStateManifest> {
        transaction::run(self.conn, |_| {
            self.replace_files(&checkpoint.id, files)?;
            self.update(checkpoint)?;
            self.record_evidence(checkpoint, files)
        })
    }

    /// The checkpoint with this identifier, optionally only within one execution.
    pub fn get(&self, id: &str, execution_id: Option<&str>) -> Result<Option<CheckpointRecord>> {
        let row = self
            .conn
            .query_row(
                &format!("{SELECT} WHERE id=?1 AND (?2 IS NULL OR execution_id=?2)"),
                params![id, execution_id],
                read_row,
            )
            .optional()?;
        row.map(checkpoint).transpose()
    }

    pub fn list(&self, execution_id: &str) -> Result<Vec<CheckpointRecord>> {
        let mut statement = self.conn.prepare(&format!(
            "{SELECT} WHERE execution_id=?1 ORDER BY captured_at"
        ))?;
        let rows = statement
            .query_map([execution_id], read_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(checkpoint).collect()
    }

    pub fn list_files(&self, checkpoint_id: &str) -> Result<Vec<CheckpointFileRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT id, checkpoint_id, relative_path, exists_flag, kind, size, mode, hash, content_path \
             FROM checkpoint_files WHERE checkpoint_id=?1 ORDER BY relative_path",
        )?;
        let rows = statement
            .query_map([checkpoint_id], |row| {
                Ok(FileRow {
                    id: row.get(0)?,
                    checkpoint_id: row.get(1)?,
                    relative_path: row.get(2)?,
                    exists: row.get(3)?,
                    kind: row.get(4)?,
                    size: row.get(5)?,
                    mode: row.get(6)?,
                    hash: row.get(7)?,
                    content_path: row.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(file).collect()
    }
}

struct Row {
    id: String,
    execution_id: String,
    workspace: String,
    phase: String,
    status: String,
    captured_at: String,
    root_path: String,
    error: Option<String>,
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    Ok(Row {
        id: row.get(0)?,
        execution_id: row.get(1)?,
        workspace: row.get(2)?,
        phase: row.get(3)?,
        status: row.get(4)?,
        captured_at: row.get(5)?,
        root_path: row.get(6)?,
        error: row.get(7)?,
    })
}

fn checkpoint(row: Row) -> Result<CheckpointRecord> {
    let invalid = || Error::Message(INVALID_CHECKPOINT.to_string());
    Ok(CheckpointRecord {
        id: row.id,
        execution_id: row.execution_id,
        workspace: row.workspace,
        phase: enum_value::<CheckpointPhase>(&row.phase).ok_or_else(invalid)?,
        status: enum_value::<CheckpointStatus>(&row.status).ok_or_else(invalid)?,
        captured_at: row.captured_at,
        root_path: row.root_path,
        error: row.error,
    })
}

struct FileRow {
    id: String,
    checkpoint_id: String,
    relative_path: String,
    exists: i64,
    kind: String,
    size: Option<i64>,
    mode: Option<i64>,
    hash: Option<String>,
    content_path: Option<String>,
}

fn file(row: FileRow) -> Result<CheckpointFileRecord> {
    let invalid = || Error::Message(INVALID_CHECKPOINT.to_string());
    Ok(CheckpointFileRecord {
        id: row.id,
        checkpoint_id: row.checkpoint_id,
        relative_path: row.relative_path,
        exists: match row.exists {
            0 => false,
            1 => true,
            _ => return Err(invalid()),
        },
        kind: enum_value::<CheckpointFileKind>(&row.kind).ok_or_else(invalid)?,
        size: row.size,
        mode: row.mode,
        hash: row.hash,
        content_path: row.content_path,
    })
}
