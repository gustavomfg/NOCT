//! Workspace model bindings. Port of `shared/ai/bindings.ts` and `bindingSchemas.ts`.

use serde::{Deserialize, Serialize};

use super::model::ModelReference;
use crate::validation::{ContractError, text};
use crate::wire::present;

/// `z.string().trim().min(1).max(512)`, as in `bindingSchemas.ts`.
const WORKSPACE_ID_CHARACTERS: u64 = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WorkspaceModelBindings {
    pub workspace_id: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_binding: Option<ModelReference>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub embedding_binding: Option<ModelReference>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub remote_embedding_allowed: Option<bool>,
}

impl WorkspaceModelBindings {
    pub fn validated(self) -> Result<Self, ContractError> {
        Ok(Self {
            workspace_id: text("workspaceId", &self.workspace_id, WORKSPACE_ID_CHARACTERS)?,
            default_binding: self
                .default_binding
                .map(ModelReference::validated)
                .transpose()?,
            embedding_binding: self
                .embedding_binding
                .map(ModelReference::validated)
                .transpose()?,
            remote_embedding_allowed: self.remote_embedding_allowed,
        })
    }
}
