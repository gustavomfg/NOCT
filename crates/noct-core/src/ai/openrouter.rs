//! The OpenAI-compatible chat protocol of OpenRouter: the request body, the reading of each stream event, the status of a
//! reply, and the check of a model identifier. Pure: no request is sent here.
//!
//! A failure is a fixed code, never the text the provider sent. A provider error body can echo the request, and the
//! request carries the key, so no part of a reply is put in a message the interface can show.

use serde_json::{Value, json};

use crate::limits::provider_chat::MAXIMUM_MODEL_CHARACTERS;
use crate::validation::ContractError;

use super::sse::SseError;

/// The roles the beta 1 request uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
        }
    }
}

/// One message of the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

/// What one stream event means: a piece of the answer, or the end of the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    Delta(String),
    Done,
}

/// Why a chat call failed. Each variant has one stable code, and the code is the only text the interface gets from the
/// adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatFailure {
    Malformed,
    Truncated,
    TooLarge,
    ProviderError,
    Unauthorized,
    RateLimited,
    Unavailable,
    Rejected,
    Transport,
    Timeout,
    Cancelled,
}

impl ChatFailure {
    /// The stable code of the failure.
    pub fn code(self) -> &'static str {
        match self {
            Self::Malformed => "stream-malformed",
            Self::Truncated => "stream-truncated",
            Self::TooLarge => "response-too-large",
            Self::ProviderError => "provider-error",
            Self::Unauthorized => "provider-unauthorized",
            Self::RateLimited => "provider-rate-limited",
            Self::Unavailable => "provider-unavailable",
            Self::Rejected => "provider-rejected",
            Self::Transport => "transport",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
        }
    }
}

impl From<SseError> for ChatFailure {
    fn from(error: SseError) -> Self {
        match error {
            SseError::NotUtf8 | SseError::Malformed => Self::Malformed,
            SseError::TooLarge => Self::TooLarge,
            SseError::Truncated => Self::Truncated,
        }
    }
}

/// The request body. It names the model, the messages, and asks for a stream. The key is not part of the body: it is
/// sent in a header by the transport.
pub fn chat_body(model: &str, messages: &[ChatMessage]) -> String {
    let messages: Vec<Value> = messages
        .iter()
        .map(|message| json!({ "role": message.role.as_str(), "content": message.content }))
        .collect();
    json!({ "model": model, "messages": messages, "stream": true }).to_string()
}

/// Reads the data of one event. `[DONE]` ends the answer. An event with no text in it, such as a role-only chunk or a
/// usage chunk, carries nothing and returns `None`. An error object in the stream is a failure, whatever it says.
pub fn read_event(data: &str) -> Result<Option<StreamEvent>, ChatFailure> {
    if data == "[DONE]" {
        return Ok(Some(StreamEvent::Done));
    }
    let value: Value = serde_json::from_str(data).map_err(|_| ChatFailure::Malformed)?;
    if value.get("error").is_some() {
        return Err(ChatFailure::ProviderError);
    }
    let choices = value
        .get("choices")
        .and_then(Value::as_array)
        .ok_or(ChatFailure::Malformed)?;
    let Some(first) = choices.first() else {
        return Ok(None);
    };
    match first.pointer("/delta/content") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.is_empty() => Ok(None),
        Some(Value::String(text)) => Ok(Some(StreamEvent::Delta(text.clone()))),
        Some(_) => Err(ChatFailure::Malformed),
    }
}

/// The failure of a reply's status, or `None` for a 2xx reply. A 3xx reply is refused by the network policy before this
/// is asked, so it is not classified here.
pub fn status_failure(status: u16) -> Option<ChatFailure> {
    match status {
        200..=299 => None,
        401 | 403 => Some(ChatFailure::Unauthorized),
        429 => Some(ChatFailure::RateLimited),
        500..=599 => Some(ChatFailure::Unavailable),
        _ => Some(ChatFailure::Rejected),
    }
}

/// Checks a model identifier of the form `vendor/name`, with an optional `:variant` such as `:free`. Only the shape is
/// checked here. Whether the model exists, or is free, is not a rule of the core: `:free` is a filter and a suggestion in
/// the interface.
pub fn validated_model_id(value: &str) -> Result<&str, ContractError> {
    let length = value.chars().count();
    if length == 0 || length > MAXIMUM_MODEL_CHARACTERS {
        return Err(ContractError::invalid("modelId"));
    }
    let allowed = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | ':' | '-'));
    let mut parts = value.split('/');
    let shape = matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(vendor), Some(name), None) if !vendor.is_empty() && !name.is_empty()
    );
    if allowed && shape {
        Ok(value)
    } else {
        Err(ContractError::invalid("modelId"))
    }
}
