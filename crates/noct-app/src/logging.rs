//! The diagnostic sink of the shell. Debug records are written only while diagnostic mode is on, as
//! `Logger.debug` does in `electron/logging/Logger.ts`. Warnings and errors always pass.
//!
//! The shell also measures its commands. Each one reports its name, its duration and whether it failed, and the
//! database's warning policy decides what is written. A warning names the operation and its duration, and nothing
//! else: no path, no value and no message of the error.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use noct_db::diagnostics::{Diagnostic, DiagnosticSink, Level};
use noct_db::observer::{OperationMetric, OperationObserver, warn_on_attention};

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

/// The observer of the shell's commands. It applies the database's warning policy to each measured operation.
#[derive(Debug)]
pub struct ShellObserver {
    sink: Arc<dyn DiagnosticSink>,
}

impl ShellObserver {
    pub fn new(sink: Arc<dyn DiagnosticSink>) -> Self {
        Self { sink }
    }
}

impl OperationObserver for ShellObserver {
    fn observe(&self, metric: &OperationMetric) {
        warn_on_attention(metric, self.sink.as_ref());
    }
}

/// Runs `callback` as the named operation and reports it to `observer`, once the callback has ended. A panic in
/// the callback counts as a failure, as the database's measurement counts it. The result is returned unchanged.
pub fn measured<T, E>(
    observer: &dyn OperationObserver,
    operation: &'static str,
    callback: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    /// Reports on drop, so a panic that unwinds through the callback is still reported, as a failure.
    struct Report<'a> {
        observer: &'a dyn OperationObserver,
        operation: &'static str,
        started: Instant,
        failed: bool,
    }

    impl Drop for Report<'_> {
        fn drop(&mut self) {
            let duration_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
            self.observer.observe(&OperationMetric {
                operation: self.operation,
                duration_ms,
                failed: self.failed,
            });
        }
    }

    let mut report = Report {
        observer,
        operation,
        started: Instant::now(),
        failed: true,
    };
    let result = callback();
    report.failed = result.is_err();
    result
}
