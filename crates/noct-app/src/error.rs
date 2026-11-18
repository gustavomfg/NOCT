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
