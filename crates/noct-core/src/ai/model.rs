//! Model descriptors. Port of `shared/ai/model.ts` and `shared/ai/modelSchemas.ts`.
//!
//! Field order matches the Zod output order, so `serde_json::to_string` writes the same
//! text as `JSON.stringify` on the parsed TypeScript descriptor.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::ProviderSource;
use crate::limits::model as limits;
use crate::validation::{ContractError, optional_positive, optional_text, text};
use crate::wire::{js_number, present};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelCapability {
    Chat,
    Streaming,
    ToolCalling,
    Vision,
    StructuredOutput,
    Reasoning,
    Embeddings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelAvailability {
    Available,
    Disabled,
    Offline,
    MissingCredentials,
    Incompatible,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Currency {
    #[serde(rename = "USD")]
    Usd,
}

/// `providerId` and `modelId` together identify a model.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModelReference {
    pub provider_id: String,
    pub model_id: String,
}

impl ModelReference {
    pub fn validated(self) -> Result<Self, ContractError> {
        Ok(Self {
            provider_id: text(
                "providerId",
                &self.provider_id,
                limits::IDENTIFIER_CHARACTERS,
            )?,
            model_id: text("modelId", &self.model_id, limits::IDENTIFIER_CHARACTERS)?,
        })
    }
}

/// Prices are per million tokens. A missing price is `None`, never zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModelPricing {
    pub currency: Currency,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub input_per_million_tokens: Option<f64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub cached_input_per_million_tokens: Option<f64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub output_per_million_tokens: Option<f64>,
    pub effective_at: String,
}

impl ModelPricing {
    pub fn validated(self) -> Result<Self, ContractError> {
        for (field, price) in [
            ("inputPerMillionTokens", self.input_per_million_tokens),
            (
                "cachedInputPerMillionTokens",
                self.cached_input_per_million_tokens,
            ),
            ("outputPerMillionTokens", self.output_per_million_tokens),
        ] {
            if let Some(value) = price
                && !(value.is_finite() && value >= 0.0)
            {
                return Err(ContractError::invalid(field));
            }
        }
        if self.input_per_million_tokens.is_none()
            && self.cached_input_per_million_tokens.is_none()
            && self.output_per_million_tokens.is_none()
        {
            return Err(ContractError::message(
                "Informe ao menos um preço normalizado.",
            ));
        }
        Ok(Self {
            effective_at: text(
                "effectiveAt",
                &self.effective_at,
                limits::PRICING_EFFECTIVE_AT_CHARACTERS,
            )?,
            ..self
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModelDescriptor {
    pub provider_id: String,
    pub model_id: String,
    pub display_name: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub family: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub version: Option<String>,
    pub source: ProviderSource,
    pub capabilities: Vec<ModelCapability>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub context_window: Option<u64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_output_tokens: Option<u64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub pricing: Option<ModelPricing>,
    pub availability: ModelAvailability,
}

impl ModelDescriptor {
    /// Applies `modelDescriptorSchema`: trims and bounds the text, rejects zero counts,
    /// validates pricing, and rejects duplicated capabilities.
    pub fn validated(self) -> Result<Self, ContractError> {
        let mut seen = HashSet::with_capacity(self.capabilities.len());
        if !self
            .capabilities
            .iter()
            .all(|capability| seen.insert(*capability))
        {
            return Err(ContractError::message(
                "Capacidades duplicadas não são permitidas.",
            ));
        }
        Ok(Self {
            provider_id: text(
                "providerId",
                &self.provider_id,
                limits::IDENTIFIER_CHARACTERS,
            )?,
            model_id: text("modelId", &self.model_id, limits::IDENTIFIER_CHARACTERS)?,
            display_name: text(
                "displayName",
                &self.display_name,
                limits::DISPLAY_NAME_CHARACTERS,
            )?,
            family: optional_text("family", self.family, limits::FAMILY_CHARACTERS)?,
            version: optional_text("version", self.version, limits::VERSION_CHARACTERS)?,
            context_window: optional_positive("contextWindow", self.context_window)?,
            max_output_tokens: optional_positive("maxOutputTokens", self.max_output_tokens)?,
            pricing: self.pricing.map(ModelPricing::validated).transpose()?,
            ..self
        })
    }

    /// Two descriptors describe the same catalog entry when provider and model match.
    pub fn reference(&self) -> ModelReference {
        ModelReference {
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
        }
    }
}
