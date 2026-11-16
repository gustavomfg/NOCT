//! The application log file, checked against the format and the redaction of `electron/logging/Logger.ts`:
//! one JSON object per line, private to the user, redacted before it is written, and no record that waits
//! for the disk.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use noct_db::diagnostics::{Diagnostic, DiagnosticSink, Level};
use noct_db::file_log::{FileLogSink, LOG_FILE_NAME};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

mod common;
use common::scratch;

fn read_entries(directory: &Path) -> Vec<Value> {
    fs::read_to_string(directory.join(LOG_FILE_NAME))
        .expect("the log file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is JSON"))
        .collect()
}

#[test]
fn each_record_is_one_json_line_with_the_typescript_fields() {
    let directory = scratch("file-log-fields");
    let sink = FileLogSink::open(&directory).expect("open");
    let session = sink.session_id().to_string();
    let data = json!({ "count": 2 });

    sink.record(&Diagnostic {
        level: Level::Warn,
        category: "persistence.statistics",
        message: "falhou",
        data: Some(&data),
    });
    drop(sink);

    let entries = read_entries(&directory);
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    let keys: Vec<&str> = entry
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "timestamp",
            "sessionId",
            "level",
            "category",
            "message",
            "data"
        ]
    );
    assert_eq!(entry["level"], "warn");
    assert_eq!(entry["category"], "persistence.statistics");
    assert_eq!(entry["message"], "falhou");
    assert_eq!(entry["data"], json!({ "count": 2 }));
    assert_eq!(entry["sessionId"], session.as_str());
    assert!(Uuid::parse_str(&session).is_ok(), "{session}");
    // `toISOString` form: milliseconds and a `Z`, as in `2026-11-16T12:34:56.789Z`.
    let timestamp = entry["timestamp"].as_str().expect("a timestamp");
    assert_eq!(timestamp.len(), 24, "{timestamp}");
    OffsetDateTime::parse(timestamp, &Rfc3339).expect("RFC 3339");
}

#[test]
fn secrets_and_private_fields_never_reach_the_file() {
    let directory = scratch("file-log-redaction");
    let sink = FileLogSink::open(&directory).expect("open");
    let data = json!({
        "api_key": "chave-privada",
        "prompt": "pedido privado",
        "ok": true,
        "nested": { "password": "senha-privada", "safe": "ok" }
    });

    sink.record(&Diagnostic {
        level: Level::Error,
        category: "persistence.test",
        message: "Authorization: Bearer segredo-super-secreto",
        data: Some(&data),
    });
    drop(sink);

    let text = fs::read_to_string(directory.join(LOG_FILE_NAME)).expect("the log file");
    for secret in [
        "segredo-super-secreto",
        "chave-privada",
        "pedido privado",
        "senha-privada",
    ] {
        assert!(!text.contains(secret), "{secret} reached the file");
    }
    let entries = read_entries(&directory);
    assert_eq!(entries[0]["message"], "Authorization: Bearer [REDACTED]");
    assert_eq!(
        entries[0]["data"],
        json!({ "ok": true, "nested": { "safe": "ok" } })
    );
}

/// The way the store receives it: `OpenOptions::diagnostics` is an `Arc<dyn DiagnosticSink>`, so the sink
/// must be `Send + Sync` for the store to share it with its background work.
#[test]
fn the_sink_is_shared_through_the_diagnostic_trait_object() {
    let directory = scratch("file-log-shared");
    let sink: Arc<dyn DiagnosticSink> = Arc::new(FileLogSink::open(&directory).expect("open"));
    let other = Arc::clone(&sink);

    other.warn("persistence.test", "de outra ponta");
    drop(other);
    drop(sink);

    let entries = read_entries(&directory);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["message"], "de outra ponta");
}

#[test]
fn a_record_without_data_has_no_data_field() {
    let directory = scratch("file-log-no-data");
    let sink = FileLogSink::open(&directory).expect("open");

    sink.warn("persistence.test", "sem dados");
    drop(sink);

    let entries = read_entries(&directory);
    assert_eq!(entries.len(), 1);
    assert!(entries[0].get("data").is_none());
}

#[test]
fn reopening_appends_to_the_same_file_under_a_new_session() {
    let directory = scratch("file-log-reopen");

    let first = FileLogSink::open(&directory).expect("first open");
    let first_session = first.session_id().to_string();
    first.warn("persistence.test", "primeiro");
    drop(first);

    let second = FileLogSink::open(&directory).expect("second open");
    let second_session = second.session_id().to_string();
    second.warn("persistence.test", "segundo");
    drop(second);

    let entries = read_entries(&directory);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["message"], "primeiro");
    assert_eq!(entries[0]["sessionId"], first_session.as_str());
    assert_eq!(entries[1]["message"], "segundo");
    assert_eq!(entries[1]["sessionId"], second_session.as_str());
    assert_ne!(first_session, second_session);
}

#[test]
fn dropping_the_sink_writes_every_queued_record_in_order() {
    let directory = scratch("file-log-drain");
    let sink = FileLogSink::open(&directory).expect("open");

    for index in 0..200 {
        sink.warn("persistence.test", &format!("registro {index}"));
    }
    // The queue holds more than 200 records, so none is dropped.
    assert_eq!(sink.dropped_records(), 0);
    drop(sink);

    let entries = read_entries(&directory);
    assert_eq!(entries.len(), 200);
    assert_eq!(entries[0]["message"], "registro 0");
    assert_eq!(entries[199]["message"], "registro 199");
}

#[test]
fn opening_where_the_directory_cannot_exist_is_reported() {
    let directory = scratch("file-log-blocked");
    let blocker = directory.join("a-file");
    fs::write(&blocker, b"not a directory").expect("a file in the way");

    assert!(FileLogSink::open(&blocker.join("logs")).is_err());
}

#[cfg(unix)]
#[test]
fn the_log_file_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;

    let directory = scratch("file-log-mode");
    let sink = FileLogSink::open(&directory).expect("open");
    sink.warn("persistence.test", "privado");
    drop(sink);

    let mode = fs::metadata(directory.join(LOG_FILE_NAME))
        .expect("the log file")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}
