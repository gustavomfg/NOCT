//! The error a command returns to the interface. It carries a code and a message the interface can show,
//! and nothing else: the causes stay in the main process, so no path, query or stack reaches the page.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandError {
    /// The arguments do not match the contract of the command.
    InvalidInput,
    /// The local database or the diagnostic file could not be used.
    Persistence,
    /// Something the shell did not expect, such as a worker that failed to finish.
    Internal,
    /// The request names a project that is not saved. The message is the one the TypeScript shell gives for it.
    UnknownWorkspace,
    /// The request names a conversation that does not exist. The message is the one the TypeScript shell gives.
    UnknownConversation,
    /// The request names a brain memory that does not exist in the project. The message is the TypeScript one.
    UnknownMemory,
    /// The request moves a memory to a status its lifecycle does not allow. The TypeScript message also names the two
    /// statuses; this one does not, so the interface shows the same refusal without them.
    InvalidTransition,
    /// The operating system offers no secure credential store, so a key cannot be saved. Nothing is written.
    SecureStorageUnavailable,
    /// The credential vault refused or failed a read or a write. The cause stays in the main process.
    CredentialStore,
    /// The request names a suggestion that does not belong to the conversation. The message is the TypeScript one.
    UnknownSuggestion,
    /// The request moves a suggestion to a status its lifecycle does not allow. Unlike the memory message, this one
    /// names the area, as the TypeScript message does, but not the two statuses.
    InvalidSuggestionTransition,
    /// The request names an artifact that is not in the conversation, or was already removed. The message is the
    /// TypeScript one.
    UnknownArtifact,
    /// The provider chat is switched off in this version. Nothing is read or sent.
    ChatDisabled,
    /// The project has no consent for the provider. Nothing is read or sent.
    ConsentRequired,
    /// The project has no default OpenRouter model. Nothing is read or sent.
    ModelNotSet,
    /// No OpenRouter key is saved. Nothing is read or sent.
    KeyNotConfigured,
    /// A question is already running. The new one is not started.
    ChatBusy,
    /// The path of a change is outside the project, a symbolic link, or a sensitive name. Nothing is written.
    PathNotAllowed,
    /// The file of a change is not a regular text file, is too large, or cannot be read as text. Nothing is written.
    FileNotEditable,
    /// The file changed after the change was proposed, or after it was applied. Nothing is written.
    ChangeConflict,
    /// The request names a change that is not in the journal.
    UnknownChange,
    /// The change is not in a status that allows the request. Nothing is written.
    InvalidChangeTransition,
    /// The journal already holds the most changes that may be open. Nothing is proposed.
    TooManyOpenChanges,
    /// The file of a change could not be written or removed. The journal is set back, so nothing is recorded as done.
    ChangeWrite,
    /// The folder chosen as a project cannot be one: the message says why (a broad or system folder, not a folder,
    /// missing, or not readable). Nothing is saved.
    WorkspaceRefused(&'static str),
    /// The folder chosen as a project is already saved. Nothing is saved again.
    WorkspaceExists,
    /// The model list could not be reached: the network, the policy, a timeout or an error status.
    ModelsNetwork,
    /// The model list reply is not a list, or was cut before its end.
    ModelsInvalid,
    /// The model list reply is over its byte cap or holds too many entries.
    ModelsTooLarge,
    /// The read of the model list was cancelled.
    ModelsCancelled,
    /// A read of the model list is already running.
    ModelsBusy,
}

impl CommandError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid-input",
            Self::Persistence => "persistence",
            Self::Internal => "internal",
            Self::UnknownWorkspace => "unknown-workspace",
            Self::UnknownConversation => "unknown-conversation",
            Self::UnknownMemory => "unknown-memory",
            Self::InvalidTransition => "invalid-transition",
            Self::SecureStorageUnavailable => "secure-storage-unavailable",
            Self::CredentialStore => "credential-store",
            Self::UnknownSuggestion => "unknown-suggestion",
            Self::InvalidSuggestionTransition => "invalid-suggestion-transition",
            Self::UnknownArtifact => "unknown-artifact",
            Self::ChatDisabled => "chat-disabled",
            Self::ConsentRequired => "consent-required",
            Self::ModelNotSet => "model-not-set",
            Self::KeyNotConfigured => "key-not-configured",
            Self::ChatBusy => "chat-busy",
            Self::PathNotAllowed => "path-not-allowed",
            Self::FileNotEditable => "file-not-editable",
            Self::ChangeConflict => "change-conflict",
            Self::UnknownChange => "unknown-change",
            Self::InvalidChangeTransition => "invalid-change-transition",
            Self::TooManyOpenChanges => "too-many-open-changes",
            Self::ChangeWrite => "change-write",
            Self::WorkspaceRefused(_) => "workspace-refused",
            Self::WorkspaceExists => "workspace-exists",
            Self::ModelsNetwork => "models-network",
            Self::ModelsInvalid => "models-invalid",
            Self::ModelsTooLarge => "models-too-large",
            Self::ModelsCancelled => "models-cancelled",
            Self::ModelsBusy => "models-busy",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidInput => "Entrada inválida.",
            Self::Persistence => "Não foi possível acessar os dados locais.",
            Self::Internal => "Falha interna do NOCT.",
            Self::UnknownWorkspace => "Pasta do projeto não encontrada.",
            Self::UnknownConversation => "Conversa não encontrada.",
            Self::UnknownMemory => "Memória não encontrada.",
            Self::InvalidTransition => "Transição de memória inválida.",
            Self::SecureStorageUnavailable => {
                "O sistema operacional não oferece armazenamento seguro de credenciais. Nada foi salvo."
            }
            Self::CredentialStore => {
                "Não foi possível acessar o armazenamento seguro de credenciais. Cadastre a chave novamente."
            }
            Self::UnknownSuggestion => "Sugestão não pertence a esta conversa.",
            Self::InvalidSuggestionTransition => "Transição de sugestão inválida.",
            Self::UnknownArtifact => "Artefato não encontrado ou já removido.",
            Self::ChatDisabled => {
                "O chat com o provedor está desligado nesta versão. Nada foi enviado."
            }
            Self::ConsentRequired => {
                "Este projeto ainda não tem consentimento para o OpenRouter. Nada foi enviado."
            }
            Self::ModelNotSet => {
                "Escolha um modelo padrão para este projeto antes de perguntar. Nada foi enviado."
            }
            Self::KeyNotConfigured => {
                "Salve uma chave do OpenRouter antes de perguntar. Nada foi enviado."
            }
            Self::ChatBusy => "Já existe uma pergunta em andamento.",
            Self::PathNotAllowed => {
                "Este caminho não pode ser alterado pelo NOCT. Nada foi escrito."
            }
            Self::FileNotEditable => {
                "Este arquivo não pode ser alterado pelo NOCT: não é texto, é grande demais ou não é um arquivo comum. Nada foi escrito."
            }
            Self::ChangeConflict => "O arquivo mudou desde a proposta. Nada foi escrito.",
            Self::UnknownChange => "Alteração não encontrada.",
            Self::InvalidChangeTransition => {
                "Esta alteração não pode receber essa ação agora. Nada foi escrito."
            }
            Self::TooManyOpenChanges => {
                "Há alterações demais em aberto. Aprove, rejeite ou desfaça uma antes de propor outra."
            }
            Self::ChangeWrite => {
                "Não foi possível escrever o arquivo do projeto. Nada foi alterado."
            }
            Self::WorkspaceRefused(message) => message,
            Self::WorkspaceExists => "Este projeto já está na lista.",
            Self::ModelsNetwork => "Não foi possível buscar a lista de modelos do OpenRouter.",
            Self::ModelsInvalid => "O OpenRouter respondeu com uma lista de modelos inválida.",
            Self::ModelsTooLarge => "A lista de modelos do OpenRouter é grande demais.",
            Self::ModelsCancelled => "A busca da lista de modelos foi cancelada.",
            Self::ModelsBusy => "A lista de modelos já está sendo buscada.",
        }
    }
}

impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("CommandError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", self.message())?;
        state.end()
    }
}
