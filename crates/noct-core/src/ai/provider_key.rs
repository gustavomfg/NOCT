//! Provider keys of the first remote provider. The key travels from the interface to the main process once, in
//! `ProviderKeyInput`, and no type in this module can carry it back: `ProviderKeyStatus` says only whether a key is
//! configured. The key is validated here before the main process stores it, so an invalid value never reaches the
//! operating system store or an HTTP header.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::limits::provider_key as limits;
use crate::validation::ContractError;

/// The providers that can hold a key. Beta 1 has one: OpenRouter, the built-in host `openrouter.ai`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKeyProvider {
    OpenRouter,
}

/// A key to save. Write-only: no command returns it, and `Debug` does not print it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProviderKeyInput {
    pub provider: ProviderKeyProvider,
    pub key: String,
}

impl fmt::Debug for ProviderKeyInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderKeyInput")
            .field("provider", &self.provider)
            .field("key", &"<redacted>")
            .finish()
    }
}

impl ProviderKeyInput {
    /// Checks the key and returns it without surrounding white space. A valid key has between the minimum and the
    /// maximum number of characters, and every character is visible ASCII (`!` to `~`). The error names the field,
    /// never the value.
    pub fn validated(self) -> Result<Self, ContractError> {
        let key = self.key.trim().to_string();
        let length = key.len() as u64;
        let visible = key.bytes().all(|byte| (0x21..=0x7e).contains(&byte));
        if !visible
            || !(limits::MINIMUM_KEY_CHARACTERS..=limits::MAXIMUM_KEY_CHARACTERS).contains(&length)
        {
            return Err(ContractError::invalid("key"));
        }
        Ok(Self { key, ..self })
    }
}

/// What the interface sees of a provider's key. It carries no secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderKeyStatus {
    pub provider: ProviderKeyProvider,
    pub configured: bool,
}
