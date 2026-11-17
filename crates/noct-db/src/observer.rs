//! Operation observer of the persistence layer.
//!
//! The TypeScript runtime measures each database operation and hands it to one observer, and
//! `electron/main.ts` turns a failed or slow operation into a warning. This module is the Rust
//! mechanism for the same contract. No operation calls it yet: the call sites come with the phase
//! that moves the repositories (phase 6). A measurement never changes the outcome of the operation:
//! an observer or a sink that panics is contained, and the result is returned as it was.

use std::fmt::Debug;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use crate::diagnostics::DiagnosticSink;

/// An operation at or above this many milliseconds is reported as slow, as in `electron/main.ts`.
pub const SLOW_OPERATION_MS: u64 = 50;

/// The event that the warning policy names when it writes to the sink.
pub const OPERATION_EVENT: &str = "persistence.operation";

/// Every operation name the TypeScript runtime reports: the reads measured in `Database.ts`, the
/// repository transactions, and the default transaction of `runInTransaction`. The integration
/// test `tests/observer.rs` checks this list against `electron/database`.
pub const CATALOG: &[&str] = &[
    "artifacts.list",
    "artifacts.page",
    "backup.importData",
    "brainMemories.create",
    "brainMemories.createCandidates",
    "brainMemories.page",
    "brainMemories.retrieve",
    "brainMemories.update",
    "changes.decision",
    "changes.replace",
    "changes.save",
    "checkpoints.complete",
    "checkpoints.replaceFiles",
    "checkpoints.save",
    "conversations.addMessage",
    "conversations.get",
    "conversations.list",
    "conversations.page",
    "engineeringIntelligence.resolveInsight",
    "engineeringIntelligence.saveEvaluation",
    "engineeringIntelligence.saveInsight",
    "executionCommands.save",
    "executions.save",
    "messages.list",
    "messages.page",
    "messages.recent",
    "modelCatalog.replaceProviderModels",
    "operations.reconcile",
    "projectIndex.markFileState",
    "projectIndex.refreshRelationHashes",
    "projectIndex.removeMissingFiles",
    "projectIndex.removeMissingForChange",
    "projectIndex.replaceExclusions",
    "projectIndex.replaceStackEvidence",
    "projectIndex.saveFileAnalysis",
    "projectIndex.updateRun",
    "projectIndex.upsertExclusions",
    "semanticIndex.deleteFile",
    "semanticIndex.markFileStale",
    "semanticIndex.replaceFileUnits",
    "semanticIndex.updateRun",
    "settings.get",
    "settings.set",
    "suggestions.add",
    "suggestions.list",
    "suggestions.page",
    "suggestions.reconcile",
    "suggestions.setStatus",
    "transaction",
    "validation.create",
    "validation.save",
    "workspaceMemory.get",
    "workspaces.list",
    "workspaces.relocate",
];

/// One measured operation, as the TypeScript observer receives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationMetric {
    pub operation: &'static str,
    pub duration_ms: u64,
    pub failed: bool,
}

/// Receives every measured operation. It must not change the database. A panic in it is contained
/// by the measurement, so it never reaches the caller of the operation.
pub trait OperationObserver: Send + Sync + Debug {
    fn observe(&self, metric: &OperationMetric);
}

/// The warning policy of `electron/main.ts`: a failed operation, or one that takes at least
/// [`SLOW_OPERATION_MS`], is written to the sink. The others are not warned. The shell applies the same policy to
/// its own operations, so the two warn alike.
pub fn warn_on_attention(metric: &OperationMetric, sink: &dyn DiagnosticSink) {
    if metric.failed || metric.duration_ms >= SLOW_OPERATION_MS {
        sink.warn(
            OPERATION_EVENT,
            &format!(
                "{} took {} ms (failed: {})",
                metric.operation, metric.duration_ms, metric.failed
            ),
        );
    }
}

/// Milliseconds rounded to the nearest, as `Math.round` in the TypeScript runtime.
pub(crate) fn milliseconds(elapsed: Duration) -> u64 {
    (elapsed.as_secs_f64() * 1000.0).round() as u64
}

/// Reports one operation when it ends. The report is written on drop, so a panicking callback is
/// reported too, as a failure.
pub(crate) struct Measurement<'a> {
    operation: &'static str,
    started: Instant,
    /// Starts as `true`: a callback that panics never sets it, so the report says it failed.
    pub(crate) failed: bool,
    observer: Option<&'a dyn OperationObserver>,
    sink: &'a dyn DiagnosticSink,
}

impl<'a> Measurement<'a> {
    pub(crate) fn start(
        operation: &'static str,
        observer: Option<&'a dyn OperationObserver>,
        sink: &'a dyn DiagnosticSink,
    ) -> Self {
        Self {
            operation,
            started: Instant::now(),
            failed: true,
            observer,
            sink,
        }
    }
}

impl Drop for Measurement<'_> {
    fn drop(&mut self) {
        let metric = OperationMetric {
            operation: self.operation,
            duration_ms: milliseconds(self.started.elapsed()),
            failed: self.failed,
        };
        // Both calls run under `catch_unwind`: unwinding out of a drop during a panic would abort.
        if let Some(observer) = self.observer {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.observe(&metric)));
        }
        let _ = catch_unwind(AssertUnwindSafe(|| warn_on_attention(&metric, self.sink)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn milliseconds_round_to_the_nearest_like_the_typescript_runtime() {
        assert_eq!(milliseconds(Duration::from_micros(1_499)), 1);
        assert_eq!(milliseconds(Duration::from_micros(1_500)), 2);
        assert_eq!(milliseconds(Duration::ZERO), 0);
    }
}
