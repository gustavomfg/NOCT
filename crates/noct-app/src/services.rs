//! The work behind the commands of the first slice. Each function takes the store and returns the
//! contract value, so it is tested without a Tauri runtime. The commands in `commands.rs` only lock the
//! store, move the work to a blocking thread and map the arguments.

use std::path::PathBuf;

use noct_core::contracts::{Conversation, Workspace};
use noct_core::ipc::conversations::{
    ConversationCursor, ConversationPage, ConversationPageRequest,
};
use noct_core::ipc::settings::{AppSettings, SettingsUpdate, from_saved, to_saved};
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
