//! AI contracts: models, provider sources and workspace model bindings. Port of
//! `shared/ai`.

pub mod bindings;
pub mod catalog;
pub mod consent;
pub mod context;
pub mod model;
pub mod network;
pub mod openrouter;
pub mod provider_configuration;
pub mod provider_key;
pub mod sse;
pub mod turn;

use serde::{Deserialize, Serialize};

/// Where a provider runs. Port of `ProviderSource` in `shared/ai/provider.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderSource {
    Local,
    Remote,
}
