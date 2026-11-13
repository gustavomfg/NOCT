//! Provider configurations. Port of `shared/ai/providerConfiguration.ts` and
//! `providerConfigurationSchemas.ts`. Secrets are never part of these types: a summary
//! reports only whether a credential is configured.

use serde::{Deserialize, Serialize};

use super::ProviderSource;
use crate::limits::provider_configuration as limits;
use crate::validation::{ContractError, text, uuid_text};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderConfigurationType {
    ChatCompletions,
}

/// The editable part of a provider configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProviderConfigurationInput {
    pub provider_type: ProviderConfigurationType,
    pub display_name: String,
    pub source: ProviderSource,
    pub base_url: String,
    pub enabled: bool,
    pub requires_authentication: bool,
    pub timeout_ms: u64,
}

impl ProviderConfigurationInput {
    pub fn validated(self) -> Result<Self, ContractError> {
        let timeout = self.timeout_ms;
        if !(limits::MINIMUM_TIMEOUT_MS..=limits::MAXIMUM_TIMEOUT_MS).contains(&timeout) {
            return Err(ContractError::invalid("timeoutMs"));
        }
        Ok(Self {
            display_name: text(
                "displayName",
                &self.display_name,
                limits::DISPLAY_NAME_CHARACTERS,
            )?,
            base_url: text("baseUrl", &self.base_url, limits::BASE_URL_CHARACTERS)?,
            ..self
        })
    }
}

/// What a provider configuration looks like to the renderer. It carries no secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigurationSummary {
    pub id: String,
    pub provider_type: ProviderConfigurationType,
    pub display_name: String,
    pub source: ProviderSource,
    pub base_url: String,
    pub enabled: bool,
    pub requires_authentication: bool,
    pub credential_configured: bool,
    pub timeout_ms: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// Validates an identifier the way `z.string().uuid()` does.
pub fn identifier(value: &str) -> Result<String, ContractError> {
    uuid_text("id", value)
}
