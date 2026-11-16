//! The Tauri commands of the first slice. Each one checks its arguments against the contract, locks the
//! store only while it reads or writes, and moves the blocking work to a worker thread. Which window may
//! call them is decided by `capabilities/`, not here.

use std::sync::{Mutex, MutexGuard};

use noct_core::contracts::Workspace;
use noct_core::ipc::conversations::{ConversationPage, ConversationPageRequest};
use noct_core::ipc::settings::{AppSettings, SettingsUpdate};
use noct_db::store::Store;
use tauri::State;

use crate::bootstrap::Persistence;
use crate::error::CommandError;
use crate::services;
use crate::workspace_scope::default_blocked_roots;

#[tauri::command]
pub async fn settings_get(state: State<'_, Persistence>) -> Result<AppSettings, CommandError> {
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::settings_get(&guard)
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
    blocking(move || {
        let guard = lock(&store)?;
        services::settings_set(&guard, &diagnostics, &update)
    })
    .await
}

#[tauri::command]
pub async fn workspaces_list(
    state: State<'_, Persistence>,
) -> Result<Vec<Workspace>, CommandError> {
    let store = state.store.clone();
    blocking(move || {
        let rows = {
            let guard = lock(&store)?;
            services::workspace_rows(&guard)?
        };
        Ok(services::workspaces_from_rows(
            rows,
            &default_blocked_roots(),
        ))
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
    blocking(move || {
        let guard = lock(&store)?;
        services::conversations_page(&guard, &request)
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
