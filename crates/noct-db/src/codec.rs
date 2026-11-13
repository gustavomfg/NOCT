//! Stored text for unit enums. The text is the enum's serde representation, so what the
//! database holds and what the contract accepts cannot diverge.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{Error, Result};

pub fn enum_text<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value).map_err(|error| Error::Message(error.to_string()))? {
        Value::String(text) => Ok(text),
        _ => Err(Error::Message("Valor persistido inválido.".to_string())),
    }
}

pub fn enum_value<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(Value::String(text.to_owned())).ok()
}
