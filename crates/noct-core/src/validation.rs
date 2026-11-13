//! Validation rules shared by the contracts. They reproduce the Zod schemas of
//! `shared/`: strings are trimmed with JavaScript's `trim` before their length is checked.

use std::fmt;

use crate::text::{js_trim, utf16_len};

/// A value that a TypeScript schema would reject. The message names the field and is
/// safe to show; it never includes the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractError(String);

impl ContractError {
    pub fn invalid(field: &str) -> Self {
        Self(format!("Campo inválido: {field}."))
    }

    pub fn message(text: &str) -> Self {
        Self(text.to_string())
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ContractError {}

/// `z.string().trim().min(1).max(max)`: returns the trimmed text.
pub fn text(field: &str, value: &str, max: u64) -> Result<String, ContractError> {
    let trimmed = js_trim(value);
    let length = utf16_len(trimmed) as u64;
    if length == 0 || length > max {
        return Err(ContractError::invalid(field));
    }
    Ok(trimmed.to_string())
}

/// `z.string().trim().min(1).max(max).optional()`.
pub fn optional_text(
    field: &str,
    value: Option<String>,
    max: u64,
) -> Result<Option<String>, ContractError> {
    value
        .map(|text_value| text(field, &text_value, max))
        .transpose()
}

/// `z.number().int().positive().optional()`: any present value must be at least 1.
pub fn optional_positive(field: &str, value: Option<u64>) -> Result<Option<u64>, ContractError> {
    match value {
        Some(0) => Err(ContractError::invalid(field)),
        other => Ok(other),
    }
}

/// `z.string().uuid()`: the canonical hyphenated form, with a version digit from 1 to 8,
/// an RFC variant, or the nil and max UUIDs. Returns the value unchanged when it passes.
pub fn uuid_text(field: &str, value: &str) -> Result<String, ContractError> {
    if is_zod_uuid(value) {
        Ok(value.to_string())
    } else {
        Err(ContractError::invalid(field))
    }
}

fn is_zod_uuid(value: &str) -> bool {
    if value == "00000000-0000-0000-0000-000000000000"
        || value == "ffffffff-ffff-ffff-ffff-ffffffffffff"
    {
        return true;
    }
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    let hyphens = [8, 13, 18, 23];
    for (index, byte) in bytes.iter().enumerate() {
        if hyphens.contains(&index) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    let version = bytes[14];
    let variant = bytes[19];
    matches!(version, b'1'..=b'8') && matches!(variant, b'8' | b'9' | b'a' | b'b' | b'A' | b'B')
}
