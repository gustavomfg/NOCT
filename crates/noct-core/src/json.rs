//! JSON values that cross a persistence boundary. Port of `shared/json.ts`.
//!
//! The TypeScript check also rejects values that `JSON.stringify` would silently alter
//! (functions, symbols, `NaN`, cycles, class instances). A `serde_json::Value` cannot
//! hold those states, so the checks that remain here are nesting depth and size.

use serde_json::Value;

/// Same limit as `JSON_VALUE_LIMITS.maxDepth` in `shared/json.ts`.
pub const MAX_JSON_DEPTH: usize = 32;

/// The value does not fit the persistence contract. The message is the one the
/// TypeScript application shows for both causes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("O metadata precisa conter apenas valores JSON válidos dentro do limite permitido.")]
pub struct JsonLimitError;

/// Serializes `value` for storage. Fails when nesting exceeds [`MAX_JSON_DEPTH`] or when
/// the output is longer than `max_characters` UTF-16 code units, which is how
/// JavaScript measures `string.length`.
pub fn serialize_within_limit(
    value: &Value,
    max_characters: usize,
) -> Result<String, JsonLimitError> {
    if !within_depth(value, 0) {
        return Err(JsonLimitError);
    }
    let serialized = serde_json::to_string(value).map_err(|_| JsonLimitError)?;
    if serialized.encode_utf16().count() > max_characters {
        return Err(JsonLimitError);
    }
    Ok(serialized)
}

fn within_depth(value: &Value, depth: usize) -> bool {
    if depth > MAX_JSON_DEPTH {
        return false;
    }
    match value {
        Value::Array(items) => items.iter().all(|item| within_depth(item, depth + 1)),
        Value::Object(map) => map.values().all(|item| within_depth(item, depth + 1)),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn nested(levels: usize) -> Value {
        (0..levels).fold(Value::Null, |inner, _| Value::Array(vec![inner]))
    }

    #[test]
    fn serializes_compactly_like_json_stringify() {
        let value = json!({ "a": [1, "b", null, true] });
        assert_eq!(
            serialize_within_limit(&value, 100).unwrap(),
            r#"{"a":[1,"b",null,true]}"#
        );
    }

    #[test]
    fn accepts_nesting_up_to_the_limit_and_rejects_one_more_level() {
        // A container at depth N holds leaves at depth N + 1; the deepest leaf of
        // `nested(levels)` sits at depth `levels`.
        assert!(serialize_within_limit(&nested(MAX_JSON_DEPTH), 1_000).is_ok());
        assert_eq!(
            serialize_within_limit(&nested(MAX_JSON_DEPTH + 1), 1_000),
            Err(JsonLimitError)
        );
    }

    #[test]
    fn measures_length_in_utf16_code_units() {
        // A single emoji is two UTF-16 code units, so the quoted string is four.
        let value = json!("😀");
        assert_eq!(serialize_within_limit(&value, 4).unwrap(), "\"😀\"");
        assert_eq!(serialize_within_limit(&value, 3), Err(JsonLimitError));
    }
}
