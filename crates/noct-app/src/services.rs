//! The work behind the commands of the first slice. Each function takes the store and returns the
//! contract value, so it is tested without a Tauri runtime. The commands in `commands.rs` only lock the
//! store, move the work to a blocking thread and map the arguments.

use std::path::PathBuf;

use noct_core::contracts::{Conversation, Workspace};
use noct_core::ipc::conversations::{
    ConversationCursor, ConversationPage, ConversationPageRequest,
};
use noct_core::ipc::settings::{AppSettings, SettingsUpdate, from_saved, to_saved};
use noct_core::ipc::workspaces::{
    WorkspaceCursor, WorkspaceFavoriteRequest, WorkspacePage, WorkspacePageRequest,
};
use noct_db::repositories::workspaces::WorkspaceRow;
use noct_db::store::Store;

use crate::error::CommandError;
use crate::logging::DiagnosticGate;
use crate::workspace_scope;

pub fn settings_get(store: &Store) -> Result<AppSettings, CommandError> {
    let saved = store
        .settings()
        .get()
        .map_err(|_| CommandError::Persistence)?;
    Ok(from_saved(&saved))
}

/// Writes the fields of the update and returns the settings as they are stored afterwards. The diagnostic
/// gate follows the new value once the write has succeeded, so a failed write leaves the gate unchanged.
pub fn settings_set(
    store: &Store,
    diagnostics: &DiagnosticGate,
    update: &SettingsUpdate,
) -> Result<AppSettings, CommandError> {
    store
        .settings()
        .set(&to_saved(update))
        .map_err(|_| CommandError::Persistence)?;
    if let Some(enabled) = update.diagnostic_mode {
        diagnostics.set_enabled(enabled);
    }
    settings_get(store)
}

/// The saved workspaces, in the order the store keeps them: favorites first, then the latest opened.
pub fn workspace_rows(store: &Store) -> Result<Vec<WorkspaceRow>, CommandError> {
    store
        .workspaces()
        .list()
        .map_err(|_| CommandError::Persistence)
}

/// One page of the saved workspaces, as the rows the page holds and whether another row follows them. The store
/// reads `limit` rows after the request's cursor, plus one to learn whether a row follows; so the read is bounded
/// by the limit, not by the number of projects. The availability of each row is checked later, by the caller, and
/// only for the rows of this page.
pub fn workspace_page_rows(
    store: &Store,
    request: &WorkspacePageRequest,
) -> Result<(Vec<WorkspaceRow>, bool), CommandError> {
    let after = request.cursor.as_ref().map(|cursor| {
        (
            cursor.favorite,
            cursor.last_opened_at.as_str(),
            cursor.path.as_str(),
        )
    });
    let mut rows = store
        .workspaces()
        .page_after(after, request.limit.saturating_add(1))
        .map_err(|_| CommandError::Persistence)?;
    let has_more = rows.len() > request.limit;
    rows.truncate(request.limit);
    Ok((rows, has_more))
}

/// The key of a project row, as the cursor of the page that ends with it.
fn cursor_of(row: &WorkspaceRow) -> WorkspaceCursor {
    WorkspaceCursor {
        favorite: row.favorite,
        last_opened_at: row.last_opened_at.clone(),
        path: row.path.clone(),
    }
}

/// The page of rows the store returned, with their availability. The cursor is the key of the last row, and it is
/// present only when a row follows the page.
pub fn workspace_page_from_rows(
    rows: Vec<WorkspaceRow>,
    has_more: bool,
    blocked: &[PathBuf],
) -> WorkspacePage {
    let next_cursor = if has_more {
        rows.last().map(cursor_of)
    } else {
        None
    };
    WorkspacePage {
        items: workspaces_from_rows(rows, blocked),
        has_more,
        next_cursor,
    }
}

/// Marks a saved project as a favorite, or unmarks it. The project must be saved: a request for a path that is not
/// saved is refused with `UnknownWorkspace`, and nothing is written. The caller holds the store lock across the
/// check and the write, so no other write can save or remove the project between them.
pub fn workspace_set_favorite(
    store: &Store,
    request: &WorkspaceFavoriteRequest,
) -> Result<(), CommandError> {
    let saved = workspace_rows(store)?
        .iter()
        .any(|row| row.path == request.workspace);
    if !saved {
        return Err(CommandError::UnknownWorkspace);
    }
    store
        .workspaces()
        .set_favorite(&request.workspace, request.favorite)
        .map_err(|_| CommandError::Persistence)
}

/// One page of projects with their availability. It reads the rows under the store lock, which the caller
/// holds, and checks the page outside of it. Tests use this whole path; the command splits it at the lock.
pub fn workspace_page(
    store: &Store,
    blocked: &[PathBuf],
    request: &WorkspacePageRequest,
) -> Result<WorkspacePage, CommandError> {
    let (rows, has_more) = workspace_page_rows(store, request)?;
    Ok(workspace_page_from_rows(rows, has_more, blocked))
}

/// Each row with its availability. The check reads the file system, so the caller runs it outside the
/// store lock. A workspace is authorized only while its folder is available, as `workspace.list` does.
pub fn workspaces_from_rows(rows: Vec<WorkspaceRow>, blocked: &[PathBuf]) -> Vec<Workspace> {
    rows.into_iter()
        .map(|row| {
            let inspection = workspace_scope::inspect(&row.path, blocked);
            let available =
                inspection.availability == noct_core::contracts::WorkspaceAvailability::Available;
            Workspace {
                authorized: row.authorized && available,
                availability: inspection.availability,
                availability_message: inspection.message.map(str::to_string),
                path: row.path,
                name: row.name,
                favorite: row.favorite,
                created_at: row.created_at,
                last_opened_at: row.last_opened_at,
            }
        })
        .collect()
}

/// One page of conversations after the cursor of the request. The store is asked for one row more than the
/// limit: that extra row says whether another page exists, and it is not returned.
pub fn conversations_page(
    store: &Store,
    request: &ConversationPageRequest,
) -> Result<ConversationPage, CommandError> {
    let after = request
        .cursor
        .as_ref()
        .map(|cursor| (cursor.updated_at.as_str(), cursor.id.as_str()));
    let mut rows = store
        .conversations()
        .page_after(after, request.limit + 1)
        .map_err(|_| CommandError::Persistence)?;
    let has_more = rows.len() > request.limit;
    rows.truncate(request.limit);
    let next_cursor = if has_more {
        rows.last().map(|row| ConversationCursor {
            updated_at: row.updated_at.clone(),
            id: row.id.clone(),
        })
    } else {
        None
    };
    let items = rows
        .into_iter()
        .map(|row| Conversation {
            id: row.id,
            title: row.title,
            workspace: row.workspace,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
        .collect();
    Ok(ConversationPage {
        items,
        has_more,
        next_cursor,
    })
}
