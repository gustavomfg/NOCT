//! Owns the SQLite connection and its startup/shutdown invariants.
//!
//! Port of `electron/database/DatabaseRuntime.ts`. The order of operations, the
//! pragmas, the file permissions, the retention rules and the user-facing messages
//! are kept identical so that a database opened by either implementation behaves the
//! same way.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use noct_core::limits::DATABASE_SCHEMA_VERSION;
use rusqlite::backup::Progress;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Duration, OffsetDateTime};

use crate::{Error, Result, migrations, schema};

pub const DATABASE_FILE_NAME: &str = "nocturne.db";
const MIGRATION_BACKUP_PREFIX: &str = "nocturne.db.backup-";
const MIGRATION_BACKUP_RETENTION: usize = 3;
const RECOVERY_DIRECTORY: &str = "backups";
const RECOVERY_PREFIX: &str = "nocturne-before-restore-";
/// Retention used by the recovery point taken before a restore.
pub const RECOVERY_RETENTION: usize = 5;
const QUICK_CHECK_KEY: &str = "maintenance.lastQuickCheck";
const QUICK_CHECK_INTERVAL: Duration = Duration::days(7);
const SUPPORTED_SCHEMA: u32 = DATABASE_SCHEMA_VERSION as u32;

/// Pragmas applied to every connection, in the same order as the TypeScript runtime.
/// `synchronous = FULL` gives commit-time durability in WAL mode.
const CONNECTION_PRAGMAS: &str = "
    PRAGMA journal_mode = WAL;
    PRAGMA foreign_keys = ON;
    PRAGMA busy_timeout = 5000;
    PRAGMA synchronous = FULL;
    PRAGMA temp_store = MEMORY;
";

const ORPHAN_CLEANUP_SQL: &str = "DELETE FROM messages WHERE conversation_id NOT IN (SELECT id FROM conversations); DELETE FROM artifacts WHERE conversation_id NOT IN (SELECT id FROM conversations); DELETE FROM suggestions WHERE conversation_id NOT IN (SELECT id FROM conversations); DELETE FROM suggestion_decisions WHERE suggestion_id NOT IN (SELECT id FROM suggestions); DELETE FROM brain_memories WHERE workspace_id NOT IN (SELECT path FROM workspaces) OR (conversation_id IS NOT NULL AND conversation_id NOT IN (SELECT id FROM conversations)); DELETE FROM workspace_model_bindings WHERE workspace_id NOT IN (SELECT path FROM workspaces);";

/// An open, migrated NOCT database.
pub struct Database {
    data_directory: PathBuf,
    path: PathBuf,
    connection: Connection,
}

impl Database {
    /// Opens `nocturne.db` inside `data_directory`, migrates it to the supported schema
    /// and runs the startup maintenance. Fails without modifying the schema when the
    /// file was written by a newer application version.
    pub fn open(data_directory: &Path) -> Result<Self> {
        let path = data_directory.join(DATABASE_FILE_NAME);
        restrict_if_present(&path)?;
        let mut connection = Connection::open(&path)?;
        restrict_if_present(&path)?;

        let schema_version = schema::user_version(&connection)?;
        if schema_version > SUPPORTED_SCHEMA {
            return Err(Error::Message(format!(
                "Este banco usa o schema {schema_version}, mas esta versão do Nocturne suporta até o schema {SUPPORTED_SCHEMA}. Atualize o aplicativo antes de abrir estes dados."
            )));
        }

        connection.execute_batch(CONNECTION_PRAGMAS)?;
        restrict_database_files(&path)?;

        if schema_version > 0 && schema_version < SUPPORTED_SCHEMA && path.exists() {
            create_migration_backup(&connection, &path)?;
        }

        migrations::migrate_latest(&mut connection, schema_version).map_err(|error| {
            Error::Message(format!(
                "A migração do banco falhou e foi revertida integralmente: {error}"
            ))
        })?;
        restrict_database_files(&path)?;

        let database = Self {
            data_directory: data_directory.to_path_buf(),
            path,
            connection,
        };
        database.run_scheduled_integrity_check()?;
        database.cleanup_orphans()?;
        Ok(database)
    }

    pub fn data_directory(&self) -> &Path {
        &self.data_directory
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }

    /// Copies the live database to `backups/nocturne-before-restore-<timestamp>.db` and
    /// keeps the newest `retain` (at least one) recovery points.
    ///
    /// Blocking: callers on an interactive path must run this off the UI thread.
    pub fn create_recovery_snapshot(&self, retain: usize) -> Result<PathBuf> {
        let directory = self.data_directory.join(RECOVERY_DIRECTORY);
        create_private_directory(&directory)?;
        let timestamp = OffsetDateTime::now_utc()
            .format(&format_description!(
                "[year]-[month]-[day]T[hour]-[minute]-[second]-[subsecond digits:3]Z"
            ))
            .map_err(|error| Error::Message(error.to_string()))?;
        let destination = directory.join(format!("{RECOVERY_PREFIX}{timestamp}.db"));

        let result = (|| -> Result<()> {
            self.connection
                .backup("main", &destination, None::<fn(Progress)>)?;
            restrict_path(&destination)?;
            prune_matching(&directory, RECOVERY_PREFIX, ".db", retain.max(1))?;
            Ok(())
        })();

        if let Err(error) = result {
            let _ = fs::remove_file(&destination);
            return Err(Error::Message(format!(
                "Não foi possível criar o ponto de recuperação: {error}"
            )));
        }
        Ok(destination)
    }

    /// Removes rows whose parent no longer exists. Runs on every open.
    pub fn cleanup_orphans(&self) -> Result<()> {
        self.connection.execute_batch(ORPHAN_CLEANUP_SQL)?;
        Ok(())
    }

    /// Checkpoints and closes the connection, then restricts the database files again.
    pub fn close(self) -> Result<()> {
        self.connection
            .execute_batch("PRAGMA optimize; PRAGMA wal_checkpoint(PASSIVE);")?;
        self.connection
            .close()
            .map_err(|(_, error)| Error::Sqlite(error))?;
        restrict_database_files(&self.path)?;
        Ok(())
    }

    /// Runs `quick_check` at most once every seven days. A corrupted database is
    /// reported and left untouched so the user can restore a backup.
    fn run_scheduled_integrity_check(&self) -> Result<()> {
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM settings WHERE key=?",
                [QUICK_CHECK_KEY],
                |row| row.get(0),
            )
            .optional()?;
        let recently_checked = stored
            .as_deref()
            .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
            .is_some_and(|last_check| {
                OffsetDateTime::now_utc() - last_check < QUICK_CHECK_INTERVAL
            });
        if recently_checked {
            return Ok(());
        }

        let integrity = schema::quick_check(&self.connection)?;
        if integrity != "ok" {
            return Err(Error::Message(format!(
                "Banco de dados corrompido ({integrity}). Preserve o arquivo e restaure um backup."
            )));
        }
        self.connection.execute(
            "INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            (QUICK_CHECK_KEY, iso_timestamp(OffsetDateTime::now_utc())?),
        )?;
        Ok(())
    }
}

/// Copies the database before a schema upgrade and verifies the copy. The copy is kept
/// only when it passes `quick_check`; at most `MIGRATION_BACKUP_RETENTION` are retained.
fn create_migration_backup(connection: &Connection, database_path: &Path) -> Result<()> {
    let integrity = schema::quick_check(connection)?;
    if integrity != "ok" {
        return Err(Error::Message(format!(
            "O banco atual falhou na verificação de integridade ({integrity}); a migração não foi iniciada."
        )));
    }
    let busy: i64 = connection
        .query_row("PRAGMA wal_checkpoint(FULL)", [], |row| row.get(0))
        .optional()?
        .unwrap_or(0);
    if busy != 0 {
        return Err(Error::Message(
            "O WAL está ocupado; a cópia pré-migração não foi concluída com segurança.".to_string(),
        ));
    }

    let directory = database_path.parent().unwrap_or(Path::new("."));
    let millis = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
    let backup_path = directory.join(format!("{MIGRATION_BACKUP_PREFIX}{millis}.db"));

    let copied = fs::copy(database_path, &backup_path).and_then(|_| restrict_path(&backup_path));
    if let Err(error) = copied {
        remove_backup(&backup_path);
        return Err(Error::Io(error));
    }

    let verified = (|| -> Result<()> {
        let backup = Connection::open_with_flags(&backup_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let backup_integrity = schema::quick_check(&backup)?;
        if backup_integrity != "ok" {
            return Err(Error::Message(backup_integrity));
        }
        Ok(())
    })();
    remove_sidecars(&backup_path);

    if let Err(error) = verified {
        remove_backup(&backup_path);
        return Err(Error::Message(format!(
            "Não foi possível verificar o backup pré-migração: {error}"
        )));
    }

    prune_matching(
        directory,
        MIGRATION_BACKUP_PREFIX,
        ".db",
        MIGRATION_BACKUP_RETENTION,
    )?;
    Ok(())
}

/// Keeps the `keep` lexicographically largest names that match `prefix` and `suffix`.
/// Timestamped names sort chronologically, so this keeps the newest files.
fn prune_matching(directory: &Path, prefix: &str, suffix: &str, keep: usize) -> Result<()> {
    let mut names = Vec::new();
    for entry in fs::read_dir(directory)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.starts_with(prefix) && name.ends_with(suffix) {
            names.push(name);
        }
    }
    names.sort_unstable_by(|left, right| right.cmp(left));
    for name in names.iter().skip(keep) {
        remove_file_if_present(&directory.join(name))?;
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn remove_file_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Removes the `-wal` and `-shm` companions of a backup. Errors are ignored so the
/// original failure is the one reported.
fn remove_sidecars(path: &Path) {
    for suffix in ["-wal", "-shm"] {
        let _ = remove_file_if_present(&sidecar(path, suffix));
    }
}

fn remove_backup(path: &Path) {
    let _ = remove_file_if_present(path);
    remove_sidecars(path);
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn restrict_database_files(path: &Path) -> io::Result<()> {
    for suffix in ["", "-wal", "-shm"] {
        restrict_if_present(&sidecar(path, suffix))?;
    }
    Ok(())
}

/// Owner read/write only. On platforms without POSIX modes this is a no-op, which is
/// also what the TypeScript runtime does there.
fn restrict_path(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn restrict_if_present(path: &Path) -> io::Result<()> {
    match restrict_path(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Same shape as `Date.prototype.toISOString()`: UTC with millisecond precision.
fn iso_timestamp(at: OffsetDateTime) -> Result<String> {
    at.format(&format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
    ))
    .map_err(|error| Error::Message(error.to_string()))
}
