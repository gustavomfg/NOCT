//! The model list of OpenRouter (`GET /api/v1/models`), read into the few fields the interface shows. Pure: no request is
//! sent here.
//!
//! The endpoint is public, so the request carries no key. The reply is treated as untrusted: only `id`, `name`,
//! `context_length` and the two prices are read, an entry whose `id` fails [`validated_model_id`] is dropped, a name is
//! cut to a bound and loses its control characters, and every other field is ignored. Nothing of the reply reaches the
//! interface except these values, which the interface shows as text.

use serde::Serialize;
use serde_json::Value;

use crate::limits::model_catalog::{MAXIMUM_MODELS, MAXIMUM_NAME_CHARACTERS};

use super::openrouter::validated_model_id;

/// The path of the model list under the OpenRouter base URL.
pub const OPENROUTER_MODELS_PATH: &str = "/api/v1/models";

/// One model of the list, as the interface shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    /// The context window, in tokens, when the reply states a positive whole number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
    /// Both the prompt and the completion price are exactly `"0"`.
    pub free: bool,
}

/// Why a reply could not be read as a model list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogError {
    /// The reply is not JSON, or has no `data` list.
    Invalid,
    /// The list holds more entries than [`MAXIMUM_MODELS`].
    TooMany,
}

/// Reads a reply into the list the interface shows, sorted by name and then by id. Entries that are not objects, or whose
/// `id` is not a valid model identifier, are dropped; the same `id` twice keeps the first.
pub fn parse_models(body: &[u8]) -> Result<Vec<CatalogModel>, CatalogError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| CatalogError::Invalid)?;
    let entries = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or(CatalogError::Invalid)?;
    if entries.len() > MAXIMUM_MODELS {
        return Err(CatalogError::TooMany);
    }
    let mut models: Vec<CatalogModel> = Vec::new();
    for entry in entries {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if validated_model_id(id).is_err() || models.iter().any(|model| model.id == id) {
            continue;
        }
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .map(clean_name)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| id.to_string());
        let context_length = entry
            .get("context_length")
            .and_then(Value::as_u64)
            .filter(|length| *length > 0);
        let pricing = entry.get("pricing");
        let price = |field: &str| pricing.and_then(|p| p.get(field)).and_then(Value::as_str);
        let free = price("prompt") == Some("0") && price("completion") == Some("0");
        models.push(CatalogModel {
            id: id.to_string(),
            name,
            context_length,
            free,
        });
    }
    models.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(models)
}

/// A display name without control characters, with its spaces collapsed, and no longer than the bound.
fn clean_name(raw: &str) -> String {
    let collapsed = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    collapsed.chars().take(MAXIMUM_NAME_CHARACTERS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(entries: &str) -> Vec<u8> {
        format!(r#"{{"data":[{entries}]}}"#).into_bytes()
    }

    #[test]
    fn a_valid_list_keeps_only_the_shown_fields_and_marks_free_models() {
        let models = parse_models(&body(
            r#"{"id":"deepseek/deepseek-chat-v3:free","name":"DeepSeek V3 (free)","context_length":131072,
                "pricing":{"prompt":"0","completion":"0"},"description":"<b>x</b>","extra":{"a":1}},
               {"id":"openai/gpt-4o","name":"GPT-4o","context_length":128000,
                "pricing":{"prompt":"0.0000025","completion":"0.00001"}}"#,
        ))
        .expect("read");
        assert_eq!(
            models,
            vec![
                CatalogModel {
                    id: "deepseek/deepseek-chat-v3:free".into(),
                    name: "DeepSeek V3 (free)".into(),
                    context_length: Some(131_072),
                    free: true,
                },
                CatalogModel {
                    id: "openai/gpt-4o".into(),
                    name: "GPT-4o".into(),
                    context_length: Some(128_000),
                    free: false,
                },
            ]
        );
        let json = serde_json::to_string(&models[0]).expect("serializes");
        assert!(!json.contains("description") && !json.contains("extra"));
    }

    #[test]
    fn a_malformed_id_is_dropped_and_the_rest_is_kept() {
        let models = parse_models(&body(
            r#"{"id":"no-vendor","name":"a"},{"id":"a/b/c","name":"b"},{"id":"<script>/x","name":"c"},
               {"id":42},{"name":"no id"},"text",{"id":"vendor/ok","name":"ok"},{"id":"vendor/ok","name":"twice"}"#,
        ))
        .expect("read");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "vendor/ok");
        assert_eq!(models[0].name, "ok");
    }

    #[test]
    fn a_name_loses_control_characters_and_is_cut_and_a_missing_name_is_the_id() {
        let long = "n".repeat(500);
        let models = parse_models(&body(&format!(
            r#"{{"id":"v/a","name":"A\u0000b\n\tc"}},{{"id":"v/b","name":"{long}"}},{{"id":"v/c"}},{{"id":"v/d","name":"   "}}"#
        )))
        .expect("read");
        assert_eq!(models[0].name, "A b c");
        assert_eq!(models[1].name.chars().count(), MAXIMUM_NAME_CHARACTERS);
        assert_eq!(models[2].name, "v/c");
        assert_eq!(models[3].name, "v/d");
    }

    #[test]
    fn a_price_that_is_not_exactly_zero_is_not_free() {
        let models = parse_models(&body(
            r#"{"id":"v/a","pricing":{"prompt":"0","completion":"0.1"}},{"id":"v/b","pricing":{"prompt":0,"completion":0}},
               {"id":"v/c"},{"id":"v/d","context_length":-5},{"id":"v/e","context_length":"128000"}"#,
        ))
        .expect("read");
        assert!(models.iter().all(|model| !model.free));
        assert!(models.iter().all(|model| model.context_length.is_none()));
    }

    #[test]
    fn a_reply_that_is_not_a_list_is_invalid_and_a_long_list_is_refused() {
        assert_eq!(parse_models(b"<html>"), Err(CatalogError::Invalid));
        assert_eq!(parse_models(br#"{"data":{}}"#), Err(CatalogError::Invalid));
        assert_eq!(
            parse_models(br#"{"data":[{"id":"v/a""#),
            Err(CatalogError::Invalid)
        );
        let many = vec![r#"{"id":"v/a"}"#; MAXIMUM_MODELS + 1].join(",");
        assert_eq!(parse_models(&body(&many)), Err(CatalogError::TooMany));
    }
}
