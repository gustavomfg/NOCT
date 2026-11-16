//! The diagnostic sink of the shell. Debug records are written only while diagnostic mode is on, as
//! `Logger.debug` does in `electron/logging/Logger.ts`. Warnings and errors always pass.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use noct_db::diagnostics::{Diagnostic, DiagnosticSink, Level};

#[derive(Debug)]
pub struct DiagnosticGate {
    enabled: AtomicBool,
    sink: Arc<dyn DiagnosticSink>,
}

impl DiagnosticGate {
    pub fn new(sink: Arc<dyn DiagnosticSink>, enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            sink,
        }
    }

    /// Turns debug records on or off. The change applies to the records written after it.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

impl DiagnosticSink for DiagnosticGate {
    fn record(&self, diagnostic: &Diagnostic<'_>) {
        if matches!(diagnostic.level, Level::Debug) && !self.is_enabled() {
            return;
        }
        self.sink.record(diagnostic);
    }
}
