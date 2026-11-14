//! Database recovery: lists the copies that can replace a damaged `nocturne.db`, checks them
//! and restores one. The damaged files move to a quarantine directory before the copy is
//! installed, so a restore never destroys them. Port of `electron/database/recovery.ts`.
//!
//! The functions do blocking filesystem I/O, so the application runs them off the interactive
//! path. A restore needs the database closed.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use noct_core::limits::DATABASE_SCHEMA_VERSION;
use rusqlite::{Connection, ErrorCode, OpenFlags};
use time::OffsetDateTime;

use crate::runtime::{
    DATABASE_FILE_NAME, MIGRATION_BACKUP_PREFIX, RECOVERY_DIRECTORY, RECOVERY_PREFIX,
    create_private_directory, sidecar,
};
use crate::{Error, Result, clock, schema};

const QUARANTINE_PREFIX: &str = "database-corrupt-";
/// Prefix of the verified copy written before it is installed.
const STAGING_PREFIX: &str = "nocturne.db.recovery-";
const SUPPORTED_SCHEMA: u32 = DATABASE_SCHEMA_VERSION as u32;
const RECOVERY_FAILED: &str = "A recuperação do banco falhou; o arquivo original foi preservado";
/// Texts that mark a damaged file when the error carries no SQLite code.
const CORRUPTION_MESSAGES: [&str; 3] = [
    "database disk image is malformed",
    "file is not a database",
    "falha de integridade",
];

/// A copy that can replace the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryCandidate {
    pub path: PathBuf,
    pub schema_version: u32,
    /// The modification time, UTC, in the `toISOString` format.
    pub modified_at: String,
}

/// Whether the data directory holds the remains of an interrupted restore: a quarantine
/// directory or a staged copy.
pub fn has_recovery_artifacts(data_directory: &Path) -> bool {
    let Ok(entries) = fs::read_dir(data_directory) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        name.starts_with(QUARANTINE_PREFIX) || name.starts_with(STAGING_PREFIX)
    })
}

/// Whether the error means the database file is damaged, so a recovery copy may replace it.
/// Access and permission failures are not corruption.
pub fn is_recoverable_corruption(error: &Error) -> bool {
    let coded = matches!(
        error,
        Error::Sqlite(rusqlite::Error::SqliteFailure(failure, _))
            if matches!(failure.code, ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)
    );
    if coded {
        return true;
    }
    let message = error.to_string().to_lowercase();
    CORRUPTION_MESSAGES
        .iter()
        .any(|pattern| message.contains(pattern))
}

/// Checks a database file read-only and returns its schema version. The file must exist, and
/// one that fails the integrity check is an error.
pub fn inspect_database_file(path: &Path) -> Result<u32> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity = schema::quick_check(&connection)?;
    if integrity != "ok" {
        return Err(Error::Message(format!(
            "Falha de integridade: {integrity}."
        )));
    }
    schema::user_version(&connection)
}

/// The copies that can replace the database, newest first. A damaged copy, or one whose
/// schema version the application does not support, is never offered.
pub fn list_recovery_candidates(data_directory: &Path) -> Vec<RecoveryCandidate> {
    let mut found: Vec<(SystemTime, RecoveryCandidate)> = Vec::new();
    for (directory, accepts) in candidate_locations(data_directory) {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !accepts(&name) {
                continue;
            }
            let path = entry.path();
            let Ok(schema_version) = inspect_database_file(&path) else {
                continue;
            };
            if !(1..=SUPPORTED_SCHEMA).contains(&schema_version) {
                continue;
            }
            let Ok(modified) = fs::metadata(&path).and_then(|metadata| metadata.modified()) else {
                continue;
            };
            let Ok(modified_at) = clock::iso_at(OffsetDateTime::from(modified)) else {
                continue;
            };
            found.push((
                modified,
                RecoveryCandidate {
                    path,
                    schema_version,
                    modified_at,
                },
            ));
        }
    }
    found.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    found.into_iter().map(|(_, candidate)| candidate).collect()
}

/// Replaces `nocturne.db` with `source`, once the copy passes the integrity check. The damaged
/// database and its `-wal` and `-shm` files move first to a new quarantine directory, which is
/// returned. A failed restore moves them back. The database must be closed.
pub fn restore_database_file(data_directory: &Path, source: &Path) -> Result<PathBuf> {
    let database_path = data_directory.join(DATABASE_FILE_NAME);
    let timestamp = clock::iso_now()?.replace([':', '.'], "-");
    let quarantine = data_directory.join(format!("{QUARANTINE_PREFIX}{timestamp}"));
    let staging = data_directory.join(format!(
        "{STAGING_PREFIX}{}-{timestamp}",
        std::process::id()
    ));
    create_private_directory(&quarantine)?;

    let mut progress = Progress::default();
    if let Err(error) = install(&database_path, source, &staging, &quarantine, &mut progress) {
        progress.undo(&database_path, &staging, &quarantine);
        return Err(Error::Message(format!("{RECOVERY_FAILED}: {error}")));
    }
    Ok(quarantine)
}

/// A directory that can hold recovery copies, with the test for the names accepted in it.
type CandidateLocation = (PathBuf, fn(&str) -> bool);

/// Where recovery copies live, with the names the application accepts in each place.
fn candidate_locations(data_directory: &Path) -> [CandidateLocation; 3] {
    [
        (data_directory.to_path_buf(), |name| {
            name.starts_with(MIGRATION_BACKUP_PREFIX) && !is_sidecar(name)
        }),
        (data_directory.to_path_buf(), |name| {
            name.starts_with(STAGING_PREFIX) && !is_sidecar(name)
        }),
        (data_directory.join(RECOVERY_DIRECTORY), |name| {
            name.starts_with(RECOVERY_PREFIX) && name.ends_with(".db")
        }),
    ]
}

fn is_sidecar(name: &str) -> bool {
    name.ends_with("-wal") || name.ends_with("-shm")
}

/// What a restore has changed so far, so that a failure can undo exactly that.
#[derive(Default)]
struct Progress {
    /// Each damaged file with its location before the move and in the quarantine.
    moved: Vec<(PathBuf, PathBuf)>,
    staged: bool,
}

impl Progress {
    fn undo(&self, database_path: &Path, staging: &Path, quarantine: &Path) {
        if self.staged {
            let _ = fs::remove_file(staging);
        }
        if self.moved.iter().any(|(source, _)| source == database_path) {
            let _ = fs::remove_file(database_path);
        }
        for (source, destination) in self.moved.iter().rev() {
            let _ = fs::rename(destination, source);
        }
        let _ = fs::remove_dir(quarantine);
    }
}

fn install(
    database_path: &Path,
    source: &Path,
    staging: &Path,
    quarantine: &Path,
    progress: &mut Progress,
) -> Result<()> {
    move_damaged_files(database_path, quarantine, &mut progress.moved)?;
    stage_copy(source, staging)?;
    progress.staged = true;
    inspect_database_file(staging)?;
    fs::rename(staging, database_path)?;
    Ok(())
}

/// Moves `nocturne.db` and its sidecars into the quarantine. A file that does not exist is
/// skipped.
fn move_damaged_files(
    database_path: &Path,
    quarantine: &Path,
    moved: &mut Vec<(PathBuf, PathBuf)>,
) -> io::Result<()> {
    for suffix in ["", "-wal", "-shm"] {
        let source = sidecar(database_path, suffix);
        let destination = quarantine.join(format!("{DATABASE_FILE_NAME}{suffix}"));
        match fs::rename(&source, &destination) {
            Ok(()) => moved.push((source, destination)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Copies `source` to a new file that only the owner can read, and flushes it. A copy that
/// fails part way is removed again. A file that was already there is never touched.
fn stage_copy(source: &Path, staging: &Path) -> io::Result<()> {
    let mut input = File::open(source)?;
    let copied = {
        let mut output = create_new_private_file(staging)?;
        io::copy(&mut input, &mut output).and_then(|_| output.sync_all())
    };
    if copied.is_err() {
        let _ = fs::remove_file(staging);
    }
    copied
}

fn create_new_private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
