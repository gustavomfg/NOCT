//! Workspace model bindings. Port of `shared/ai/bindings.ts` and `bindingSchemas.ts`.

use serde::{Deserialize, Serialize};

use super::model::ModelReference;
use super::openrouter::validated_model_id;
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

/// The provider id of the OpenRouter models. The beta 1 shell binds only this provider from its model selector.
/// A request for the model bindings of one project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectBindingsRequest {
    pub workspace: String,
}

/// A request to set or clear the default OpenRouter model of one project. `model_id` is read as a value so that a missing
/// field is refused rather than taken for "clear": only `null` clears the default.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DefaultModelRequest {
    pub workspace: String,
    pub model_id: serde_json::Value,
}

impl DefaultModelRequest {
    /// The identifier to bind, or `None` to clear the default. Any value other than a string or `null` is refused.
    pub fn model_id(&self) -> Result<Option<&str>, ContractError> {
        match &self.model_id {
            serde_json::Value::Null => Ok(None),
            serde_json::Value::String(id) => Ok(Some(id.as_str())),
            _ => Err(ContractError::invalid("modelId")),
        }
    }
}

/// The provider id of the OpenRouter models. The beta 1 shell binds only this provider from its model selector.
pub const OPENROUTER_PROVIDER_ID: &str = "openrouter";

/// The default model of the OpenRouter provider, chosen by its identifier. The provider is fixed, so the identifier is the
/// only input, and it is checked by `validated_model_id`: its length, its characters and its `vendor/name` shape. `:free`
/// is not a rule here. It is a filter and a suggestion in the interface.
pub fn openrouter_default(model_id: &str) -> Result<ModelReference, ContractError> {
    let model_id = validated_model_id(model_id)?;
    Ok(ModelReference {
        provider_id: OPENROUTER_PROVIDER_ID.to_string(),
        model_id: model_id.to_string(),
    })
}
