//! The Tauri commands of the first slice. Each one checks its arguments against the contract, locks the
//! store only while it reads or writes, and moves the blocking work to a worker thread. Which window may
//! call them is decided by `capabilities/`, not here.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use noct_core::ai::bindings::{
    DefaultModelRequest, ProjectBindingsRequest, WorkspaceModelBindings,
};
use noct_core::ai::consent::{ProjectConsentRequest, ProjectConsentStatus};
use noct_core::ai::provider_key::{ProviderKeyInput, ProviderKeyProvider, ProviderKeyStatus};
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
use noct_secrets::ProviderCredentialVault;
use tauri::State;
use tauri::ipc::Channel;

use crate::bootstrap::Persistence;
use crate::consent;
use crate::credentials;
use crate::error::CommandError;
use crate::logging::measured;
use crate::provider_chat::{
    self, ChatControl, ChatEvent, ChatRequest, ChatRun, ChatStatus, OPENROUTER_CHAT_URL,
    PROVIDER_CHAT_ENABLED,
};
use crate::provider_transport::{Limits, SystemResolver};
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

/// Deletes an artifact of a conversation. The argument is `{ conversationId, artifactId }`. The file the artifact names, if
/// any, stays in the project folder.
#[tauri::command]
pub async fn artifacts_delete(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<noct_core::ipc::artifacts::ArtifactDeletion, CommandError> {
    let request: noct_core::ipc::artifacts::ArtifactDeleteRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::artifact_delete(&guard, &default_blocked_roots(), &request)
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

/// Reads the model bindings of a project. The argument is `{ workspace }`; the reply is the bindings, or `null`.
#[tauri::command]
pub async fn models_bindings_get(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<Option<WorkspaceModelBindings>, CommandError> {
    let request: ProjectBindingsRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::models_bindings(&guard, &default_blocked_roots(), request)
    })
    .await
}

/// Sets the default OpenRouter model of a project. The argument is `{ workspace, modelId }`; `modelId` must be present,
/// and `null` clears the default. The other bindings of the project are kept.
#[tauri::command]
pub async fn models_set_default_model(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<WorkspaceModelBindings, CommandError> {
    let request: DefaultModelRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || {
        let guard = lock(&store)?;
        services::models_set_default(&guard, &default_blocked_roots(), request)
    })
    .await
}

/// Whether the provider chat is switched on in this version. It is off until the owner has reviewed the beta.
#[tauri::command]
pub fn provider_chat_status() -> ChatStatus {
    provider_chat::status()
}

/// Starts a question about the project. The argument is `{ input: { workspace, question }, onEvent }`, where `onEvent`
/// is the channel that carries the answer: deltas in order, then `done` or `failed`. The command replies when the call
/// has started, or refuses before any file is read and before any connection is made. Nothing is written to the project.
#[tauri::command]
pub async fn provider_chat_send(
    state: State<'_, Persistence>,
    chat: State<'_, Arc<ChatControl>>,
    input: serde_json::Value,
    on_event: Channel<ChatEvent>,
) -> Result<(), CommandError> {
    let request: ChatRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    if chat.is_running() {
        return Err(CommandError::ChatBusy);
    }
    let store = state.store.clone();
    let directory = state.data_directory.clone();
    let prepared = blocking(move || {
        let guard = lock(&store)?;
        provider_chat::prepare(
            &guard,
            &default_blocked_roots(),
            request,
            PROVIDER_CHAT_ENABLED,
        )
    })
    .await?;
    let key_reference = prepared.key_reference.clone();
    let model_id = prepared.model_id.clone();
    let question = prepared.question.clone();
    let workspace = prepared.workspace.clone();
    let (key, body) = blocking(move || {
        let files = provider_chat::project_context(Path::new(&workspace))?;
        let vault = open_vault(&directory)?;
        let key = vault
            .resolve(&key_reference)
            .map_err(credentials::map_vault)?
            .ok_or(CommandError::KeyNotConfigured)?;
        Ok((key, provider_chat::body(&model_id, &question, &files)))
    })
    .await?;
    let run = ChatRun {
        approvals: noct_core::ai::network::BUILTIN_APPROVALS.to_vec(),
        resolver: Arc::new(SystemResolver),
        source: noct_core::ai::ProviderSource::Remote,
        url: OPENROUTER_CHAT_URL.to_string(),
        key,
        body,
        limits: Limits::beta_one(),
    };
    chat.start(async move {
        provider_chat::run_chat(run, &mut |event| {
            let _ = on_event.send(event);
        })
        .await;
    })
}

/// Stops the question that is running. Nothing more is sent on its channel. Replies with nothing; a cancel with no
/// question running is not an error.
#[tauri::command]
pub fn provider_chat_cancel(chat: State<'_, Arc<ChatControl>>) {
    chat.cancel();
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

/// Whether the OpenRouter key is configured. The reply carries no key.
#[tauri::command]
pub async fn provider_key_status(
    state: State<'_, Persistence>,
) -> Result<ProviderKeyStatus, CommandError> {
    let store = state.store.clone();
    let directory = state.data_directory.clone();
    blocking(move || {
        credentials::status(
            &store,
            || open_vault(&directory),
            ProviderKeyProvider::OpenRouter,
        )
    })
    .await
}

/// Saves the OpenRouter key. The argument is `{ provider, key }`. The key is validated, then written to the operating
/// system store, and the reply carries no key. A missing store is refused with `secure-storage-unavailable`.
#[tauri::command]
pub async fn provider_key_save(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<ProviderKeyStatus, CommandError> {
    let request: ProviderKeyInput =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    let directory = state.data_directory.clone();
    blocking(move || credentials::save(&store, || open_vault(&directory), request)).await
}

/// Removes the OpenRouter key from the operating system store and clears its reference.
#[tauri::command]
pub async fn provider_key_remove(
    state: State<'_, Persistence>,
) -> Result<ProviderKeyStatus, CommandError> {
    let store = state.store.clone();
    let directory = state.data_directory.clone();
    blocking(move || {
        credentials::remove(
            &store,
            || open_vault(&directory),
            ProviderKeyProvider::OpenRouter,
        )
    })
    .await
}

/// Whether the project has consented to send its context to OpenRouter. The argument is `{ workspace }`.
#[tauri::command]
pub async fn provider_consent_status(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<ProjectConsentStatus, CommandError> {
    let request: ProjectConsentRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || consent::status(&store, request)).await
}

/// Records the consent of a saved project. The interface sends it only after its consent screen is confirmed.
#[tauri::command]
pub async fn provider_consent_grant(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<ProjectConsentStatus, CommandError> {
    let request: ProjectConsentRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || consent::grant(&store, request)).await
}

/// Removes the consent of a saved project.
#[tauri::command]
pub async fn provider_consent_revoke(
    state: State<'_, Persistence>,
    input: serde_json::Value,
) -> Result<ProjectConsentStatus, CommandError> {
    let request: ProjectConsentRequest =
        serde_json::from_value(input).map_err(|_| CommandError::InvalidInput)?;
    let store = state.store.clone();
    blocking(move || consent::revoke(&store, request)).await
}

fn open_vault(data_directory: &Path) -> Result<ProviderCredentialVault, CommandError> {
    ProviderCredentialVault::open(data_directory.join(credentials::VAULT_DIRECTORY))
        .map_err(credentials::map_vault)
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
