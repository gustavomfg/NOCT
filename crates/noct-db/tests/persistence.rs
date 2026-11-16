//! The persistence helpers, checked against `tests/atomicFile.test.ts`,
//! `tests/workspaceWriteQueue.test.ts` and `tests/productUserData.test.ts`.

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use noct_db::persistence::atomic_file::write_atomic_file;
use noct_db::persistence::product_data::migrate_product_user_data;
use noct_db::persistence::write_queue::SerializedWrites;
use rusqlite::Connection;

mod common;
use common::scratch;

/// Names of the entries of `directory`, sorted.
fn entries(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("directory")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// A legacy or current database with one row, written with the given schema version.
fn database_at(directory: &Path, version: u32) {
    fs::create_dir_all(directory).expect("product directory");
    let connection = Connection::open(directory.join("nocturne.db")).expect("database");
    connection
        .execute_batch(&format!(
            "CREATE TABLE state (value TEXT NOT NULL); PRAGMA user_version = {version};"
        ))
        .expect("schema");
    connection
        .execute("INSERT INTO state(value) VALUES('preserved')", [])
        .expect("row");
}

#[test]
fn an_atomic_write_creates_the_file_and_leaves_no_temporary() {
    let root = scratch("atomic-create");
    let path = root.join("project.json");

    write_atomic_file(&path, br#"{"version":1}"#, 0o600).expect("write");

    assert_eq!(fs::read_to_string(&path).expect("read"), r#"{"version":1}"#);
    assert_eq!(entries(&root), vec!["project.json"]);
}

#[test]
fn repeated_atomic_writes_replace_the_target_without_temporaries() {
    let root = scratch("atomic-repeat");
    let path = root.join("project.json");

    for revision in 1..=3 {
        write_atomic_file(
            &path,
            format!(r#"{{"revision":{revision}}}"#).as_bytes(),
            0o600,
        )
        .expect("write");
    }

    assert_eq!(
        fs::read_to_string(&path).expect("read"),
        r#"{"revision":3}"#
    );
    assert_eq!(entries(&root), vec!["project.json"]);
}

#[test]
fn a_failed_replace_keeps_the_target_and_removes_the_temporary() {
    let root = scratch("atomic-failure");
    // A directory cannot be replaced by a file, so the rename fails after the write succeeds.
    let target = root.join("memory.md");
    fs::create_dir(&target).expect("directory in the way");
    fs::write(target.join("kept"), "x").expect("content");

    let error = write_atomic_file(&target, "conteúdo novo".as_bytes(), 0o600);

    assert!(error.is_err());
    assert_eq!(fs::read_to_string(target.join("kept")).expect("kept"), "x");
    assert_eq!(entries(&root), vec!["memory.md"]);
}

#[cfg(unix)]
#[test]
fn an_atomic_write_applies_the_restrictive_mode() {
    use std::os::unix::fs::PermissionsExt;
    let root = scratch("atomic-mode");
    let path = root.join("memory.md");

    write_atomic_file(&path, "conteúdo".as_bytes(), 0o600).expect("write");

    let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn writes_on_one_key_run_one_after_another() {
    let writes = Arc::new(SerializedWrites::new());
    let order = Arc::new(Mutex::new(Vec::<&str>::new()));
    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel::<()>();

    let first = {
        let (writes, order) = (Arc::clone(&writes), Arc::clone(&order));
        thread::spawn(move || {
            writes.run("workspace-a", || {
                order.lock().expect("order").push("first-start");
                started_sender.send(()).expect("signal");
                release_receiver.recv().expect("release");
                order.lock().expect("order").push("first-end");
                "first"
            })
        })
    };
    started_receiver.recv().expect("first started");
    let second = {
        let (writes, order) = (Arc::clone(&writes), Arc::clone(&order));
        thread::spawn(move || {
            writes.run("workspace-a", || {
                order.lock().expect("order").push("second");
                "second"
            })
        })
    };
    // Give the second operation time to run if the queue wrongly lets it pass.
    thread::sleep(Duration::from_millis(50));
    assert_eq!(*order.lock().expect("order"), vec!["first-start"]);

    release_sender.send(()).expect("release");
    assert_eq!(first.join().expect("first"), "first");
    assert_eq!(second.join().expect("second"), "second");
    assert_eq!(
        *order.lock().expect("order"),
        vec!["first-start", "first-end", "second"]
    );
}

#[test]
fn a_failed_operation_does_not_block_the_next_one_on_its_key() {
    let writes = SerializedWrites::new();

    let failed = catch_unwind(AssertUnwindSafe(|| {
        writes.run("workspace-a", || -> &str { panic!("falha") })
    }));

    assert!(failed.is_err());
    assert_eq!(
        writes.run("workspace-a", || "after-failure"),
        "after-failure"
    );
}

#[test]
fn operations_on_different_keys_do_not_wait_for_each_other() {
    let writes = Arc::new(SerializedWrites::new());
    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel::<()>();

    let held = {
        let writes = Arc::clone(&writes);
        thread::spawn(move || {
            writes.run("workspace-b", || {
                started_sender.send(()).expect("signal");
                release_receiver.recv().expect("release");
            })
        })
    };
    started_receiver.recv().expect("held");

    let other = writes.run("workspace-c", || "c");

    assert_eq!(other, "c");
    release_sender.send(()).expect("release");
    held.join().expect("held operation");
}

#[test]
fn a_fresh_installation_uses_the_new_directory() {
    let app_data = scratch("product-fresh");

    assert_eq!(
        migrate_product_user_data(&app_data, "Nocturne Studio", "Nocturne Codex"),
        app_data.join("Nocturne Studio")
    );
}

#[test]
fn the_legacy_directory_is_renamed_when_the_new_one_is_absent() {
    let app_data = scratch("product-rename");
    let legacy = app_data.join("Nocturne Codex");
    fs::create_dir(&legacy).expect("legacy");
    fs::write(legacy.join("nocturne.db"), "database").expect("database");

    let result = migrate_product_user_data(&app_data, "Nocturne Studio", "Nocturne Codex");

    assert_eq!(result, app_data.join("Nocturne Studio"));
    assert_eq!(
        fs::read_to_string(result.join("nocturne.db")).expect("moved database"),
        "database"
    );
    assert!(!legacy.exists());
}

#[test]
fn a_legacy_database_is_not_hidden_behind_an_incomplete_new_directory() {
    let app_data = scratch("product-incomplete");
    let legacy = app_data.join("Nocturne Codex");
    fs::create_dir(&legacy).expect("legacy");
    fs::write(legacy.join("nocturne.db"), "database").expect("database");
    fs::create_dir(app_data.join("Nocturne Studio")).expect("empty new directory");

    assert_eq!(
        migrate_product_user_data(&app_data, "Nocturne Studio", "Nocturne Codex"),
        legacy
    );
}

#[test]
fn a_valid_legacy_database_is_not_hidden_behind_a_corrupt_new_one() {
    let app_data = scratch("product-corrupt");
    let current = app_data.join("Nocturne Studio");
    let legacy = app_data.join("Nocturne Codex");
    fs::create_dir(&current).expect("current");
    fs::write(current.join("nocturne.db"), "corrupt database").expect("corrupt");
    database_at(&legacy, 15);

    assert_eq!(
        migrate_product_user_data(&app_data, "Nocturne Studio", "Nocturne Codex"),
        legacy
    );
    assert_eq!(
        fs::read_to_string(current.join("nocturne.db")).expect("corrupt copy"),
        "corrupt database"
    );
}

#[test]
fn a_populated_legacy_database_beats_an_empty_one_from_an_interrupted_start() {
    let app_data = scratch("product-empty");
    let current = app_data.join("Nocturne Studio");
    let legacy = app_data.join("Nocturne Codex");
    database_at(&current, 0);
    database_at(&legacy, 15);

    assert_eq!(
        migrate_product_user_data(&app_data, "Nocturne Studio", "Nocturne Codex"),
        legacy
    );
}

#[test]
fn a_populated_new_database_is_kept_over_a_legacy_one() {
    let app_data = scratch("product-both");
    let current = app_data.join("Nocturne Studio");
    let legacy = app_data.join("Nocturne Codex");
    database_at(&current, 32);
    database_at(&legacy, 15);

    let result: PathBuf = migrate_product_user_data(&app_data, "Nocturne Studio", "Nocturne Codex");

    assert_eq!(result, current);
}
