//! The diagnostic gate: debug records follow the diagnostic mode, the other levels always pass.

use std::sync::{Arc, Mutex};

use noct_app::logging::DiagnosticGate;
use noct_db::diagnostics::{Diagnostic, DiagnosticSink, Level};

#[derive(Debug, Default)]
struct Recorder(Mutex<Vec<&'static str>>);

impl DiagnosticSink for Recorder {
    fn record(&self, diagnostic: &Diagnostic<'_>) {
        self.0
            .lock()
            .expect("the recorder is not poisoned")
            .push(diagnostic.category);
    }
}

fn emit(gate: &DiagnosticGate, level: Level, category: &'static str) {
    gate.record(&Diagnostic {
        level,
        category,
        message: "message",
        data: None,
    });
}

fn gate(enabled: bool) -> (DiagnosticGate, Arc<Recorder>) {
    let recorder = Arc::new(Recorder::default());
    let gate = DiagnosticGate::new(recorder.clone(), enabled);
    (gate, recorder)
}

#[test]
fn debug_records_are_dropped_while_diagnostic_mode_is_off() {
    let (gate, recorder) = gate(false);

    emit(&gate, Level::Debug, "debug");
    emit(&gate, Level::Info, "info");
    emit(&gate, Level::Warn, "warn");
    emit(&gate, Level::Error, "error");

    assert_eq!(
        *recorder.0.lock().expect("not poisoned"),
        ["info", "warn", "error"]
    );
}

#[test]
fn turning_diagnostic_mode_on_and_off_applies_to_the_next_records() {
    let (gate, recorder) = gate(false);

    gate.set_enabled(true);
    emit(&gate, Level::Debug, "first");
    gate.set_enabled(false);
    emit(&gate, Level::Debug, "second");

    assert_eq!(*recorder.0.lock().expect("not poisoned"), ["first"]);
}
