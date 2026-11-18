//! Server-sent events, as OpenAI-compatible providers send them. Pure: bytes in, events out.
//!
//! The parser is strict. A field it does not know, a carriage return that is not part of a `\r\n` line end, bytes that
//! are not UTF-8, a line or an event over its cap, and a stream that ends inside a line or an event are all errors. None
//! is skipped, because a skipped event would make a truncated answer look complete.

use crate::limits::provider_chat::MAXIMUM_EVENT_BYTES;

/// One event: the data lines of the event, joined with a newline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub data: String,
}

/// Why the stream cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SseError {
    NotUtf8,
    Malformed,
    TooLarge,
    Truncated,
}

/// The parser state. Feed it the bytes of the body as they arrive, then call [`SseParser::finish`] at the end.
#[derive(Debug, Default)]
pub struct SseParser {
    line: Vec<u8>,
    data: Vec<String>,
    data_bytes: usize,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads the bytes and returns the events they complete. An error ends the parse: no later byte is read.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, SseError> {
        let mut events = Vec::new();
        for &byte in bytes {
            if byte == b'\n' {
                let mut line = std::mem::take(&mut self.line);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                self.take_line(&line, &mut events)?;
            } else {
                self.line.push(byte);
                if self.line.len() > MAXIMUM_EVENT_BYTES {
                    return Err(SseError::TooLarge);
                }
            }
        }
        Ok(events)
    }

    /// Ends the stream. A partial line, or an event with no blank line after it, is a truncated stream.
    pub fn finish(self) -> Result<(), SseError> {
        if self.line.is_empty() && self.data.is_empty() {
            Ok(())
        } else {
            Err(SseError::Truncated)
        }
    }

    fn take_line(&mut self, line: &[u8], events: &mut Vec<SseEvent>) -> Result<(), SseError> {
        let text = std::str::from_utf8(line).map_err(|_| SseError::NotUtf8)?;
        if text.contains('\r') {
            return Err(SseError::Malformed);
        }
        if text.is_empty() {
            if !self.data.is_empty() {
                events.push(SseEvent {
                    data: self.data.join("\n"),
                });
                self.data.clear();
                self.data_bytes = 0;
            }
            return Ok(());
        }
        if text.starts_with(':') {
            return Ok(());
        }
        let (field, value) = match text.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (text, ""),
        };
        match field {
            "data" => {
                self.data_bytes += value.len() + 1;
                if self.data_bytes > MAXIMUM_EVENT_BYTES {
                    return Err(SseError::TooLarge);
                }
                self.data.push(value.to_string());
                Ok(())
            }
            "event" | "id" | "retry" => Ok(()),
            _ => Err(SseError::Malformed),
        }
    }
}
