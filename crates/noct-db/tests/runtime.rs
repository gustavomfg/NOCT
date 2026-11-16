//! Runtime behaviour of `Database`: startup order, permissions, pre-migration backups,
//! retention, recovery points and maintenance. Each expectation mirrors
//! electron/database/DatabaseRuntime.ts.

use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

use noct_db::migrations;
use noct_db::runtime::{DATABASE_FILE_NAME, Database, RECOVERY_RETENTION};
use noct_db::schema;
use rusqlite::Connection;

mod common;
use common::scratch;

const QUICK_CHECK_KEY: &str = "maintenance.lastQuickCheck";

/// Creates a database file the way the runtime does, stopped at `version`.
fn create_database_at(dir: &Path, version: u32) {
    let mut conn = Connection::open(dir.join(DATABASE_FILE_NAME)).expect("database file");
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
        .expect("pragmas");
    migrations::migrate_to(&mut conn, 0, version).expect("legacy migrations");
}

fn file_user_version(path: &Path) -> u32 {
    let conn = Connection::open(path).expect("open for inspection");
    schema::user_version(&conn).expect("user version")
}

fn names_matching(dir: &Path, prefix: &str, suffix: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("directory")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.starts_with(prefix) && name.ends_with(suffix))
        .collect();
    names.sort();
    names
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).expect("metadata").permissions().mode() & 0o777
}

#[test]
fn fresh_database_opens_at_latest_schema_with_private_files() {
    let dir = scratch("fresh");
    let db = Database::open_runtime(&dir).expect("open");
    assert_eq!(schema::user_version(db.connection()).unwrap(), 32);
    db.close().expect("close");

    let path = dir.join(DATABASE_FILE_NAME);
    assert_eq!(file_user_version(&path), 32);
    #[cfg(unix)]
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn newer_schema_is_refused_without_touching_the_file() {
    let dir = scratch("newer");
    let path = dir.join(DATABASE_FILE_NAME);
    Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA user_version = 33;")
        .unwrap();

    let Err(error) = Database::open_runtime(&dir) else {
        panic!("a newer schema must be refused");
    };
    assert_eq!(
        error.to_string(),
        "Este banco usa o schema 33, mas esta versão do Nocturne suporta até o schema 32. Atualize o aplicativo antes de abrir estes dados."
    );
    assert_eq!(file_user_version(&path), 33);
    assert!(names_matching(&dir, "nocturne.db.backup-", ".db").is_empty());
}

#[test]
fn legacy_database_is_backed_up_before_migrating_and_backups_are_pruned() {
    let dir = scratch("legacy");
    create_database_at(&dir, 20);
    // Millisecond stamps of the same width as real backups, so ordering is realistic.
    for stamp in [
        1_600_000_000_000u64,
        1_650_000_000_000,
        1_660_000_000_000,
        1_670_000_000_000,
    ] {
        fs::write(dir.join(format!("nocturne.db.backup-{stamp}.db")), b"older").unwrap();
    }

    let db = Database::open_runtime(&dir).expect("open legacy database");
    db.close().expect("close");

    assert_eq!(file_user_version(&dir.join(DATABASE_FILE_NAME)), 32);
    let backups = names_matching(&dir, "nocturne.db.backup-", ".db");
    assert_eq!(
        backups.len(),
        3,
        "retention keeps three backups: {backups:?}"
    );
    assert!(!dir.join("nocturne.db.backup-1600000000000.db").exists());
    assert!(!dir.join("nocturne.db.backup-1650000000000.db").exists());

    let newest = backups.last().expect("new backup");
    assert!(
        !newest.contains("1670000000000"),
        "the new backup must be the newest entry: {newest}"
    );
    let newest_path = dir.join(newest);
    assert_eq!(
        file_user_version(&newest_path),
        20,
        "backup holds the pre-migration schema"
    );
    assert!(!dir.join(format!("{newest}-wal")).exists());
    assert!(!dir.join(format!("{newest}-shm")).exists());
    #[cfg(unix)]
    assert_eq!(mode(&newest_path), 0o600);
}

#[test]
fn recovery_snapshots_are_private_and_limited_to_the_newest_points() {
    let dir = scratch("recovery");
    let db = Database::open_runtime(&dir).expect("open");
    for _ in 0..(RECOVERY_RETENTION + 1) {
        db.create_recovery_snapshot(RECOVERY_RETENTION)
            .expect("snapshot");
        // Snapshot names carry millisecond timestamps; keep them distinct.
        thread::sleep(Duration::from_millis(3));
    }

    let backups_dir = dir.join("backups");
    let snapshots = names_matching(&backups_dir, "nocturne-before-restore-", ".db");
    assert_eq!(snapshots.len(), RECOVERY_RETENTION);
    #[cfg(unix)]
    {
        assert_eq!(mode(&backups_dir), 0o700);
        for name in &snapshots {
            assert_eq!(mode(&backups_dir.join(name)), 0o600);
        }
    }
}

#[test]
fn startup_cleanup_removes_rows_left_behind_by_deleted_conversations() {
    let dir = scratch("orphans");
    let db = Database::open_runtime(&dir).expect("open");
    db.connection()
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             INSERT INTO messages (id, conversation_id, role, content, created_at)
               VALUES ('orphan', 'missing', 'user', 'x', '2026-01-01T00:00:00.000Z');
             PRAGMA foreign_keys = ON;",
        )
        .expect("insert orphan");

    db.cleanup_orphans().expect("cleanup");
    let remaining: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE id = 'orphan'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 0);
}

#[test]
fn integrity_check_is_recorded_and_repeated_only_after_seven_days() {
    let dir = scratch("quick-check");
    let read_stamp = |db: &Database| -> String {
        db.connection()
            .query_row(
                "SELECT value FROM settings WHERE key = ?",
                [QUICK_CHECK_KEY],
                |row| row.get(0),
            )
            .unwrap()
    };

    let db = Database::open_runtime(&dir).expect("first open");
    let first = read_stamp(&db);
    db.close().expect("close");
    assert_eq!(
        first.len(),
        24,
        "ISO-8601 with millisecond precision: {first}"
    );
    assert!(first.ends_with('Z'));

    // A stamp older than seven days triggers a new check on the next open.
    Connection::open(dir.join(DATABASE_FILE_NAME))
        .unwrap()
        .execute(
            "UPDATE settings SET value = '2001-01-01T00:00:00.000Z' WHERE key = ?",
            [QUICK_CHECK_KEY],
        )
        .unwrap();
    let db = Database::open_runtime(&dir).expect("open after stale stamp");
    let refreshed = read_stamp(&db);
    db.close().expect("close");
    assert_ne!(refreshed, "2001-01-01T00:00:00.000Z");

    // A recent stamp is left alone.
    let db = Database::open_runtime(&dir).expect("open within interval");
    assert_eq!(read_stamp(&db), refreshed);
    db.close().expect("close");
}
