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
