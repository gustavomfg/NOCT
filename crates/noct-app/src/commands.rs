//! The Tauri commands of the first slice. Each one checks its arguments against the contract, locks the
//! store only while it reads or writes, and moves the blocking work to a worker thread. Which window may
//! call them is decided by `capabilities/`, not here.

use std::sync::{Mutex, MutexGuard};

use noct_core::ai::bindings::WorkspaceModelBindings;
use noct_core::brain_memory::BrainMemory;
use noct_core::contracts::{Conversation, Message};
use noct_core::ipc::ReplyRequest;
use noct_core::ipc::ai::SaveAssistantRequest;
use noct_core::ipc::brain::{
    BrainMemoryCreateRequest, BrainMemoryExtraction, BrainMemoryUpdateRequest,
};
use noct_core::ipc::conversations::{ConversationPage, ConversationPageRequest};
use noct_core::ipc::settings::{AppSettings, SettingsUpdate};
use noct_core::ipc::suggestions::{SuggestionCreation, SuggestionStatusRequest};
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

/// Deletes a conversation with its messages and artifacts. The argument is the conversation id, as a string.
#[tauri::command]
pub async fn conversations_delete(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<(), CommandError> {
    let id = match input {
        serde_json::Value::String(text) => text,
        _ => return Err(CommandError::InvalidInput),
    };
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::conversation_delete(&guard, &default_blocked_roots(), &id)
    })
    .await
}

/// Deletes a memory of the conversation's project. The argument is `{ conversationId, memoryId }`.
#[tauri::command]
pub async fn brain_delete(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<(), CommandError> {
    let request: noct_core::ipc::brain::BrainMemoryDeleteRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::brain_delete(&guard, &default_blocked_roots(), &request)
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

/// Reads the memory blocks of an assistant reply and stores its candidates. The argument is `{ conversationId, content }`.
/// A failure to store the candidates is a reply with a warning, not an error.
#[tauri::command]
pub async fn brain_extract(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<BrainMemoryExtraction, CommandError> {
    let request: ReplyRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::brain_extract(&guard, &default_blocked_roots(), &request)
    })
    .await
}

/// Reads the review snapshot of an assistant reply and reconciles its suggestions. The argument is
/// `{ conversationId, content }`. A reply without a usable snapshot is a reply with a warning, not an error.
#[tauri::command]
pub async fn suggestions_create(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<SuggestionCreation, CommandError> {
    let request: ReplyRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::suggestions_create(&guard, &default_blocked_roots(), &request)
    })
    .await
}

/// Moves a suggestion to a new status, with an optional result. The argument is
/// `{ conversationId, suggestionId, status, result? }`. The decision is recorded in the store; the `.nocturne` history
/// of the TypeScript shell is not written yet.
#[tauri::command]
pub async fn suggestions_status(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<noct_core::suggestions::Suggestion, CommandError> {
    let request: SuggestionStatusRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::suggestions_status(&guard, &default_blocked_roots(), &request)
    })
    .await
}

/// Stores the model bindings of a project. The argument is the bindings object itself, as `models.setBindings` takes it.
#[tauri::command]
pub async fn models_set_bindings(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<WorkspaceModelBindings, CommandError> {
    let bindings: WorkspaceModelBindings =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::models_set_bindings(&guard, &default_blocked_roots(), bindings)
    })
    .await
}

/// Stores an assistant reply with its metadata. The argument is `{ conversationId, content, metadata? }`.
#[tauri::command]
pub async fn ai_save_assistant(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<Message, CommandError> {
    let request: SaveAssistantRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::ai_save_assistant(&guard, &default_blocked_roots(), &request)
    })
    .await
}

/// Removes a project's record with its conversations and their history. The argument is the project path, as a string.
/// Nothing is removed from the folder.
#[tauri::command]
pub async fn workspace_remove(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<(), CommandError> {
    let workspace = match input {
        serde_json::Value::String(text) => text,
        _ => return Err(CommandError::InvalidInput),
    };
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::workspace_remove(&guard, &workspace)
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
