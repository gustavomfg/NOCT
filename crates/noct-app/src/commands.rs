//! The Tauri commands of the first slice. Each one checks its arguments against the contract, locks the
//! store only while it reads or writes, and moves the blocking work to a worker thread. Which window may
//! call them is decided by `capabilities/`, not here.

use std::sync::{Mutex, MutexGuard};

use noct_core::brain_memory::BrainMemory;
use noct_core::contracts::Conversation;
use noct_core::ipc::brain::{BrainMemoryCreateRequest, BrainMemoryUpdateRequest};
use noct_core::ipc::conversations::{ConversationPage, ConversationPageRequest};
use noct_core::ipc::settings::{AppSettings, SettingsUpdate};
use noct_core::ipc::workspaces::{WorkspaceFavoriteRequest, WorkspacePage, WorkspacePageRequest};
use noct_db::store::Store;
use tauri::State;

use crate::bootstrap::Persistence;
use crate::error::CommandError;
use crate::logging::measured;
use crate::services;
use crate::workspace_scope::default_blocked_roots;

// The commands below are measured under the names the TypeScript runtime gives the same reads and writes
// (`noct_db::observer::CATALOG`). A command with no such name is not measured; see docs/status.md.

#[tauri::command]
pub async fn settings_get(state: State<'_, Persistence>) -> Result<AppSettings, CommandError> {
    let store = state.store.clone();
    let observer = state.observer.clone();
    blocking(move || {
        measured(observer.as_ref(), "settings.get", || {
            let guard = lock(&store)?;
            services::settings_get(&guard)
        })
    })
    .await
}

#[tauri::command]
pub async fn settings_set(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<AppSettings, CommandError> {
    let update: SettingsUpdate =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    let diagnostics = state.diagnostics.clone();
    let observer = state.observer.clone();
    blocking(move || {
        measured(observer.as_ref(), "settings.set", || {
            let guard = lock(&store)?;
            services::settings_set(&guard, &diagnostics, &update)
        })
    })
    .await
}

/// One page of projects. The argument is a page request: a required limit from 1 to 500, and an optional offset.
/// The unpaginated list is not a command. The store lock is held only while the rows are read; the availability
/// of the page is checked after it is released, as the file-system check must not hold the store.
#[tauri::command]
pub async fn workspaces_list(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<WorkspacePage, CommandError> {
    let request: WorkspacePageRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    let observer = state.observer.clone();
    blocking(move || {
        measured(observer.as_ref(), "workspaces.list", || {
            let (rows, has_more) = {
                let guard = lock(&store)?;
                services::workspace_page_rows(&guard, &request)?
            };
            Ok(services::workspace_page_from_rows(
                rows,
                has_more,
                &default_blocked_roots(),
            ))
        })
    })
    .await
}

/// Creates a conversation in a saved project. The argument is the project path as a string, which is the contract
/// the TypeScript shell uses. The store lock covers the scope check, the saved check and the write.
#[tauri::command]
pub async fn conversations_create(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<Conversation, CommandError> {
    let workspace = match input {
        serde_json::Value::String(text) => text,
        _ => return Err(CommandError::InvalidInput),
    };
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::conversation_create(&guard, &default_blocked_roots(), &workspace)
    })
    .await
}

/// Creates a candidate brain memory. The argument is `{ conversationId, kind, scope, content }`, checked by the request
/// contract before the store is read. The store lock covers the conversation lookup, the project check and the write.
#[tauri::command]
pub async fn brain_create(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<BrainMemory, CommandError> {
    let request: BrainMemoryCreateRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::brain_create(&guard, &default_blocked_roots(), &request)
    })
    .await
}

/// Changes a brain memory. The argument is `{ conversationId, memoryId, ... }`, where the optional fields that are present
/// are changed. The request contract checks it before the store is read. The store lock covers the lookups, the project
/// check and the write.
#[tauri::command]
pub async fn brain_update(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<BrainMemory, CommandError> {
    let request: BrainMemoryUpdateRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::brain_update(&guard, &default_blocked_roots(), &request)
    })
    .await
}

/// Marks a saved project as a favorite, or unmarks it. The argument is `{ workspace, favorite }`. The store lock is
/// held across the check that the project is saved and the write, so the two cannot be separated by another write.
#[tauri::command]
pub async fn workspace_set_favorite(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<(), CommandError> {
    let request: WorkspaceFavoriteRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::workspace_set_favorite(&guard, &request)
    })
    .await
}

/// One page of conversations. The argument is a page request: a required limit from 1 to 500, and the
/// cursor of the previous page. The unpaginated list is not a command.
#[tauri::command]
pub async fn conversations_list(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<ConversationPage, CommandError> {
    let request: ConversationPageRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    let observer = state.observer.clone();
    blocking(move || {
        measured(observer.as_ref(), "conversations.page", || {
            let guard = lock(&store)?;
            services::conversations_page(&guard, &request)
        })
    })
    .await
}

fn lock(store: &Mutex<Store>) -> Result<MutexGuard<'_, Store>, CommandError> {
    store.lock().map_err(|_| CommandError::Internal)
}

async fn blocking<T, F>(work: F) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CommandError> + Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(result) => result,
        Err(_) => Err(CommandError::Internal),
    }
}
