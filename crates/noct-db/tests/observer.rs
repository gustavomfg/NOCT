//! The operation observer, checked against `electron/database` and `electron/main.ts`: the catalog
//! names what the TypeScript runtime reports, the warning policy is the one of the main process, and
//! no observer can change the result of an operation or break the database.

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use noct_db::Error;
use noct_db::diagnostics::{Diagnostic, DiagnosticSink};
use noct_db::observer::{
    CATALOG, OPERATION_EVENT, OperationMetric, OperationObserver, SLOW_OPERATION_MS,
};
use noct_db::runtime::{Database, OpenOptions};
use serde_json::Value;

mod common;
use common::scratch;

#[derive(Debug, Default)]
struct Recorder(Mutex<Vec<OperationMetric>>);

impl OperationObserver for Recorder {
    fn observe(&self, metric: &OperationMetric) {
        self.0.lock().expect("recorder").push(metric.clone());
    }
}

impl Recorder {
    fn metrics(&self) -> Vec<OperationMetric> {
        self.0.lock().expect("recorder").clone()
    }
}

#[derive(Debug)]
struct Panicking;

impl OperationObserver for Panicking {
    fn observe(&self, _metric: &OperationMetric) {
        panic!("the observer fails");
    }
}

#[derive(Debug, Default)]
struct Warnings(Mutex<Vec<(String, String)>>);

impl DiagnosticSink for Warnings {
    fn record(&self, diagnostic: &Diagnostic<'_>) {
        self.0.lock().expect("warnings").push((
            diagnostic.category.to_string(),
            diagnostic.message.to_string(),
        ));
    }
}

impl Warnings {
    /// The warnings of the operation policy only, not the ones the opening may write.
    fn operations(&self) -> Vec<String> {
        self.0
            .lock()
            .expect("warnings")
            .iter()
            .filter(|(event, _)| event == OPERATION_EVENT)
            .map(|(_, message)| message.clone())
            .collect()
    }
}

fn open(
    name: &str,
    observer: Option<Arc<dyn OperationObserver>>,
    warnings: Arc<Warnings>,
) -> Database {
    let directory = scratch(name);
    Database::open_runtime_with(
        &directory,
        OpenOptions {
            diagnostics: warnings,
            operation_observer: observer,
            ..OpenOptions::default()
        },
    )
    .expect("open the database")
}

/// The operation names frozen from the TypeScript reference (`electron-final`), read from the JSON fixture.
fn frozen_names(list: &Value) -> BTreeSet<String> {
    list.as_array()
        .expect("the fixture lists names")
        .iter()
        .map(|name| name.as_str().expect("a name is a string").to_string())
        .collect()
}

#[test]
fn the_catalog_names_what_the_typescript_runtime_reports() {
    // The names were read from electron/database at `electron-final` (c8c378b) and frozen in the fixture, so the
    // comparison no longer needs the TypeScript sources on this branch.
    let frozen: Value =
        serde_json::from_str(include_str!("fixtures/typescript-observer-names.json"))
            .expect("the observer fixture is JSON");
    let reads = frozen_names(&frozen["reads"]);
    let transactions = frozen_names(&frozen["transactions"]);
    assert!(!reads.is_empty(), "the fixture has no measured reads");
    assert!(
        !transactions.is_empty(),
        "the fixture has no repository transactions"
    );
    assert_eq!(
        frozen["defaultTransactionOperation"], "transaction",
        "the default transaction name in the fixture"
    );

    let mut expected: BTreeSet<String> = reads.union(&transactions).cloned().collect();
    expected.insert("transaction".to_string());
    let catalog: BTreeSet<&str> = CATALOG.iter().copied().collect();
    assert_eq!(catalog.len(), CATALOG.len(), "the catalog repeats a name");
    let expected: BTreeSet<&str> = expected.iter().map(String::as_str).collect();
    assert_eq!(
        catalog, expected,
        "the catalog differs from the operations frozen from electron/database"
    );
}

#[test]
fn a_failed_or_slow_operation_is_warned_and_a_fast_one_is_not() {
    let recorder = Arc::new(Recorder::default());
    let warnings = Arc::new(Warnings::default());
    let database = open(
        "observer-policy",
        Some(recorder.clone()),
        Arc::clone(&warnings),
    );

    database
        .measure("settings.get", || Ok(()))
        .expect("the fast operation");
    database
        .measure("settings.get", || {
            std::thread::sleep(Duration::from_millis(SLOW_OPERATION_MS + 20));
            Ok(())
        })
        .expect("the slow operation");
    let failure: noct_db::Result<()> =
        database.measure("settings.get", || Err(Error::Message("a falha".into())));
    assert!(failure.is_err(), "the failure reaches the caller");

    let metrics = recorder.metrics();
    assert_eq!(metrics.len(), 3, "every operation is observed");
    assert!(!metrics[0].failed);
    assert!(metrics[1].duration_ms >= SLOW_OPERATION_MS);
    assert!(metrics[2].failed);
    assert!(
        metrics
            .iter()
            .all(|metric| metric.operation == "settings.get")
    );

    let operations = warnings.operations();
    assert_eq!(
        operations.len(),
        2,
        "only the slow and the failed operation: {operations:?}"
    );
    assert!(
        operations[0].contains("settings.get took"),
        "{operations:?}"
    );
    assert!(operations[1].contains("failed: true"), "{operations:?}");
    database.close().expect("close");
}

#[test]
fn a_panicking_operation_is_reported_as_failed_and_the_panic_propagates() {
    let recorder = Arc::new(Recorder::default());
    let database = open("observer-panic", Some(recorder.clone()), Arc::default());

    let outcome = catch_unwind(AssertUnwindSafe(|| {
        database.measure("settings.set", || -> noct_db::Result<()> {
            panic!("the operation panics")
        })
    }));
    assert!(outcome.is_err(), "the panic reaches the caller");

    let metrics = recorder.metrics();
    assert_eq!(metrics.len(), 1, "the panicking operation is observed");
    assert!(metrics[0].failed, "and it is reported as failed");
    database.close().expect("close");
}

#[test]
fn an_observer_that_panics_changes_neither_the_result_nor_the_database() {
    let database = open(
        "observer-contained",
        Some(Arc::new(Panicking)),
        Arc::default(),
    );

    for _ in 0..3 {
        let value = database
            .measure("settings.get", || Ok(42))
            .expect("the result is kept");
        assert_eq!(value, 42);
    }
    let rows: i64 = database
        .connection()
        .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
        .expect("the database still answers");
    assert!(rows >= 0);
    database.close().expect("close");
}

#[test]
fn operations_without_an_observer_are_still_warned_when_they_fail() {
    let warnings = Arc::new(Warnings::default());
    let database = open("observer-default", None, Arc::clone(&warnings));

    let failure: noct_db::Result<()> =
        database.measure("workspaces.list", || Err(Error::Message("falha".into())));
    assert!(failure.is_err());
    assert_eq!(warnings.operations().len(), 1);
    database.close().expect("close");
}
