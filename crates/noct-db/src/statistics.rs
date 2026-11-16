//! Statistics of the semantic index, refreshed with `ANALYZE` on `semantic_units` alone.
//!
//! `PRAGMA optimize` stores sampled estimates for a table that has no statistics, and it does not
//! refresh them when the table grows. The planner then keeps a slow plan for one large workspace.
//! A database closed by the TypeScript build can hold such estimates too, so the check does not look
//! only for missing statistics. It compares the stored row count and the stored rows per workspace with
//! the table. The measurements are in `docs/perf/semantic-index-query-plans.md`; the decision is in ADR 0002.
//!
//! A failure in this module is never an error for the caller. It is written to the diagnostic sink and
//! reported as [`StatisticsOutcome::Failed`]. The statistics only steer the planner, so no data is affected.
//!
//! No migration is involved and `user_version` does not change. The first `ANALYZE` makes SQLite create
//! its internal `sqlite_stat1` and `sqlite_stat4` tables, which NOCT does not define or read.

use std::path::Path;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};

use crate::diagnostics::DiagnosticSink;
use crate::{Error, Result};

/// A table or a round with at least this many rows depends on its statistics. This is a judgement, not
/// a budget; see the limits in `docs/perf/semantic-index-query-plans.md`.
pub(crate) const LARGE_ROWS: i64 = 10_000;

/// The diagnostic event of this module.
pub(crate) const EVENT: &str = "persistence.statistics";

const INDEX: &str = "idx_semantic_units_workspace_status";

/// A stored figure is accepted when it is within this share of the table's value. Stored averages differ
/// from the exact ones only by rounding. Sampled estimates were far outside it: 1,409 against 10,000 rows
/// at the smallest measured size.
const TOLERANCE: f64 = 0.10;

/// What a statistics step did. A failure is `Failed`, with its reason, and the reason is in the log too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatisticsOutcome {
    /// The table is too small, or its statistics describe it. Nothing ran.
    NotNeeded,
    /// `ANALYZE semantic_units` ran.
    Analyzed,
    /// The step failed. The statistics are as they were before the step.
    Failed(String),
}

/// Whether `semantic_units` has at least `LARGE_ROWS` rows, and its statistics are missing or do not
/// describe it. The row count and the rows per workspace are both compared: a sampled analysis keeps the
/// table total right and the per-workspace figure wrong, so a check on the total alone would miss it.
pub(crate) fn needs_refresh(connection: &Connection) -> Result<bool> {
    let rows: i64 =
        connection.query_row("SELECT COUNT(*) FROM semantic_units", [], |row| row.get(0))?;
    if rows < LARGE_ROWS {
        return Ok(false);
    }
    let Some(stat) = index_stat(connection)? else {
        return Ok(true);
    };
    let Some((stored_rows, stored_per_workspace)) = parse_stat(&stat) else {
        return Ok(true);
    };
    let workspaces: i64 = connection.query_row(
        "SELECT COUNT(DISTINCT workspace) FROM semantic_units",
        [],
        |row| row.get(0),
    )?;
    let rows = rows as f64;
    let per_workspace = rows / workspaces.max(1) as f64;
    Ok(!within(stored_rows, rows) || !within(stored_per_workspace, per_workspace))
}

/// Refreshes the statistics when they need it. `context` says where the call happens, for the log.
pub(crate) fn refresh_if_needed(
    connection: &Connection,
    diagnostics: &dyn DiagnosticSink,
    context: &str,
) -> StatisticsOutcome {
    let needed = match needs_refresh(connection) {
        Ok(needed) => needed,
        Err(error) => {
            return StatisticsOutcome::Failed(record_failure(diagnostics, context, &error));
        }
    };
    if !needed {
        return StatisticsOutcome::NotNeeded;
    }
    match analyze(connection) {
        Ok(()) => StatisticsOutcome::Analyzed,
        Err(error) => StatisticsOutcome::Failed(record_failure(diagnostics, context, &error)),
    }
}

/// Analyzes `semantic_units` after a large round has committed. The round is already recorded, so a
/// failure is written to the log and is not returned.
pub(crate) fn refresh_after_round(connection: &Connection, diagnostics: &dyn DiagnosticSink) {
    if let Err(error) = analyze(connection) {
        record_failure(diagnostics, "after a large round", &error);
    }
}

/// Starts the analysis on a connection of its own when the table needs it, so the caller is not blocked.
/// The caller waits for the returned handle. A failure to decide is logged and starts nothing.
pub(crate) fn start_if_needed(
    connection: &Connection,
    path: &Path,
    busy_timeout: Duration,
    diagnostics: Arc<dyn DiagnosticSink>,
) -> Option<JoinHandle<StatisticsOutcome>> {
    match needs_refresh(connection) {
        Ok(true) => Some(start_in_background(path, busy_timeout, diagnostics)),
        Ok(false) => None,
        Err(error) => {
            record_failure(diagnostics.as_ref(), "on open", &error);
            None
        }
    }
}

/// Runs `ANALYZE` on a connection of its own. The connection waits at most `busy_timeout` for a lock that
/// the application's writes hold; past that the analysis fails and is logged.
pub(crate) fn start_in_background(
    path: &Path,
    busy_timeout: Duration,
    diagnostics: Arc<dyn DiagnosticSink>,
) -> JoinHandle<StatisticsOutcome> {
    let path = path.to_path_buf();
    thread::spawn(move || {
        let connection = match open_with_busy_timeout(&path, busy_timeout) {
            Ok(connection) => connection,
            Err(error) => {
                return StatisticsOutcome::Failed(record_failure(
                    diagnostics.as_ref(),
                    "on open",
                    &error,
                ));
            }
        };
        match analyze(&connection) {
            Ok(()) => StatisticsOutcome::Analyzed,
            Err(error) => {
                StatisticsOutcome::Failed(record_failure(diagnostics.as_ref(), "on open", &error))
            }
        }
    })
}

/// Records that the analysis thread panicked. The thread returns no outcome in that case.
pub(crate) fn background_panicked(diagnostics: &dyn DiagnosticSink) -> StatisticsOutcome {
    let reason = "the analysis thread panicked".to_string();
    diagnostics.warn(EVENT, &format!("on open: {reason}"));
    StatisticsOutcome::Failed(reason)
}

fn open_with_busy_timeout(path: &Path, busy_timeout: Duration) -> Result<Connection> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(busy_timeout)?;
    Ok(connection)
}

/// Analyzes `semantic_units` alone. The statistics of the other tables are left as they are.
fn analyze(connection: &Connection) -> Result<()> {
    connection.execute_batch("ANALYZE semantic_units;")?;
    Ok(())
}

/// Writes a failure to the diagnostic sink and returns its reason.
fn record_failure(diagnostics: &dyn DiagnosticSink, context: &str, error: &Error) -> String {
    let reason = error.to_string();
    diagnostics.warn(EVENT, &format!("{context}: {reason}"));
    reason
}

/// Equal within the tolerance. A zero actual value is compared against an absolute tolerance of one.
fn within(stored: f64, actual: f64) -> bool {
    (stored - actual).abs() <= TOLERANCE * actual.max(1.0)
}

/// The first two numbers of a `stat`: the table's row count, and the average rows per first index
/// column, which is the workspace.
fn parse_stat(stat: &str) -> Option<(f64, f64)> {
    let mut values = stat.split_whitespace().map(str::parse::<f64>);
    let rows = values.next()?.ok()?;
    let per_workspace = values.next()?.ok()?;
    Some((rows, per_workspace))
}

fn index_stat(connection: &Connection) -> Result<Option<String>> {
    let table_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='sqlite_stat1')",
        [],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Ok(None);
    }
    Ok(connection
        .query_row(
            "SELECT stat FROM sqlite_stat1 WHERE tbl='semantic_units' AND idx=?1",
            [INDEX],
            |row| row.get(0),
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use rusqlite::OpenFlags;

    use super::*;

    /// Keeps the warnings it receives, so a test can check what was logged.
    #[derive(Debug, Default)]
    struct Recording(Mutex<Vec<String>>);

    impl DiagnosticSink for Recording {
        fn warn(&self, event: &'static str, message: &str) {
            self.0
                .lock()
                .expect("recording lock")
                .push(format!("{event}: {message}"));
        }
    }

    /// A database under `target/` with `rows` rows of `semantic_units` and no statistics. Only the columns
    /// the statistics read are created.
    fn unanalyzed(name: &str, rows: i64) -> PathBuf {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/noct-db-statistics")
            .join(name);
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("directory");
        let path = directory.join("nocturne.db");
        let connection = Connection::open(&path).expect("open");
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL; \
                 CREATE TABLE semantic_units(id TEXT PRIMARY KEY, workspace TEXT NOT NULL);",
            )
            .expect("schema");
        connection
            .execute(
                "WITH RECURSIVE numbers(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM numbers WHERE i < ?1) \
                 INSERT INTO semantic_units(id, workspace) SELECT 'unit-' || i, '/work/a' FROM numbers",
                [rows],
            )
            .expect("rows");
        path
    }

    #[test]
    fn a_background_analysis_blocked_past_a_short_busy_timeout_is_logged_and_reported() {
        let path = unanalyzed("background-blocked", LARGE_ROWS);
        let holder = Connection::open(&path).expect("holder");
        holder.execute_batch("BEGIN IMMEDIATE").expect("lock");
        let recording = Arc::new(Recording::default());

        let outcome = start_in_background(&path, Duration::from_millis(10), recording.clone())
            .join()
            .expect("analysis thread");
        holder.execute_batch("ROLLBACK").expect("release");

        let StatisticsOutcome::Failed(reason) = outcome else {
            panic!("a blocked analysis must fail, got {outcome:?}");
        };
        assert!(
            reason.contains("locked"),
            "the reason names the lock: {reason}"
        );
        let entries = recording.0.lock().expect("recording lock");
        assert_eq!(entries.len(), 1, "the failure is logged once: {entries:?}");
        assert!(entries[0].starts_with(EVENT), "{entries:?}");
    }

    #[test]
    fn a_background_analysis_without_contention_analyzes_and_logs_nothing() {
        let path = unanalyzed("background-free", LARGE_ROWS);
        let recording = Arc::new(Recording::default());

        let outcome = start_in_background(&path, Duration::from_millis(10), recording.clone())
            .join()
            .expect("analysis thread");

        assert_eq!(outcome, StatisticsOutcome::Analyzed);
        assert!(recording.0.lock().expect("recording lock").is_empty());
    }

    #[test]
    fn a_failed_analysis_after_a_large_round_is_logged_and_returns_nothing() {
        let path = unanalyzed("round-end-read-only", LARGE_ROWS);
        let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("read-only");
        let recording = Recording::default();

        refresh_after_round(&connection, &recording);

        let entries = recording.0.lock().expect("recording lock");
        assert_eq!(entries.len(), 1, "the failure is logged once: {entries:?}");
        assert!(entries[0].starts_with(EVENT), "{entries:?}");
        assert!(entries[0].contains("after a large round"), "{entries:?}");
    }
}
