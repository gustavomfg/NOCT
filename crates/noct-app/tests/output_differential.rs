//! Output differential of the first slice (ADR 0001): the four commands must return in Rust what the
//! TypeScript application returns, for the database that each of the nine release lines leaves.
//!
//! The bases and the TypeScript outputs come from `scripts/generate-output-differential.mjs`. Run it first,
//! then the test:
//!
//!   cargo test -p noct-app --test output_differential -- --ignored
//!
//! The test is ignored by default, as the row gate is, because it needs those bases. Without them it fails:
//! nothing is skipped silently.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::Scratch;
use noct_app::bootstrap;
use noct_app::services;
use noct_app::workspace_scope::default_blocked_roots;
use serde_json::{Value, json};

/// The nine tagged release lines of the migration, from 0.5.0-beta to 1.0.2. The current line is 1.0.5 and
/// it is not a base: its database is the one the reference upgrades.
const TAGS: [&str; 9] = [
    "v0.5.0-beta",
    "v0.6.0-beta",
    "v0.7.0-beta",
    "v0.8.0-beta",
    "v0.9.0-beta",
    "v0.9.5-beta",
    "v1.0.0",
    "v1.0.1",
    "v1.0.2",
];

const DATABASE_FILES: [&str; 3] = ["nocturne.db", "nocturne.db-wal", "nocturne.db-shm"];

fn gate_directory() -> PathBuf {
    match std::env::var_os("NOCT_OUTPUT_DIFFERENTIAL_DIR") {
        Some(value) => PathBuf::from(value),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/output-differential"),
    }
}

fn read_expected(gate: &Path) -> Value {
    let file = gate.join("expected.json");
    let text = fs::read_to_string(&file).unwrap_or_else(|_| {
        panic!(
            "{} is missing: run node scripts/generate-output-differential.mjs",
            file.display()
        )
    });
    serde_json::from_str(&text).expect("expected.json is JSON")
}

/// Replaces the gate directory by `<gate>` in every string, as the generator does.
fn relocate(value: Value, gate: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(gate, "<gate>")),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|item| relocate(item, gate)).collect())
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, item)| (key, relocate(item, gate)))
                .collect(),
        ),
        other => other,
    }
}

/// A copy of the database the TypeScript side read, in a scratch directory of this test.
fn copy_of_base(gate: &Path, tag: &str) -> Scratch {
    let source = gate.join("read").join(tag);
    let scratch = Scratch::new(&format!("differential-{tag}"));
    for name in DATABASE_FILES {
        let from = source.join(name);
        if from.exists() {
            fs::copy(&from, scratch.path().join(name)).expect("the database is copied");
        }
    }
    assert!(
        scratch.path().join("nocturne.db").is_file(),
        "the base of {tag} is missing in {}",
        source.display()
    );
    scratch
}

/// The four outputs of the first slice for the database in `scratch`, as the commands return them.
fn rust_outputs(scratch: &Scratch) -> Value {
    let persistence = bootstrap::open(scratch.path()).expect("the shell opens the copy");
    let store = persistence.store.lock().expect("the store is not poisoned");
    let settings = services::settings_get(&store).expect("the settings are read");
    let rows = services::workspace_rows(&store).expect("the workspaces are read");
    let workspaces = services::workspaces_from_rows(rows, &default_blocked_roots());
    let conversations = services::conversations_list(&store).expect("the conversations are read");
    json!({
        "settings": serde_json::to_value(settings).expect("the settings serialize"),
        "workspaces": serde_json::to_value(workspaces).expect("the workspaces serialize"),
        "conversations": serde_json::to_value(conversations).expect("the conversations serialize"),
    })
}

#[test]
#[ignore = "needs the release-line bases; run node scripts/generate-output-differential.mjs"]
fn every_release_line_returns_the_same_outputs_in_rust_as_in_typescript() {
    let gate = gate_directory();
    let expected = read_expected(&gate);

    let tags: Vec<&str> = expected["tags"]
        .as_array()
        .expect("the tags are a list")
        .iter()
        .map(|tag| tag.as_str().expect("a tag is a string"))
        .collect();
    assert_eq!(tags, TAGS, "the bases are the nine release lines");

    let lines = expected["lines"].as_array().expect("the lines are a list");
    assert_eq!(lines.len(), TAGS.len(), "one line per base");

    let gate_text = fs::canonicalize(&gate)
        .expect("the gate directory exists")
        .to_string_lossy()
        .into_owned();
    for (line, tag) in lines.iter().zip(TAGS) {
        assert_eq!(line["tag"], tag, "lines follow the order of the bases");
        let scratch = copy_of_base(&gate, tag);
        let actual = relocate(rust_outputs(&scratch), &gate_text);
        let mut wanted = line.clone();
        wanted
            .as_object_mut()
            .expect("a line is an object")
            .remove("tag");
        assert_eq!(actual, wanted, "{tag}: Rust and TypeScript outputs differ");
    }
}
