//! The consent of a project to send its context to a provider. A consent belongs to one project and one provider, and it
//! covers one version of what is sent: the host, the context rules and the caps of `ai::network` and `ai::context`. A
//! change to any of those changes `CONSENT_VERSION`, so every project must consent again.
//!
//! The consent is not secret. The interface reads and writes it through the three commands of the app layer, and the
//! request carries the project path only.

use serde::{Deserialize, Serialize};

use crate::ai::provider_key::ProviderKeyProvider;
use crate::ipc::workspaces::PATH_CHARACTERS_MAX;
use crate::validation::ContractError;

/// What the consent covers: OpenRouter, the built-in host, the rules of `ai::context` and the caps of
/// `limits::provider_context`. Version 2 adds the project tree (names and sizes, no content), the files the person chooses,
/// and the titles of the project's earlier suggestions; a project that consented to version 1 is asked again.
pub const CONSENT_VERSION: &str = "openrouter-v2";

/// A request about one project. The path is checked for its length; the app layer checks that the project is saved.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectConsentRequest {
    pub workspace: String,
}

impl ProjectConsentRequest {
    pub fn validated(self) -> Result<Self, ContractError> {
        let length = self.workspace.chars().count();
        if length == 0 || length > PATH_CHARACTERS_MAX {
            return Err(ContractError::invalid("workspace"));
        }
        Ok(self)
    }
}

/// Whether the project has consented. The reply holds no content of the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConsentStatus {
    pub workspace: String,
    pub provider: ProviderKeyProvider,
    pub granted: bool,
}
