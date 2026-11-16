//! Free-form memory kept per workspace. Port of `WorkspaceMemoryRepository.ts`.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::Result;
use crate::clock::iso_now;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMemoryRow {
    pub content: String,
    pub updated_at: String,
}

pub struct WorkspaceMemory<'a> {
    conn: &'a Connection,
}

impl<'a> WorkspaceMemory<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// An empty memory with an empty timestamp when the workspace has none yet.
    pub fn get(&self, workspace: &str) -> Result<WorkspaceMemoryRow> {
        let row = self
            .conn
            .query_row(
                "SELECT content, updated_at FROM workspace_memory WHERE workspace = ?1",
                [workspace],
                |row| {
                    Ok(WorkspaceMemoryRow {
                        content: row.get(0)?,
                        updated_at: row.get(1)?,
                    })
                },
            )
            .optional()?;
        Ok(row.unwrap_or_else(|| WorkspaceMemoryRow {
            content: String::new(),
            updated_at: String::new(),
        }))
    }

    pub fn set(&self, workspace: &str, content: &str) -> Result<WorkspaceMemoryRow> {
        let updated_at = iso_now()?;
        self.conn.execute(
            "INSERT INTO workspace_memory(workspace, content, updated_at) VALUES(?1, ?2, ?3) \
             ON CONFLICT(workspace) DO UPDATE SET content = excluded.content, \
             updated_at = excluded.updated_at",
            params![workspace, content, updated_at],
        )?;
        Ok(WorkspaceMemoryRow {
            content: content.to_string(),
            updated_at,
        })
    }
}
