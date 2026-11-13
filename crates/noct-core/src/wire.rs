//! Wire helpers that reproduce what the TypeScript side does when it serializes and
//! parses JSON: `JSON.stringify` number text, and Zod's `.optional()`, which accepts an
//! absent key but rejects an explicit `null`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

/// The text `JSON.stringify` writes for a number (ECMAScript `Number::toString`).
/// Integral values carry no fraction, and very large or very small values use an
/// exponent with an explicit sign: `1e+21`, `1e-7`. Non-finite values become `null`.
pub fn js_number_string(value: f64) -> String {
    if !value.is_finite() {
        return "null".to_string();
    }
    if value == 0.0 {
        return "0".to_string();
    }
    let (sign, magnitude) = if value < 0.0 {
        ("-", -value)
    } else {
        ("", value)
    };
    // `{:e}` yields the shortest digits that round-trip, which is what ECMAScript requires.
    let scientific = format!("{magnitude:e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("`{:e}` always writes an exponent");
    let exponent: i32 = exponent.parse().expect("exponent is an integer");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exponent + 1;

    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        let (whole, fraction) = digits.split_at(n as usize);
        format!("{whole}.{fraction}")
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let exponent_sign = if e < 0 { "-" } else { "+" };
        let mantissa = if k == 1 {
            digits
        } else {
            let (first, rest) = digits.split_at(1);
            format!("{first}.{rest}")
        };
        format!("{mantissa}e{exponent_sign}{}", e.abs())
    };
    format!("{sign}{body}")
}

/// Serializes an `f64` with [`js_number_string`], so that `2.0` is written as `2`.
pub mod js_number {
    use super::*;

    pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
        let raw =
            RawValue::from_string(js_number_string(*value)).map_err(serde::ser::Error::custom)?;
        raw.serialize(serializer)
    }

    pub fn serialize_option<S: Serializer>(
        value: &Option<f64>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => serialize(value, serializer),
            None => serializer.serialize_none(),
        }
    }
}

/// Deserializes an optional field the way Zod's `.optional()` does: an absent key is
/// `None` (through `#[serde(default)]`), and an explicit `null` is an error, not `None`.
pub fn present<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}
