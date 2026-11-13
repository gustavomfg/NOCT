//! Workspace history. Port of `WorkspaceRepository.ts`, except `relocate`: that moves
//! workspace model bindings and every workspace-scoped table, so it lands together with
//! the AI configuration repositories.

use noct_core::ai::bindings::WorkspaceModelBindings;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

use super::workspace_model_bindings::ModelBindings;
use crate::clock::iso_now;
use crate::{Error, Result, transaction};

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

/// Tables that carry a workspace path, with the statement that moves their rows. Each
/// takes the destination as `?1` and the source as `?2`. Order matches the TypeScript.
const RELOCATED_TABLES: &[&str] = &[
    "UPDATE conversations SET workspace=?1 WHERE workspace=?2",
    "UPDATE artifacts SET workspace=?1 WHERE workspace=?2",
    "UPDATE workspace_memory SET workspace=?1 WHERE workspace=?2",
    "UPDATE suggestions SET workspace_id=?1 WHERE workspace_id=?2",
    "UPDATE brain_memories SET workspace_id=?1 WHERE workspace_id=?2",
    "UPDATE project_index_runs SET workspace=?1 WHERE workspace=?2",
    "INSERT INTO project_index_files(
        workspace,relative_path,classification,language,extension,size,mtime_ms,ctime_ms,mode,
        observed_hash,analyzed_hash,state,excluded,exclusion_reason,parser_id,parser_version,
        error,discovered_at,analyzed_at
     ) SELECT ?1,relative_path,classification,language,extension,size,mtime_ms,ctime_ms,mode,
        observed_hash,analyzed_hash,state,excluded,exclusion_reason,parser_id,parser_version,
        error,discovered_at,analyzed_at FROM project_index_files WHERE workspace=?2",
    "UPDATE project_index_symbols SET workspace=?1 WHERE workspace=?2",
    "UPDATE project_index_imports SET workspace=?1 WHERE workspace=?2",
    "UPDATE project_index_exports SET workspace=?1 WHERE workspace=?2",
    "UPDATE project_stack_evidence SET workspace=?1 WHERE workspace=?2",
    "UPDATE project_index_exclusions SET workspace=?1 WHERE workspace=?2",
    "UPDATE semantic_index_runs SET workspace=?1 WHERE workspace=?2",
    "UPDATE semantic_units SET workspace=?1 WHERE workspace=?2",
    "UPDATE validation_runs SET workspace=?1 WHERE workspace=?2",
    "UPDATE executions SET workspace=?1 WHERE workspace=?2",
    "UPDATE engineering_signals SET workspace=?1 WHERE workspace=?2",
    "UPDATE engineering_health_snapshots SET workspace=?1 WHERE workspace=?2",
    "UPDATE engineering_insights SET workspace=?1 WHERE workspace=?2",
    "UPDATE checkpoints SET workspace=?1 WHERE workspace=?2",
];

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

    /// Moves a saved workspace to another folder. Every workspace-scoped row and its model
    /// bindings follow it, in one transaction. Port of `WorkspaceRepository.relocate`.
    pub fn relocate(&self, source: &str, destination: &str) -> Result<()> {
        if source == destination {
            return self.touch(destination);
        }
        let bindings = ModelBindings::new(self.conn).get(source)?;
        transaction::run(self.conn, |conn| {
            let (favorite, created_at): (i64, String) = conn
                .query_row(
                    "SELECT favorite, created_at FROM workspaces WHERE path=?1",
                    [source],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .ok_or_else(|| {
                    Error::Message(
                        "Workspace original não encontrado no histórico local.".to_string(),
                    )
                })?;
            let destination_exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM workspaces WHERE path=?1)",
                [destination],
                |row| row.get(0),
            )?;
            if destination_exists {
                return Err(Error::Message(
                    "A pasta selecionada já pertence a outro workspace salvo.".to_string(),
                ));
            }

            let now = iso_now()?;
            conn.execute(
                "INSERT INTO workspaces(path,name,favorite,authorized,created_at,last_opened_at)
                 VALUES(?1,?2,?3,1,?4,?5)",
                params![
                    destination,
                    basename(destination),
                    favorite,
                    created_at,
                    now
                ],
            )?;
            for sql in RELOCATED_TABLES {
                conn.execute(sql, params![destination, source])?;
            }
            conn.execute(
                "DELETE FROM project_index_files WHERE workspace=?1",
                [source],
            )?;
            if let Some(bindings) = bindings {
                let moved = WorkspaceModelBindings {
                    workspace_id: destination.to_string(),
                    ..bindings
                };
                ModelBindings::new(conn).set(moved)?;
                ModelBindings::new(conn).delete(source)?;
            }
            conn.execute("DELETE FROM workspaces WHERE path=?1", [source])?;
            Ok(())
        })
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
