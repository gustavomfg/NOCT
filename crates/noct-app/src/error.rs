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
}

impl CommandError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid-input",
            Self::Persistence => "persistence",
            Self::Internal => "internal",
            Self::UnknownWorkspace => "unknown-workspace",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidInput => "Entrada inválida.",
            Self::Persistence => "Não foi possível acessar os dados locais.",
            Self::Internal => "Falha interna do NOCT.",
            Self::UnknownWorkspace => "Pasta do projeto não encontrada.",
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
