//! Database recovery, checked against `tests/databaseRecovery.test.ts`: the artifacts that are
//! detected, the corruption that is recoverable, the copies that are offered and the quarantine
//! and rollback of a restore.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use noct_db::Error;
use noct_db::recovery::{
    has_recovery_artifacts, inspect_database_file, is_recoverable_corruption,
    list_recovery_candidates, restore_database_file,
};
use rusqlite::{Connection, OpenFlags};

mod common;
use common::scratch;

const REFUSED: &str = "A recuperação do banco falhou; o arquivo original foi preservado";

/// A database with one row, written with the given schema version.
fn database_at(path: &Path, value: &str, version: u32) {
    let connection = Connection::open(path).expect("create database");
    connection
        .execute_batch(&format!(
            "CREATE TABLE state (value TEXT NOT NULL); PRAGMA user_version = {version};"
        ))
        .expect("schema");
    connection
        .execute("INSERT INTO state(value) VALUES(?1)", [value])
        .expect("row");
}

fn stored_value(path: &Path) -> String {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("open read-only");
    connection
        .query_row("SELECT value FROM state", [], |row| row.get(0))
        .expect("stored value")
}

fn set_modified(path: &Path, seconds_after_epoch: u64) {
    let file = File::options()
        .write(true)
        .open(path)
        .expect("open for the time");
    file.set_modified(UNIX_EPOCH + Duration::from_secs(seconds_after_epoch))
        .expect("set the modification time");
}

fn names_starting_with(directory: &Path, prefix: &str) -> Vec<String> {
    fs::read_dir(directory)
        .expect("directory")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.starts_with(prefix))
        .collect()
}

#[test]
fn recovery_artifacts_are_found_by_their_names() {
    let root = scratch("recovery-artifacts");
    assert!(!has_recovery_artifacts(&root));

    fs::write(root.join("nocturne.db.backup-kept"), "not an artifact").expect("file");
    assert!(!has_recovery_artifacts(&root));

    fs::create_dir(root.join("database-corrupt-interrupted")).expect("quarantine");
    assert!(has_recovery_artifacts(&root));
    fs::remove_dir(root.join("database-corrupt-interrupted")).expect("remove quarantine");

    fs::write(root.join("nocturne.db.recovery-123"), "partial copy").expect("staged copy");
    assert!(has_recovery_artifacts(&root));
}

#[test]
fn access_failures_are_not_corruption() {
    let denied = Error::Io(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "permission denied",
    ));
    assert!(!is_recoverable_corruption(&denied));
}

#[test]
fn a_file_that_is_not_a_database_is_recoverable_corruption() {
    let root = scratch("recovery-not-database");
    let damaged = root.join("nocturne.db");
    fs::write(&damaged, "file is not a database, really").expect("damaged file");

    let error = inspect_database_file(&damaged).expect_err("damaged file");

    assert!(is_recoverable_corruption(&error));
}

#[test]
fn a_truncated_database_is_reported_as_corruption() {
    let root = scratch("recovery-truncated");
    let path = root.join("nocturne.db");
    database_at(&path, "preserved", 1);
    let file = File::options()
        .write(true)
        .open(&path)
        .expect("open for truncation");
    file.set_len(32).expect("truncate");
    drop(file);

    let error = inspect_database_file(&path).expect_err("truncated database");

    assert!(is_recoverable_corruption(&error));
}

#[test]
fn an_intact_copy_reports_its_schema_version() {
    let root = scratch("recovery-inspect");
    let path = root.join("nocturne.db");
    database_at(&path, "value", 7);

    assert_eq!(inspect_database_file(&path).expect("intact copy"), 7);
}

#[test]
fn only_intact_supported_copies_are_offered_newest_first() {
    let root = scratch("recovery-candidates");
    let backups = root.join("backups");
    fs::create_dir(&backups).expect("backups directory");
    let older = root.join("nocturne.db.backup-1");
    let interrupted = root.join("nocturne.db.recovery-interrupted");
    let newer = backups.join("nocturne-before-restore-2.db");
    let invalid = backups.join("nocturne-before-restore-invalid.db");
    let sidecar = root.join("nocturne.db.backup-1-wal");
    let future = root.join("nocturne.db.backup-future");
    database_at(&older, "older", 1);
    database_at(&interrupted, "interrupted", 1);
    database_at(&newer, "newer", 1);
    database_at(&sidecar, "sidecar", 1);
    database_at(&future, "future", 999);
    fs::write(&invalid, "arquivo inválido").expect("invalid copy");
    // 2026-01-01T00:00:00Z, 2026-01-01T12:00:00Z and 2026-01-02T00:00:00Z.
    set_modified(&older, 1_767_225_600);
    set_modified(&interrupted, 1_767_268_800);
    set_modified(&newer, 1_767_312_000);

    let candidates = list_recovery_candidates(&root);

    let paths: Vec<PathBuf> = candidates.iter().map(|item| item.path.clone()).collect();
    assert_eq!(paths, vec![newer, interrupted, older]);
    assert!(candidates.iter().all(|item| item.schema_version == 1));
    assert_eq!(candidates[0].modified_at, "2026-01-02T00:00:00.000Z");
}

#[test]
fn a_restore_installs_the_copy_and_quarantines_the_damaged_files() {
    let root = scratch("recovery-restore");
    let data = root.join("data");
    fs::create_dir(&data).expect("data directory");
    let current = data.join("nocturne.db");
    fs::write(&current, "banco corrompido").expect("damaged database");
    fs::write(data.join("nocturne.db-wal"), "wal corrompido").expect("damaged wal");
    let source = root.join("nocturne.db.backup-valid");
    database_at(&source, "restored", 1);

    let quarantine = restore_database_file(&data, &source).expect("restore");

    assert_eq!(inspect_database_file(&current).expect("restored copy"), 1);
    assert_eq!(stored_value(&current), "restored");
    assert_eq!(
        fs::read_to_string(quarantine.join("nocturne.db")).expect("quarantined database"),
        "banco corrompido"
    );
    assert_eq!(
        fs::read_to_string(quarantine.join("nocturne.db-wal")).expect("quarantined wal"),
        "wal corrompido"
    );
    assert!(!data.join("nocturne.db-wal").exists());
    assert!(
        quarantine
            .file_name()
            .expect("name")
            .to_string_lossy()
            .starts_with("database-corrupt-")
    );
    assert!(has_recovery_artifacts(&data));
    assert_restrictive_modes(&quarantine, &current);
}

#[test]
fn a_refused_copy_puts_the_original_files_back() {
    let root = scratch("recovery-rollback");
    let current = root.join("nocturne.db");
    fs::write(&current, "estado original").expect("original database");
    fs::write(root.join("nocturne.db-wal"), "wal original").expect("original wal");
    let invalid = root.join("invalid-backup.db");
    fs::write(&invalid, "cópia inválida").expect("invalid copy");

    let error = restore_database_file(&root, &invalid).expect_err("refused copy");

    assert!(error.to_string().starts_with(REFUSED));
    assert_eq!(
        fs::read_to_string(&current).expect("original database"),
        "estado original"
    );
    assert_eq!(
        fs::read_to_string(root.join("nocturne.db-wal")).expect("original wal"),
        "wal original"
    );
    assert!(names_starting_with(&root, "database-corrupt-").is_empty());
    assert!(names_starting_with(&root, "nocturne.db.recovery-").is_empty());
    assert!(invalid.exists());
}

#[test]
fn a_missing_copy_leaves_the_database_untouched() {
    let root = scratch("recovery-missing");
    let current = root.join("nocturne.db");
    fs::write(&current, "estado original").expect("original database");

    let error = restore_database_file(&root, &root.join("nao-existe.db")).expect_err("missing");

    assert!(error.to_string().starts_with(REFUSED));
    assert_eq!(
        fs::read_to_string(&current).expect("original database"),
        "estado original"
    );
    assert!(names_starting_with(&root, "database-corrupt-").is_empty());
}

#[cfg(unix)]
fn assert_restrictive_modes(quarantine: &Path, restored: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode(quarantine), 0o700);
    assert_eq!(mode(restored), 0o600);
}

#[cfg(not(unix))]
fn assert_restrictive_modes(_quarantine: &Path, _restored: &Path) {}
