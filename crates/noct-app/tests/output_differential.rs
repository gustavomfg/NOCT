//! Output differential of the first slice (ADR 0001): the four commands must return in Rust what the
//! TypeScript application returns, for the database that each of the nine release lines leaves.
//!
//! The bases and the TypeScript outputs come from `scripts/generate-output-differential.mjs`. Run it first,
//! then the test:
//!
//!   cargo test -p noct-app --test output_differential -- --ignored
//!
//! The test is ignored by default, as the row gate is, because it needs those bases. Without them it fails:
//! nothing is skipped silently. Two readings are compared for each base: the legacy file, which Rust
//! upgrades when it opens it, against the reading TypeScript takes after its own upgrade (`base`); and the
//! copy that the TypeScript repositories wrote values into, against its reading (`written`).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::Scratch;
use noct_app::bootstrap;
use noct_app::services;
use noct_app::workspace_scope::default_blocked_roots;
use noct_core::contracts::Conversation;
use noct_core::ipc::conversations::{ConversationPageRequest, PAGE_LIMIT_MAX};
use noct_db::store::Store;
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

fn read_json(file: &Path) -> Value {
    let text = fs::read_to_string(file).unwrap_or_else(|_| {
        panic!(
            "{} is missing: run node scripts/generate-output-differential.mjs",
            file.display()
        )
    });
    serde_json::from_str(&text).expect("the file is JSON")
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

/// A scratch copy of the database files in `from`, which the test may open and change.
fn copy_database(from: &Path, tag: &str, reading: &str) -> Scratch {
    let scratch = Scratch::new(&format!("differential-{reading}-{tag}"));
    for name in DATABASE_FILES {
        let source = from.join(name);
        if source.exists() {
            fs::copy(&source, scratch.path().join(name)).expect("the database is copied");
        }
    }
    assert!(
        scratch.path().join("nocturne.db").is_file(),
        "the {reading} database of {tag} is missing in {}",
        from.display()
    );
    scratch
}

/// Every conversation of the store, read through the pages of `limit` rows that `conversations_page` returns.
/// Each page must report a `nextCursor` exactly when it reports `hasMore`, so the walk ends on a page
/// without one. The concatenation of the pages is what the TypeScript list is compared with.
fn conversations_through_pages(store: &Store, limit: usize) -> Vec<Conversation> {
    /// A walk that does not end is a cursor that does not advance. The bases hold a few dozen rows at most.
    const MAX_PAGES: usize = 10_000;
    let mut collected = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_PAGES {
        let request = ConversationPageRequest {
            limit,
            cursor: cursor.clone(),
        };
        let page =
            services::conversations_page(store, &request).expect("a page of conversations is read");
        assert!(page.items.len() <= limit, "a page holds at most its limit");
        assert_eq!(
            page.next_cursor.is_some(),
            page.has_more,
            "nextCursor is present exactly when hasMore is"
        );
        collected.extend(page.items);
        if !page.has_more {
            return collected;
        }
        cursor = page.next_cursor;
    }
    panic!("the pages did not end within {MAX_PAGES} pages");
}

/// The four outputs of the first slice for the database in `scratch`, as the commands return them. Opening
/// the store upgrades a database of an older schema, as it does for the application. The conversations are
/// read in pages of one row and of the maximum, and the two walks must agree before they are compared with
/// TypeScript.
fn rust_outputs(scratch: &Scratch) -> Value {
    let persistence = bootstrap::open(scratch.path()).expect("the shell opens the copy");
    let store = persistence.store.lock().expect("the store is not poisoned");
    let settings = services::settings_get(&store).expect("the settings are read");
    let rows = services::workspace_rows(&store).expect("the workspaces are read");
    let workspaces = services::workspaces_from_rows(rows, &default_blocked_roots());
    let one_by_one = conversations_through_pages(&store, 1);
    let by_maximum = conversations_through_pages(
        &store,
        usize::try_from(PAGE_LIMIT_MAX).expect("the page maximum fits in usize"),
    );
    let one_by_one = serde_json::to_value(one_by_one).expect("the conversations serialize");
    assert_eq!(
        one_by_one,
        serde_json::to_value(by_maximum).expect("the conversations serialize"),
        "pages of one and pages of the maximum return the same conversations"
    );
    json!({
        "settings": serde_json::to_value(settings).expect("the settings serialize"),
        "workspaces": serde_json::to_value(workspaces).expect("the workspaces serialize"),
        "conversations": one_by_one,
    })
}

#[test]
#[ignore = "needs the release-line bases; run node scripts/generate-output-differential.mjs"]
fn every_release_line_returns_the_same_outputs_in_rust_as_in_typescript() {
    let gate = gate_directory();
    let expected = read_json(&gate.join("expected.json"));
    let manifest = read_json(&gate.join("release-lines").join("manifest.json"));

    let tags: Vec<&str> = expected["tags"]
        .as_array()
        .expect("the tags are a list")
        .iter()
        .map(|tag| tag.as_str().expect("a tag is a string"))
        .collect();
    assert_eq!(tags, TAGS, "the bases are the nine release lines");

    let lines = expected["lines"].as_array().expect("the lines are a list");
    let legacy_files = manifest["lines"]
        .as_array()
        .expect("the manifest lists the bases");
    assert_eq!(lines.len(), TAGS.len(), "one line per base");
    assert_eq!(legacy_files.len(), TAGS.len(), "one legacy base per line");

    let gate_text = fs::canonicalize(&gate)
        .expect("the gate directory exists")
        .to_string_lossy()
        .into_owned();
    for ((line, legacy), tag) in lines.iter().zip(legacy_files).zip(TAGS) {
        assert_eq!(line["tag"], tag, "lines follow the order of the bases");
        assert_eq!(legacy["tag"], tag, "the manifest follows the same order");

        let legacy_directory = gate
            .join(legacy["legacy"].as_str().expect("the legacy path"))
            .parent()
            .expect("the legacy file has a folder")
            .to_path_buf();
        let upgraded = copy_database(&legacy_directory, tag, "legacy");
        assert_eq!(
            relocate(rust_outputs(&upgraded), &gate_text),
            line["base"],
            "{tag}: the legacy file, upgraded in Rust, reads differently from TypeScript"
        );

        let written = copy_database(&gate.join("read").join(tag), tag, "written");
        assert_eq!(
            relocate(rust_outputs(&written), &gate_text),
            line["written"],
            "{tag}: the copy written by TypeScript reads differently in Rust"
        );
    }
}
