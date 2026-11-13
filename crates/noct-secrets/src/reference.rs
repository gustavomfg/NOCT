use std::fmt;

use uuid::Uuid;

use crate::VaultError;

/// Opaque identifier of a stored credential. This is the only value the application
/// may persist next to a provider definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CredentialReference(Uuid);

impl CredentialReference {
    pub(crate) fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Accepts only the canonical form this crate produces: lowercase, hyphenated.
    pub fn parse(value: &str) -> Result<Self, VaultError> {
        let uuid = Uuid::parse_str(value).map_err(|_| VaultError::InvalidReference)?;
        if uuid.hyphenated().to_string() != value {
            return Err(VaultError::InvalidReference);
        }
        Ok(Self(uuid))
    }
}

impl fmt::Display for CredentialReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.hyphenated())
    }
}
