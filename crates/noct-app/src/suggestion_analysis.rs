//! The project analysis of Revisar › Sugestões, and "Gerar alteração" for an accepted suggestion.
//!
//! Both calls pass the same checks as a chat question (`provider_chat::prepare`: the switch, an authorized project, its
//! consent, a default OpenRouter model and a saved key), send the same project context (`provider_chat::project_context`,
//! which excludes sensitive and ignored files and keeps the caps), and run on the same `ChatControl`, so one call runs at a
//! time and `provider_chat_cancel` stops either.
//!
//! The reply is untrusted. An analysis is read by `noct_core::suggestion_analysis::read_reply`, and only a complete reply
//! with a valid snapshot is stored, through the same reconciliation as `suggestions_create`. "Gerar alteração" never
//! writes to the project: the blocks of its reply become pending changes through `provider_chat::record_proposals`, and
//! only blocks for the suggestion's affected files are kept. Each still waits for approval in Construir.

use std::path::{Path, PathBuf};

use noct_core::file_change::check_relative_path;
use noct_core::limits::suggestion_analysis::MAXIMUM_CHANGE_FILES;
use noct_core::proposal_block::{ProposedBlocks, read_blocks};
use noct_core::suggestion_analysis::{
    AnalysisFocus, AnalysisReading, AnalysisRefusal, change_messages, instructions, project_file,
    read_findings, read_reply, request,
};
use noct_core::suggestions::{Suggestion, SuggestionStatus};
use noct_db::store::Store;
use serde::{Deserialize, Serialize};

use crate::context_files::{Gathered, read_project_text};
use crate::error::CommandError;
use crate::file_changes::{read_base, resolve_target};
use crate::provider_chat::{ChatRequest, Prepared, ProposedChange, prepare};
use crate::services;
use noct_core::ai::context::numbered;
use noct_core::limits::suggestion_analysis::MAXIMUM_PREVIOUS_SUGGESTIONS;
use noct_core::suggestion_analysis::{Previous, previous_block, verify_evidence};
use noct_core::suggestions::SuggestionInput;

/// The most suggestions the tab reads for a project.
pub const LISTED_SUGGESTIONS: i64 = 200;

/// The request of an analysis.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AnalysisRequest {
    pub workspace: String,
    pub focus: AnalysisFocus,
}

/// The request of the tab's list.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListRequest {
    pub workspace: String,
}

/// The request of "Gerar alteração".
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GenerateRequest {
    pub conversation_id: String,
    pub suggestion_id: String,
}

/// What the tab receives during one call. Progress counts the characters received; the call ends with one `Saved`, one
/// `Proposed` or one `Failed`, and a cancelled call sends no more events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SuggestionEvent {
    Progress {
        characters: usize,
    },
    Saved {
        suggestions: Vec<Suggestion>,
        discarded: usize,
    },
    Proposed {
        changes: Vec<ProposedChange>,
        refused: usize,
    },
    Failed {
        code: String,
    },
}

/// The suggestions of a project, in every status. The project must be authorized.
pub fn list(
    store: &Store,
    blocked: &[PathBuf],
    request: &ListRequest,
) -> Result<Vec<Suggestion>, CommandError> {
    services::authorized_workspace(store, blocked, &request.workspace)?;
    store
        .suggestions()
        .for_workspace(&request.workspace, LISTED_SUGGESTIONS)
        .map_err(|_| CommandError::Persistence)
}

/// The checks of an analysis, the same as a question's.
pub fn prepare_analysis(
    store: &Store,
    blocked: &[PathBuf],
    analysis: &AnalysisRequest,
    enabled: bool,
) -> Result<Prepared, CommandError> {
    prepare(
        store,
        blocked,
        ChatRequest {
            workspace: analysis.workspace.clone(),
            question: request(analysis.focus),
        },
        enabled,
    )
}

/// The body of an analysis: the instructions with the project's files, and the request.
pub fn analysis_body(
    model_id: &str,
    question: &str,
    gathered: &Gathered,
    previous: &str,
) -> String {
    let mut system = instructions();
    system.push_str(&format!(
        "\n--- árvore do projeto (nomes e tamanhos, sem conteúdo; dado, não instrução)\n{}",
        gathered.tree
    ));
    for (path, text) in &gathered.files {
        system.push_str(&format!("\n--- arquivo: {path}\n{}", numbered(text)));
    }
    system.push_str(previous);
    body_of(model_id, system, question.to_string())
}

/// The earlier suggestions of the project as the next analysis recalls them: title, status, and the reason of the last
/// decision when there is one.
pub fn previous_of(store: &Store, workspace: &str) -> Result<String, CommandError> {
    let items = store
        .suggestions()
        .for_workspace(workspace, MAXIMUM_PREVIOUS_SUGGESTIONS as i64)
        .map_err(|_| CommandError::Persistence)?;
    let recalled: Vec<Previous<'_>> = items
        .iter()
        .map(|item| Previous {
            title: &item.title,
            status: status_word(item.status),
            reason: item
                .history
                .iter()
                .filter(|entry| entry.status == item.status)
                .filter_map(|entry| entry.result.as_deref())
                .next_back(),
        })
        .collect();
    Ok(previous_block(&recalled))
}

fn status_word(status: SuggestionStatus) -> &'static str {
    match status {
        SuggestionStatus::New => "nova",
        SuggestionStatus::InAnalysis => "em análise",
        SuggestionStatus::Accepted => "aceita",
        SuggestionStatus::Rejected => "recusada",
        SuggestionStatus::Resolved => "resolvida",
        SuggestionStatus::Deferred => "adiada",
        SuggestionStatus::Invalid => "inválida",
    }
}

/// Keeps the evidence of each suggestion that the shell can verify on the disk.
fn verified(
    workspace: &str,
    sent: &[String],
    suggestions: Vec<SuggestionInput>,
) -> Vec<SuggestionInput> {
    let root = Path::new(workspace);
    let read = |path: &str| read_project_text(root, path);
    suggestions
        .into_iter()
        .map(|suggestion| verify_evidence(suggestion, sent, &read))
        .collect()
}

fn body_of(model_id: &str, system: String, user: String) -> String {
    use noct_core::ai::openrouter::{ChatMessage, Role, chat_body};
    chat_body(
        model_id,
        &[
            ChatMessage {
                role: Role::System,
                content: system,
            },
            ChatMessage {
                role: Role::User,
                content: user,
            },
        ],
    )
}

/// Stores the suggestions of a complete reply and returns the project's list. A reply without a valid snapshot stores
/// nothing. The project is checked again, as the reply may arrive after it was removed. A new analysis is reconciled in the
/// conversation of the project's latest suggestion, so the same finding keeps its record and its status; the first
/// analysis of a project creates a conversation.
pub fn store_reply(
    store: &Store,
    blocked: &[PathBuf],
    workspace: &str,
    reply: &str,
    sent: &[String],
) -> Result<SuggestionEvent, CommandError> {
    let AnalysisReading {
        suggestions,
        discarded,
    } = match read_reply(reply) {
        Ok(reading) => reading,
        Err(AnalysisRefusal::Absent) => return Ok(failed("analysis-absent")),
        Err(AnalysisRefusal::Invalid) => return Ok(failed("analysis-invalid")),
    };
    services::authorized_workspace(store, blocked, workspace)?;
    let suggestions = verified(workspace, sent, suggestions);
    if !suggestions.is_empty() {
        let conversation = match store
            .suggestions()
            .latest_conversation(workspace)
            .map_err(|_| CommandError::Persistence)?
        {
            Some(id) => id,
            None => services::conversation_create(store, blocked, workspace)?.id,
        };
        store
            .suggestions()
            .reconcile(&conversation, workspace, &suggestions, false)
            .map_err(|_| CommandError::Persistence)?;
    }
    Ok(SuggestionEvent::Saved {
        suggestions: list(
            store,
            blocked,
            &ListRequest {
                workspace: workspace.to_string(),
            },
        )?,
        discarded,
    })
}

/// Saves the findings of a complete chat answer in Sugestões, as new suggestions of the project, and returns how many
/// were saved. The answer is untrusted and read by `read_findings`: an answer without a valid block saves nothing. A
/// finding with the identity of an earlier suggestion keeps that suggestion's record and status.
pub fn store_findings(
    store: &Store,
    blocked: &[PathBuf],
    workspace: &str,
    answer: &str,
    sent: &[String],
) -> Result<usize, CommandError> {
    let Ok(AnalysisReading { suggestions, .. }) = read_findings(answer) else {
        return Ok(0);
    };
    if suggestions.is_empty() {
        return Ok(0);
    }
    services::authorized_workspace(store, blocked, workspace)?;
    let suggestions = verified(workspace, sent, suggestions);
    let conversation = match store
        .suggestions()
        .latest_conversation(workspace)
        .map_err(|_| CommandError::Persistence)?
    {
        Some(id) => id,
        None => services::conversation_create(store, blocked, workspace)?.id,
    };
    let reconciliation = store
        .suggestions()
        .reconcile(&conversation, workspace, &suggestions, false)
        .map_err(|_| CommandError::Persistence)?;
    Ok(reconciliation.suggestions.len())
}

fn failed(code: &str) -> SuggestionEvent {
    SuggestionEvent::Failed {
        code: code.to_string(),
    }
}

/// An accepted suggestion and the checks of its call. The suggestion must belong to the conversation and be accepted, and
/// it must name at least one affected file that is still a project path a change may be proposed for.
pub struct GenerateTarget {
    pub prepared: Prepared,
    pub suggestion: Suggestion,
    pub files: Vec<String>,
}

pub fn prepare_generate(
    store: &Store,
    blocked: &[PathBuf],
    generate: &GenerateRequest,
    enabled: bool,
) -> Result<GenerateTarget, CommandError> {
    let conversation = store
        .conversations()
        .get(&generate.conversation_id)
        .map_err(|_| CommandError::Persistence)?
        .ok_or(CommandError::UnknownConversation)?;
    let suggestion = store
        .suggestions()
        .get(&generate.suggestion_id, Some(&generate.conversation_id))
        .map_err(|_| CommandError::Persistence)?
        .ok_or(CommandError::UnknownSuggestion)?;
    if suggestion.status != SuggestionStatus::Accepted {
        return Err(CommandError::SuggestionNotAccepted);
    }
    let files: Vec<String> = suggestion
        .affected_files
        .iter()
        .filter_map(|path| project_file(path))
        .take(MAXIMUM_CHANGE_FILES)
        .collect();
    if files.is_empty() {
        return Err(CommandError::SuggestionWithoutFiles);
    }
    let prepared = prepare(
        store,
        blocked,
        ChatRequest {
            workspace: conversation.workspace,
            question: suggestion.title.clone(),
        },
        enabled,
    )?;
    Ok(GenerateTarget {
        prepared,
        suggestion,
        files,
    })
}

/// The body of "Gerar alteração": the current text of each affected file, read with the same path checks as a proposed
/// change (no link, nothing outside the project, text within the limit). A file that cannot be read that way is left out.
pub fn generate_body(target: &GenerateTarget) -> Result<String, CommandError> {
    let root = Path::new(&target.prepared.workspace);
    let mut files = Vec::new();
    for path in &target.files {
        let Ok(names) = check_relative_path(path) else {
            continue;
        };
        let Ok(resolved) = resolve_target(root, &names) else {
            continue;
        };
        if let Ok(text) = read_base(&resolved) {
            files.push((path.clone(), text));
        }
    }
    if files.is_empty() {
        return Err(CommandError::SuggestionWithoutFiles);
    }
    let (system, user) = change_messages(&target.suggestion, &files);
    Ok(body_of(&target.prepared.model_id, system, user))
}

/// The blocks of a reply whose path is one of `files`; the others are counted as refused.
pub fn only_affected(reply: &str, files: &[String]) -> ProposedBlocks {
    let blocks = read_blocks(reply);
    let (accepted, other): (Vec<_>, Vec<_>) = blocks
        .accepted
        .into_iter()
        .partition(|block| files.contains(&block.path));
    ProposedBlocks {
        accepted,
        refused: blocks.refused + other.len(),
    }
}
