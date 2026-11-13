//! AI contracts: models, provider sources and workspace model bindings. Port of
//! `shared/ai`.

pub mod bindings;
pub mod model;
pub mod provider_configuration;

use serde::{Deserialize, Serialize};

/// Where a provider runs. Port of `ProviderSource` in `shared/ai/provider.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderSource {
    Local,
    Remote,
}
