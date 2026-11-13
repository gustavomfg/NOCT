//! Workspace history. Port of `WorkspaceRepository.ts`, except `relocate`: that moves
//! workspace model bindings and every workspace-scoped table, so it lands together with
//! the AI configuration repositories.

use rusqlite::{Connection, Row, params};
use serde::Serialize;

use crate::Result;
use crate::clock::iso_now;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRow {
    pub path: String,
    pub name: String,
    pub favorite: bool,
    pub authorized: bool,
    pub created_at: String,
    pub last_opened_at: String,
}

pub struct Workspaces<'a> {
    conn: &'a Connection,
}

impl<'a> Workspaces<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Favorites first, then most recently opened.
    pub fn list(&self) -> Result<Vec<WorkspaceRow>> {
        let mut statement = self.conn.prepare(
            "SELECT path, name, favorite, authorized, created_at, last_opened_at
             FROM workspaces ORDER BY favorite DESC, last_opened_at DESC",
        )?;
        let rows = statement
            .query_map([], workspace_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Records that `workspace` was opened now. The display name is the folder name.
    pub fn touch(&self, workspace: &str) -> Result<()> {
        let now = iso_now()?;
        self.conn.execute(
            "INSERT INTO workspaces(path,name,authorized,created_at,last_opened_at)
             VALUES(?,?,1,?,?)
             ON CONFLICT(path) DO UPDATE SET
               name=excluded.name,authorized=1,last_opened_at=excluded.last_opened_at",
            params![workspace, basename(workspace), now, now],
        )?;
        Ok(())
    }

    pub fn remove(&self, workspace: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM workspaces WHERE path=?", params![workspace])?;
        Ok(())
    }

    pub fn set_favorite(&self, workspace: &str, favorite: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE workspaces SET favorite=? WHERE path=?",
            params![i64::from(favorite), workspace],
        )?;
        Ok(())
    }
}

fn workspace_from_row(row: &Row<'_>) -> rusqlite::Result<WorkspaceRow> {
    Ok(WorkspaceRow {
        path: row.get("path")?,
        name: row.get("name")?,
        favorite: row.get::<_, i64>("favorite")? != 0,
        authorized: row.get::<_, i64>("authorized")? != 0,
        created_at: row.get("created_at")?,
        last_opened_at: row.get("last_opened_at")?,
    })
}

/// The last path segment, as Node's `path.basename` returns it for the current platform.
fn basename(workspace: &str) -> &str {
    let is_separator = |c: char| c == '/' || (cfg!(windows) && c == '\\');
    workspace
        .trim_end_matches(is_separator)
        .rsplit(is_separator)
        .next()
        .unwrap_or("")
}
