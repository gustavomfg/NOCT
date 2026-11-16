//! Agent modes. Port of `agentModes` in `shared/suggestions.ts`.

use serde::{Deserialize, Serialize};

/// What an agent attempt may do. Review and docs attempts never modify the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    Build,
    Review,
    Docs,
}
