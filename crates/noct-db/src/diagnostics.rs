//! Diagnostic events of the persistence layer.
//!
//! The application connects a [`DiagnosticSink`] to its log when it opens the store. The log of the
//! TypeScript application (`electron/logging/Logger.ts`) has the same fields: a level, a category, a
//! message and structured data. Until the application connects one, the default sink writes to
//! standard error, so that a failure is never silent. Messages carry operational text only: a SQLite
//! error, never a row, a path or a prompt.

use std::fmt::Debug;

use serde::Serialize;
use serde_json::Value;

/// The severity of a record. The names are the ones of the TypeScript log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// One diagnostic record. The sink redacts `data` before it writes it.
#[derive(Debug, Clone, Copy)]
pub struct Diagnostic<'a> {
    pub level: Level,
    /// The category, such as `persistence.statistics`. A warning names its operation here.
    pub category: &'static str,
    pub message: &'a str,
    pub data: Option<&'a Value>,
}

/// Receives the diagnostics of the persistence layer.
pub trait DiagnosticSink: Send + Sync + Debug {
    /// Records one diagnostic.
    fn record(&self, diagnostic: &Diagnostic<'_>);

    /// Records a warning. `event` names the operation, such as `persistence.statistics`, and becomes
    /// the category of the record.
    fn warn(&self, event: &'static str, message: &str) {
        self.record(&Diagnostic {
            level: Level::Warn,
            category: event,
            message,
            data: None,
        });
    }
}

/// The default sink: one line on standard error.
#[derive(Debug, Default, Clone, Copy)]
pub struct StandardErrorSink;

impl DiagnosticSink for StandardErrorSink {
    fn record(&self, diagnostic: &Diagnostic<'_>) {
        eprintln!(
            "noct-db {} {}: {}",
            diagnostic.level.as_str(),
            diagnostic.category,
            diagnostic.message
        );
    }
}
