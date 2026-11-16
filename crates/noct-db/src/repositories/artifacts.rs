//! Artifacts produced in a conversation. Port of `ArtifactRepository.ts`.

use noct_core::json::serialize_within_limit;
use noct_core::limits::persistence::METADATA_CHARACTERS;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::Page;
use crate::clock::iso_now;
use crate::{Error, Result};

const METADATA_LIMIT: usize = METADATA_CHARACTERS as usize;
const ARTIFACT_COLUMNS: &str = "id, conversation_id, workspace, type, title, file_path, content, \
     metadata, created_at, updated_at";

/// A stored artifact. `metadata` is the JSON text written at save time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRow {
    pub id: String,
    pub conversation_id: String,
    pub workspace: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub file_path: Option<String>,
    pub content: Option<String>,
    pub metadata: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for [`Artifacts::add`]. Optional fields follow the TypeScript `?? null` rules.
#[derive(Debug, Clone, Copy)]
pub struct NewArtifact<'a> {
    pub conversation_id: &'a str,
    pub workspace: &'a str,
    pub kind: &'a str,
    pub title: &'a str,
    pub file_path: Option<&'a str>,
    pub content: Option<&'a str>,
    pub metadata: Option<&'a Value>,
}

pub struct Artifacts<'a> {
    conn: &'a Connection,
}

impl<'a> Artifacts<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Most recently updated first.
    pub fn list(&self, conversation_id: &str) -> Result<Vec<ArtifactRow>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {ARTIFACT_COLUMNS} FROM artifacts WHERE conversation_id = ?1 \
             ORDER BY updated_at DESC"
        ))?;
        let rows = statement.query_map([conversation_id], artifact_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// One page of artifacts. `has_more` is true when a row exists past the page.
    pub fn page(
        &self,
        conversation_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<ArtifactRow>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {ARTIFACT_COLUMNS} FROM artifacts WHERE conversation_id = ?1 \
             ORDER BY updated_at DESC LIMIT ?2 OFFSET ?3"
        ))?;
        let mut items: Vec<ArtifactRow> = statement
            .query_map(
                params![conversation_id, limit as i64 + 1, offset as i64],
                artifact_from_row,
            )?
            .collect::<rusqlite::Result<_>>()?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        Ok(Page { items, has_more })
    }

    /// Saves an artifact. A non-empty `file_path` that already belongs to the same
    /// conversation updates that artifact in place and keeps its identifier and creation time.
    pub fn add(&self, input: &NewArtifact<'_>) -> Result<ArtifactRow> {
        let now = iso_now()?;
        let existing = match input.file_path.filter(|path| !path.is_empty()) {
            Some(path) => self
                .conn
                .query_row(
                    "SELECT id, created_at FROM artifacts WHERE conversation_id = ?1 \
                     AND file_path = ?2 ORDER BY updated_at DESC LIMIT 1",
                    params![input.conversation_id, path],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?,
            None => None,
        };
        let metadata = match input.metadata.filter(|value| !value.is_null()) {
            Some(value) => Some(
                serialize_within_limit(value, METADATA_LIMIT)
                    .map_err(|error| Error::Message(error.to_string()))?,
            ),
            None => None,
        };
        let (id, created_at) =
            existing.unwrap_or_else(|| (Uuid::new_v4().to_string(), now.clone()));
        self.conn.execute(
            "INSERT INTO artifacts(id, conversation_id, workspace, type, title, file_path, content, \
             metadata, created_at, updated_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
             ON CONFLICT(id) DO UPDATE SET type = excluded.type, title = excluded.title, \
             content = excluded.content, metadata = excluded.metadata, \
             updated_at = excluded.updated_at",
            params![
                id,
                input.conversation_id,
                input.workspace,
                input.kind,
                input.title,
                input.file_path,
                input.content,
                metadata,
                created_at,
                now,
            ],
        )?;
        Ok(ArtifactRow {
            id,
            conversation_id: input.conversation_id.to_string(),
            workspace: input.workspace.to_string(),
            kind: input.kind.to_string(),
            title: input.title.to_string(),
            file_path: input.file_path.map(str::to_string),
            content: input.content.map(str::to_string),
            metadata,
            created_at,
            updated_at: now,
        })
    }

    /// Deletes only when the artifact belongs to `conversation_id`.
    pub fn delete(&self, id: &str, conversation_id: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM artifacts WHERE id = ?1 AND conversation_id = ?2",
            params![id, conversation_id],
        )?;
        Ok(changed > 0)
    }
}

fn artifact_from_row(row: &Row<'_>) -> rusqlite::Result<ArtifactRow> {
    Ok(ArtifactRow {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        workspace: row.get(2)?,
        kind: row.get(3)?,
        title: row.get(4)?,
        file_path: row.get(5)?,
        content: row.get(6)?,
        metadata: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}
