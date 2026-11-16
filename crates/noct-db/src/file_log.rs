//! The application log file. Port of the file half of `electron/logging/Logger.ts`: one JSON object per
//! line with the fields `timestamp`, `sessionId`, `level`, `category`, `message` and `data`, written to
//! `nocturne.log` in the data directory. The file is created with mode 0600 on Unix. When it reaches
//! [`ROTATE_BYTES`] it moves to `nocturne.log.1`, and the next rotation replaces that backup.
//!
//! Recording never waits for the disk. A record is redacted, then queued for a background writer. A
//! record that finds the queue full, or a writer that is gone, is dropped and counted. Dropping the
//! sink closes the queue and waits until the writer has written what it holds.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};

use noct_core::log_redaction::{redact_text, redact_value};
use serde::Serialize;
use serde_json::Value;
use time::OffsetDateTime;
use time::macros::format_description;
use uuid::Uuid;

use crate::diagnostics::{Diagnostic, DiagnosticSink, Level};

/// The file the records go to, in the data directory next to `nocturne.db`.
pub const LOG_FILE_NAME: &str = "nocturne.log";
/// The size at which the file moves to its backup. The TypeScript logger uses the same limit.
pub const ROTATE_BYTES: u64 = 2_000_000;
/// The records waiting for the writer. A record beyond this is dropped, never waited for.
pub const QUEUE_CAPACITY: usize = 1_024;

/// Writes the diagnostics of the persistence layer to the application log file.
#[derive(Debug)]
pub struct FileLogSink {
    session_id: String,
    sender: Option<SyncSender<String>>,
    writer: Option<JoinHandle<()>>,
    dropped: AtomicU64,
    failures: Arc<AtomicU64>,
}

/// One line of the file, in the field order of the TypeScript logger.
#[derive(Serialize)]
struct Entry<'a> {
    timestamp: String,
    #[serde(rename = "sessionId")]
    session_id: &'a str,
    level: Level,
    category: &'a str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl FileLogSink {
    /// Opens the log file in `data_directory` and starts the writer. The file is created now, so a
    /// directory that cannot hold it is reported here and not on the first record.
    pub fn open(data_directory: &Path) -> io::Result<Self> {
        Self::open_with(data_directory, QUEUE_CAPACITY, ROTATE_BYTES)
    }

    /// The identifier of this run. Every record of the run carries it.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// The records dropped because the queue was full or the writer was gone.
    pub fn dropped_records(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// The records the writer could not write to the file.
    pub fn write_failures(&self) -> u64 {
        self.failures.load(Ordering::Relaxed)
    }

    fn open_with(data_directory: &Path, capacity: usize, rotate_bytes: u64) -> io::Result<Self> {
        fs::create_dir_all(data_directory)?;
        let writer = Writer::open(data_directory.join(LOG_FILE_NAME), rotate_bytes)?;
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let failures = Arc::new(AtomicU64::new(0));
        let worker_failures = Arc::clone(&failures);
        let handle = thread::Builder::new()
            .name("noct-log-writer".to_string())
            .spawn(move || writer.run(receiver, worker_failures))?;
        Ok(Self {
            session_id: Uuid::new_v4().to_string(),
            sender: Some(sender),
            writer: Some(handle),
            dropped: AtomicU64::new(0),
            failures,
        })
    }

    /// The JSON line of a record, redacted, or `None` when it cannot be serialized.
    fn line(&self, diagnostic: &Diagnostic<'_>) -> Option<String> {
        let entry = Entry {
            timestamp: timestamp(),
            session_id: &self.session_id,
            level: diagnostic.level,
            category: diagnostic.category,
            message: redact_text(diagnostic.message),
            data: diagnostic.data.map(redact_value),
        };
        let mut line = serde_json::to_string(&entry).ok()?;
        line.push('\n');
        Some(line)
    }
}

impl DiagnosticSink for FileLogSink {
    fn record(&self, diagnostic: &Diagnostic<'_>) {
        let Some(line) = self.line(diagnostic) else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let Some(sender) = &self.sender else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) = sender.try_send(line) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Drop for FileLogSink {
    fn drop(&mut self) {
        // Closing the queue lets the writer write what it holds and return.
        self.sender.take();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

/// The UTC time in the form `toISOString` gives: milliseconds and a `Z`.
fn timestamp() -> String {
    let format =
        format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z");
    OffsetDateTime::now_utc()
        .format(&format)
        .unwrap_or_default()
}

/// The file itself: appends lines, rotates them, and keeps the handle open between lines.
struct Writer {
    path: PathBuf,
    rotate_bytes: u64,
    file: Option<File>,
    size: u64,
}

impl Writer {
    fn open(path: PathBuf, rotate_bytes: u64) -> io::Result<Self> {
        let mut writer = Self {
            path,
            rotate_bytes,
            file: None,
            size: 0,
        };
        writer.reopen()?;
        Ok(writer)
    }

    /// Writes lines until the sink is dropped and the queue is empty.
    fn run(mut self, receiver: Receiver<String>, failures: Arc<AtomicU64>) {
        for line in receiver {
            if let Err(error) = self.write(&line) {
                failures.fetch_add(1, Ordering::Relaxed);
                eprintln!(
                    "noct-db warn persistence.log: a record could not be written ({:?})",
                    error.kind()
                );
            }
        }
    }

    fn write(&mut self, line: &str) -> io::Result<()> {
        if self.size >= self.rotate_bytes {
            self.rotate()?;
        }
        if self.file.is_none() {
            self.reopen()?;
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("the log file is not open"))?;
        file.write_all(line.as_bytes())?;
        self.size += line.len() as u64;
        Ok(())
    }

    /// Moves the file to its backup, replacing the previous backup. The handle closes first, so that
    /// the rename works on every platform.
    fn rotate(&mut self) -> io::Result<()> {
        self.file = None;
        let mut backup = self.path.clone().into_os_string();
        backup.push(".1");
        let _ = fs::remove_file(&backup);
        fs::rename(&self.path, &backup)?;
        self.size = 0;
        Ok(())
    }

    fn reopen(&mut self) -> io::Result<()> {
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&self.path)?;
        self.size = file.metadata()?.len();
        self.file = Some(file);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(name: &str) -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/noct-db-file-log")
            .join(name);
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("directory");
        path
    }

    /// Rotation with a 50-byte limit: each line is 30 bytes, so the third write rotates. The backup
    /// always holds the two lines before the last rotation, and the file the lines after it.
    #[test]
    fn rotation_keeps_one_backup_and_replaces_it() {
        let dir = directory("rotation");
        let path = dir.join(LOG_FILE_NAME);
        let mut writer = Writer::open(path.clone(), 50).expect("writer");

        for index in 1..=5 {
            writer
                .write(&format!("{index:02}{}\n", "-".repeat(27)))
                .expect("write");
        }

        let backup = fs::read_to_string(dir.join("nocturne.log.1")).expect("backup");
        let current = fs::read_to_string(&path).expect("current file");
        assert!(
            backup.starts_with("03") && backup.contains("\n04"),
            "{backup:?}"
        );
        assert!(current.starts_with("05"), "{current:?}");
        assert_eq!(backup.len(), 60);
        assert_eq!(current.len(), 30);
        assert!(!dir.join("nocturne.log.2").exists());
    }

    /// A queue of one record, with no writer draining it: the second record is dropped and counted.
    /// The receiver stays alive, so the queue is full and not disconnected.
    #[test]
    fn a_full_queue_drops_the_record_and_counts_it() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let sink = FileLogSink {
            session_id: "session".to_string(),
            sender: Some(sender),
            writer: None,
            dropped: AtomicU64::new(0),
            failures: Arc::new(AtomicU64::new(0)),
        };
        let diagnostic = Diagnostic {
            level: Level::Warn,
            category: "persistence.test",
            message: "a warning",
            data: None,
        };

        sink.record(&diagnostic);
        sink.record(&diagnostic);

        assert_eq!(sink.dropped_records(), 1);
    }
}
