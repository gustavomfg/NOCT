//! The options of `Database::open_with`: the injected diagnostic sink receives the warnings of the database.

use std::sync::{Arc, Mutex};

use noct_db::diagnostics::DiagnosticSink;
use noct_db::{Database, OpenOptions};

mod common;
use common::scratch;

/// Keeps the warnings it receives, so a test can check what was logged.
#[derive(Debug, Default)]
struct Recording(Mutex<Vec<String>>);

impl DiagnosticSink for Recording {
    fn warn(&self, event: &'static str, message: &str) {
        self.0
            .lock()
            .expect("recording lock")
            .push(format!("{event}: {message}"));
    }
}

#[test]
fn the_database_reports_to_the_injected_sink() {
    let directory = scratch("open-options-sink");
    let recording = Arc::new(Recording::default());
    let database = Database::open_with(
        &directory,
        OpenOptions {
            diagnostics: recording.clone(),
            ..OpenOptions::default()
        },
    )
    .expect("open");

    database.diagnostics().warn("test.event", "a warning");

    let entries = recording.0.lock().expect("recording lock").clone();
    assert_eq!(entries, vec!["test.event: a warning".to_string()]);
}
