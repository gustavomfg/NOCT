//! The work behind the commands of the first slice. Each function takes the store and returns the
//! contract value, so it is tested without a Tauri runtime. The commands in `commands.rs` only lock the
//! store, move the work to a blocking thread and map the arguments.

use std::path::PathBuf;

use noct_core::ai::bindings::WorkspaceModelBindings;
use noct_core::ai::turn::turn_artifacts;
use noct_core::brain_memory::{
    BrainMemory, BrainMemoryScope, BrainMemorySource, CreateBrainMemoryInput,
    InitialBrainMemoryStatus, UpdateBrainMemoryInput, allowed_transition, extract_candidates,
};
use noct_core::contracts::{Conversation, Message, Role, Workspace, WorkspaceAvailability};
use noct_core::ipc::ReplyRequest;
use noct_core::ipc::ai::SaveAssistantRequest;
use noct_core::ipc::brain::{
    BrainMemoryCreateRequest, BrainMemoryExtraction, BrainMemoryUpdateRequest,
};
use noct_core::ipc::conversations::{
    ConversationCursor, ConversationPage, ConversationPageRequest,
};
use noct_core::ipc::settings::{AppSettings, SettingsUpdate, from_saved, to_saved};
use noct_core::ipc::suggestions::{SuggestionCreation, SuggestionStatusRequest};
use noct_core::ipc::workspaces::{
    WorkspaceCursor, WorkspaceFavoriteRequest, WorkspacePage, WorkspacePageRequest,
};
use noct_core::suggestions::{
    Suggestion, SuggestionSnapshot, allowed_suggestion_transition, extract_suggestions,
    review_comparison_markdown,
};
use noct_db::repositories::workspaces::WorkspaceRow;
use noct_db::store::{AssistantTurn, Store, TurnArtifact};

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

/// Checks that a saved project may take a write: its folder is available and outside the blocked roots, and its row is
/// authorized. Each refusal is `UnknownWorkspace`, and nothing is written.
fn authorized_workspace(
    store: &Store,
    blocked: &[PathBuf],
    workspace: &str,
) -> Result<(), CommandError> {
    let inspection = workspace_scope::inspect(workspace, blocked);
    if !matches!(inspection.availability, WorkspaceAvailability::Available) {
        return Err(CommandError::UnknownWorkspace);
    }
    let authorized = workspace_rows(store)?
        .iter()
        .any(|row| row.path == workspace && row.authorized);
    if authorized {
        Ok(())
    } else {
        Err(CommandError::UnknownWorkspace)
    }
}

/// Creates a conversation in a project that is saved, authorized and available. The folder must pass the scope check
/// with the blocked roots, and the project must be saved and authorized. Either refusal is `UnknownWorkspace`, and
/// nothing is written. The write opens the project and creates the conversation in one transaction.
///
/// The TypeScript shell also creates the `.nocturne` workspace context here. The Rust shell does not do that yet, so
/// this command writes the database only. The gap is recorded in `docs/status.md`.
pub fn conversation_create(
    store: &Store,
    blocked: &[PathBuf],
    workspace: &str,
) -> Result<Conversation, CommandError> {
    if workspace.is_empty() {
        return Err(CommandError::InvalidInput);
    }
    authorized_workspace(store, blocked, workspace)?;
    let row = store
        .create_conversation(workspace)
        .map_err(|_| CommandError::Persistence)?;
    Ok(Conversation {
        id: row.id,
        title: row.title,
        workspace: row.workspace,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Creates a candidate brain memory in the project of a conversation, as the TypeScript shell stores a manual memory. The
/// conversation must exist, and its project must pass the same check as a conversation write. A conversation-scoped
/// memory keeps its conversation; a workspace-scoped one keeps none. The repository writes the memory and its history
/// entry in one transaction.
pub fn brain_create(
    store: &Store,
    blocked: &[PathBuf],
    request: &BrainMemoryCreateRequest,
) -> Result<BrainMemory, CommandError> {
    let conversation = store
        .conversations()
        .get(&request.conversation_id)
        .map_err(|_| CommandError::Persistence)?
        .ok_or(CommandError::UnknownConversation)?;
    authorized_workspace(store, blocked, &conversation.workspace)?;
    let input = CreateBrainMemoryInput {
        conversation_id: match request.scope {
            BrainMemoryScope::Conversation => Some(request.conversation_id.clone()),
            BrainMemoryScope::Workspace => None,
        },
        kind: request.kind,
        scope: request.scope,
        content: request.content.clone(),
        confidence: None,
        source_type: Some(BrainMemorySource::Manual),
        source_id: None,
        status: Some(InitialBrainMemoryStatus::Candidate),
    };
    store
        .brain_memories()
        .create(&conversation.workspace, &input)
        .map_err(|_| CommandError::Persistence)
}

/// Changes a brain memory of a conversation's project. The conversation must exist, and its project must pass the check
/// of a conversation write. The memory must exist in that project, and a status change must follow the lifecycle; a
/// status that stays the same is not a change. The conversation of a conversation-scoped memory is the one of the
/// request, and a workspace-scoped memory keeps none. The repository writes the change and its history entry in one
/// transaction.
pub fn brain_update(
    store: &Store,
    blocked: &[PathBuf],
    request: &BrainMemoryUpdateRequest,
) -> Result<BrainMemory, CommandError> {
    let conversation = store
        .conversations()
        .get(&request.conversation_id)
        .map_err(|_| CommandError::Persistence)?
        .ok_or(CommandError::UnknownConversation)?;
    authorized_workspace(store, blocked, &conversation.workspace)?;
    let workspace = conversation.workspace.as_str();
    let current = store
        .brain_memories()
        .get(&request.memory_id, workspace)
        .map_err(|_| CommandError::Persistence)?
        .ok_or(CommandError::UnknownMemory)?;
    if let Some(status) = request.status
        && status != current.status
        && !allowed_transition(current.status, status)
    {
        return Err(CommandError::InvalidTransition);
    }
    let input = UpdateBrainMemoryInput {
        kind: request.kind,
        scope: request.scope,
        conversation_id: match request.scope {
            Some(BrainMemoryScope::Conversation) => Some(Some(request.conversation_id.clone())),
            Some(BrainMemoryScope::Workspace) => Some(None),
            None => None,
        },
        content: request.content.clone(),
        confidence: request.confidence,
        status: request.status,
    };
    store
        .brain_memories()
        .update(&request.memory_id, workspace, &input)
        .map_err(|_| CommandError::Persistence)
}

/// The warning of a reply whose memories could not be stored. It is the TypeScript text. The reply itself is kept.
const CANDIDATES_NOT_SAVED: &str =
    "A resposta foi preservada, mas não foi possível salvar as candidatas do Segundo Cérebro.";

/// Reads the memory blocks of an assistant reply and stores its candidates in the project of the conversation. The
/// conversation and its project are checked as in a conversation write. A failure to store is not an error: the reply
/// comes back without memories, with the warning, as the TypeScript shell does. The candidates are stored as one unit.
pub fn brain_extract(
    store: &Store,
    blocked: &[PathBuf],
    request: &ReplyRequest,
) -> Result<BrainMemoryExtraction, CommandError> {
    let conversation = store
        .conversations()
        .get(&request.conversation_id)
        .map_err(|_| CommandError::Persistence)?
        .ok_or(CommandError::UnknownConversation)?;
    authorized_workspace(store, blocked, &conversation.workspace)?;
    let extracted = extract_candidates(&request.content);
    match store.brain_memories().create_candidates(
        &conversation.workspace,
        &request.conversation_id,
        &extracted.candidates,
    ) {
        Ok(memories) => Ok(BrainMemoryExtraction {
            memories,
            content: extracted.content,
            warning: None,
        }),
        Err(_) => Ok(BrainMemoryExtraction {
            memories: Vec::new(),
            content: extracted.content,
            warning: Some(CANDIDATES_NOT_SAVED.to_string()),
        }),
    }
}

/// The warnings of a reply without a usable review snapshot. They are the TypeScript texts, and the earlier suggestions
/// are kept.
const SNAPSHOT_INVALID: &str =
    "O snapshot de sugestões é inválido ou incompleto; sugestões anteriores foram preservadas.";
const SNAPSHOT_ABSENT: &str =
    "A resposta não trouxe um snapshot estruturado; sugestões anteriores foram preservadas.";

/// Reads the review snapshot of a reply and reconciles its suggestions with the conversation's history. The conversation
/// and its project are checked as in a conversation write. A reply without a usable snapshot stores nothing: it comes
/// back with the warning and its text. The comparison with the previous review is appended to the text, as in TypeScript.
pub fn suggestions_create(
    store: &Store,
    blocked: &[PathBuf],
    request: &ReplyRequest,
) -> Result<SuggestionCreation, CommandError> {
    let conversation = store
        .conversations()
        .get(&request.conversation_id)
        .map_err(|_| CommandError::Persistence)?
        .ok_or(CommandError::UnknownConversation)?;
    authorized_workspace(store, blocked, &conversation.workspace)?;
    let extracted = extract_suggestions(&request.content);
    if !extracted.structured {
        let warning = match extracted.snapshot {
            SuggestionSnapshot::Invalid => SNAPSHOT_INVALID,
            SuggestionSnapshot::Valid | SuggestionSnapshot::Absent => SNAPSHOT_ABSENT,
        };
        return Ok(SuggestionCreation {
            suggestions: Vec::new(),
            comparison: None,
            content: extracted.content,
            warning: Some(warning.to_string()),
        });
    }
    let reconciliation = store
        .suggestions()
        .reconcile(
            &request.conversation_id,
            &conversation.workspace,
            &extracted.suggestions,
            false,
        )
        .map_err(|_| CommandError::Persistence)?;
    let markdown = review_comparison_markdown(&reconciliation.comparison);
    let content = [extracted.content.as_str(), markdown.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(SuggestionCreation {
        suggestions: reconciliation.suggestions,
        comparison: Some(reconciliation.comparison),
        content,
        warning: None,
    })
}

/// Moves a suggestion of a conversation to a new status. The conversation and its project are checked first, then the
/// suggestion must belong to the conversation, then the move must be one the lifecycle allows. Setting the status it
/// already has changes nothing, and the store writes the decision entry in the same transaction as the status.
pub fn suggestions_status(
    store: &Store,
    blocked: &[PathBuf],
    request: &SuggestionStatusRequest,
) -> Result<Suggestion, CommandError> {
    let conversation = store
        .conversations()
        .get(&request.conversation_id)
        .map_err(|_| CommandError::Persistence)?
        .ok_or(CommandError::UnknownConversation)?;
    authorized_workspace(store, blocked, &conversation.workspace)?;
    let current = store
        .suggestions()
        .get(&request.suggestion_id, Some(&request.conversation_id))
        .map_err(|_| CommandError::Persistence)?
        .ok_or(CommandError::UnknownSuggestion)?;
    if request.status != current.status
        && !allowed_suggestion_transition(current.status, request.status)
    {
        return Err(CommandError::InvalidSuggestionTransition);
    }
    store
        .suggestions()
        .set_status(
            &request.suggestion_id,
            request.status,
            request.result.as_deref(),
        )
        .map_err(|_| CommandError::Persistence)
}

/// Stores the model bindings of a project and returns them. The bindings are validated first, as the TypeScript handler
/// does, and then the project is checked as in a conversation write: it must be saved, authorized and available. A write
/// replaces the bindings the project had.
pub fn models_set_bindings(
    store: &Store,
    blocked: &[PathBuf],
    bindings: WorkspaceModelBindings,
) -> Result<WorkspaceModelBindings, CommandError> {
    let bindings = bindings
        .validated()
        .map_err(|_| CommandError::InvalidInput)?;
    authorized_workspace(store, blocked, &bindings.workspace_id)?;
    store
        .workspace_model_bindings()
        .set(bindings)
        .map_err(|_| CommandError::Persistence)
}

/// The title of an assistant reply's Markdown artifact: `Resposta · ` and the UTC time of the write. The TypeScript shell
/// writes the local time of the system instead, which this shell does not read yet.
fn reply_title() -> String {
    let format =
        time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second] UTC");
    // The components above always format; the empty fallback only keeps the title a string.
    let when = time::OffsetDateTime::now_utc()
        .format(&format)
        .unwrap_or_default();
    format!("Resposta · {when}")
}

/// Stores an assistant reply with its metadata, and the artifacts the metadata names: one per listed file, and a report
/// for a diff. The conversation and its project are checked as in a conversation write. The reply comes back as the
/// stored message.
pub fn ai_save_assistant(
    store: &Store,
    blocked: &[PathBuf],
    request: &SaveAssistantRequest,
) -> Result<Message, CommandError> {
    let conversation = store
        .conversations()
        .get(&request.conversation_id)
        .map_err(|_| CommandError::Persistence)?
        .ok_or(CommandError::UnknownConversation)?;
    authorized_workspace(store, blocked, &conversation.workspace)?;
    let derived = turn_artifacts(request.metadata.as_ref());
    let artifacts: Vec<TurnArtifact<'_>> = derived
        .iter()
        .map(|artifact| TurnArtifact {
            kind: artifact.kind,
            title: &artifact.title,
            file_path: artifact.file_path.as_deref(),
            content: artifact.content.as_deref(),
            metadata: None,
        })
        .collect();
    let title = reply_title();
    let stored = store
        .save_assistant_turn(&AssistantTurn {
            conversation_id: &request.conversation_id,
            workspace: &conversation.workspace,
            content: &request.content,
            metadata: request.metadata.as_ref(),
            markdown_title: &title,
            artifacts: &artifacts,
        })
        .map_err(|_| CommandError::Persistence)?;
    Ok(Message {
        id: stored.id,
        conversation_id: stored.conversation_id,
        role: Role::Assistant,
        content: stored.content,
        metadata: stored.metadata,
        created_at: stored.created_at,
    })
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
