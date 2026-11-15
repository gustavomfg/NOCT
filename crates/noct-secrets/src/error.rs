use std::fmt;

/// Failure of a credential operation.
///
/// Messages are in Portuguese, like the rest of the product, and never contain secret
/// material or platform detail. Callers that need diagnostics must add them without
/// the secret value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultError {
    /// The operating system credential store is not available, or it refuses access
    /// because it is locked.
    SecureStorageUnavailable,
    /// The reference is malformed, or it is not in the index.
    InvalidReference,
    /// The secret is empty, or it is too large for the encrypted document.
    InvalidSecret,
    /// The index or the encrypted document is invalid, inaccessible, or cannot be opened with
    /// the master key.
    CorruptStore,
    /// Secrets exist, but the master key is no longer in the operating system store. The vault
    /// does not create a new key in that case, because the stored secrets could not be read.
    MasterKeyMissing,
    /// The operating system store or a file rejected a write.
    WriteFailed,
    /// The operating system store could not return a value it should hold.
    ReadFailed,
}

impl VaultError {
    /// Stable code for the renderer and diagnostics.
    pub fn code(self) -> &'static str {
        match self {
            Self::SecureStorageUnavailable => "secure-storage-unavailable",
            Self::InvalidReference => "invalid-reference",
            Self::InvalidSecret => "invalid-secret",
            Self::CorruptStore => "corrupt-store",
            Self::MasterKeyMissing => "master-key-missing",
            Self::WriteFailed => "write-failed",
            Self::ReadFailed => "read-failed",
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::SecureStorageUnavailable => {
                "O sistema operacional não oferece armazenamento seguro de credenciais."
            }
            Self::InvalidReference => "A referência de credencial é inválida ou não existe.",
            Self::InvalidSecret => "A credencial está vazia ou é grande demais.",
            Self::CorruptStore => "O cofre de credenciais está inválido ou inacessível.",
            Self::MasterKeyMissing => {
                "A chave do cofre não está no armazenamento seguro. As credenciais salvas não podem ser lidas; cadastre-as novamente."
            }
            Self::WriteFailed => "Não foi possível salvar a credencial no armazenamento seguro.",
            Self::ReadFailed => "Não foi possível acessar a credencial armazenada.",
        }
    }
}

impl fmt::Display for VaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for VaultError {}
