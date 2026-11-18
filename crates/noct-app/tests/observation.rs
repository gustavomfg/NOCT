//! The shell's measurements. A measured command reports its catalog name, its duration and whether it failed, and a
//! panic counts as a failure. The database's warning policy writes a failed operation and a slow one, with its name
//! and duration and nothing else: no path, no value and no error message.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use noct_app::logging::{ShellObserver, measured};
use noct_db::diagnostics::{Diagnostic, DiagnosticSink};
use noct_db::observer::{CATALOG, OperationMetric, OperationObserver, SLOW_OPERATION_MS};

#[derive(Debug, Default)]
struct Recorder {
    metrics: Mutex<Vec<OperationMetric>>,
}

impl OperationObserver for Recorder {
    fn observe(&self, metric: &OperationMetric) {
        self.metrics
            .lock()
            .expect("the recorder is not poisoned")
            .push(metric.clone());
    }
}

/// One record of a sink: its level, its category and its message.
#[derive(Debug, Default)]
struct Lines {
    records: Mutex<Vec<(String, String, String)>>,
}

impl DiagnosticSink for Lines {
    fn record(&self, diagnostic: &Diagnostic<'_>) {
        self.records
            .lock()
            .expect("the lines are not poisoned")
            .push((
                diagnostic.level.as_str().to_string(),
                diagnostic.category.to_string(),
                diagnostic.message.to_string(),
            ));
    }
}

#[test]
fn a_measured_success_reports_its_name_and_no_failure() {
    let recorder = Recorder::default();

    let value = measured(&recorder, "settings.get", || Ok::<_, ()>(7));

    assert_eq!(value, Ok(7));
    let metrics = recorder.metrics.lock().expect("not poisoned");
    assert_eq!(metrics.len(), 1, "one operation is measured");
    assert_eq!(metrics[0].operation, "settings.get");
    assert!(!metrics[0].failed);
}

#[test]
fn a_failing_callback_is_reported_as_failed_and_its_error_is_returned() {
    let recorder = Recorder::default();

    let result = measured(&recorder, "workspaces.list", || Err::<(), _>("refused"));

    assert_eq!(result, Err("refused"), "the error is returned unchanged");
    assert!(recorder.metrics.lock().expect("not poisoned")[0].failed);
}

#[test]
fn a_panic_is_reported_as_a_failure_and_still_propagates() {
    let recorder = Recorder::default();

    let outcome = catch_unwind(AssertUnwindSafe(|| {
        measured(&recorder, "conversations.page", || -> Result<(), ()> {
            panic!("the callback fails")
        })
    }));

    assert!(outcome.is_err(), "the panic propagates");
    let metrics = recorder.metrics.lock().expect("not poisoned");
    assert_eq!(metrics.len(), 1, "the panic is still reported");
    assert!(metrics[0].failed, "a panic counts as a failure");
}

#[test]
fn the_shell_measures_only_names_the_database_catalog_has() {
    for name in [
        "settings.get",
        "settings.set",
        "workspaces.list",
        "conversations.page",
    ] {
        assert!(CATALOG.contains(&name), "{name} is in the catalog");
    }
}

#[test]
fn a_failed_command_is_warned_with_its_name_and_nothing_else() {
    let lines = Arc::new(Lines::default());
    let observer = ShellObserver::new(lines.clone());

    observer.observe(&OperationMetric {
        operation: "workspaces.list",
        duration_ms: 3,
        failed: true,
    });

    let records = lines.records.lock().expect("not poisoned");
    assert_eq!(records.len(), 1, "a failure is written once");
    let (level, category, message) = &records[0];
    assert_eq!(level, "warn");
    assert_eq!(category, "persistence.operation");
    assert!(
        message.contains("workspaces.list"),
        "the message names the operation"
    );
    assert!(message.contains("failed: true"));
    assert!(!message.contains('/'), "the message has no path: {message}");
}

#[test]
fn a_slow_command_is_warned_and_a_quick_one_is_not() {
    let lines = Arc::new(Lines::default());
    let observer = ShellObserver::new(lines.clone());

    observer.observe(&OperationMetric {
        operation: "settings.get",
        duration_ms: SLOW_OPERATION_MS - 1,
        failed: false,
    });
    assert!(lines.records.lock().expect("not poisoned").is_empty());

    observer.observe(&OperationMetric {
        operation: "settings.get",
        duration_ms: SLOW_OPERATION_MS,
        failed: false,
    });
    assert_eq!(lines.records.lock().expect("not poisoned").len(), 1);
}
