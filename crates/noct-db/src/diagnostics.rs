//! Diagnostic events of the persistence layer.
//!
//! The local diagnostic log of the TypeScript application (`electron/logging/Logger.ts`) has no Rust
//! counterpart yet. The application connects a [`DiagnosticSink`] to its log when it runs this layer.
//! Until then the default sink writes to standard error, so that a failure is never silent. Messages
//! carry operational text only: a SQLite error, never a row, a path or a prompt.

use std::fmt::Debug;

/// Receives the warnings of the persistence layer.
pub trait DiagnosticSink: Send + Sync + Debug {
    /// Records a warning. `event` names the operation, such as `persistence.statistics`.
    fn warn(&self, event: &'static str, message: &str);
}

/// The default sink: one line on standard error.
#[derive(Debug, Default, Clone, Copy)]
pub struct StandardErrorSink;

impl DiagnosticSink for StandardErrorSink {
    fn warn(&self, event: &'static str, message: &str) {
        eprintln!("noct-db warn {event}: {message}");
    }
}
