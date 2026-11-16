//! The statistics of the semantic index. `ANALYZE` runs on `semantic_units` when a large round ends, when a
//! database whose table needs statistics is opened (in the background), and when the database is closed.
//! The check compares the stored statistics with the table, so a database closed by the TypeScript build,
//! which holds the sampled estimates of `PRAGMA optimize`, is recognised too.

use std::ops::Range;
use std::path::Path;
use std::sync::{Arc, Mutex};

use noct_core::code_intelligence::{
    DiscoveryFileClassification, ProjectIndexFile, ProjectIndexFileState,
};
use noct_core::limits::DATABASE_SCHEMA_VERSION;
use noct_core::semantic_index::{
    SemanticIndexRun, SemanticRunKind, SemanticRunStatus, SemanticUnit, SemanticUnitKind,
    SemanticUnitLocation, SemanticUnitStatus,
};
use noct_db::diagnostics::DiagnosticSink;
use noct_db::repositories::project_index::ProjectIndex;
use noct_db::repositories::semantic_index::{PersistedSemanticUnit, SemanticIndex};
use noct_db::runtime::DATABASE_FILE_NAME;
use noct_db::{Database, OpenOptions, StatisticsOutcome};
use rusqlite::{Connection, OptionalExtension};

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";
const WORKSPACE: &str = "/work/large";
const UNITS_PER_FILE: usize = 200;
const INDEX: &str = "idx_semantic_units_workspace_status";
const EVENT: &str = "persistence.statistics";

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

impl Recording {
    fn entries(&self) -> Vec<String> {
        self.0.lock().expect("recording lock").clone()
    }
}

fn open_recording(directory: &Path) -> (Database, Arc<Recording>) {
    let recording = Arc::new(Recording::default());
    let options = OpenOptions {
        diagnostics: recording.clone(),
        ..OpenOptions::default()
    };
    let database = Database::open_with(directory, options).expect("open");
    (database, recording)
}

/// Writes the units numbered `numbers` of `workspace`, `UNITS_PER_FILE` to a file. The ranges must start
/// on a file boundary, so that no file is written twice. The workspace row and the project files come
/// first, as the foreign keys require.
fn fill(database: &Database, workspace: &str, numbers: Range<usize>) {
    assert_eq!(
        numbers.start % UNITS_PER_FILE,
        0,
        "a range starts on a file boundary"
    );
    let connection = database.connection();
    connection
        .execute(
            "INSERT OR IGNORE INTO workspaces(path,name,created_at,last_opened_at) \
             VALUES(?1,'large',?2,?2)",
            [workspace, AT],
        )
        .expect("workspace");
    let project = ProjectIndex::new(connection);
    let index = SemanticIndex::new(connection);
    let first_file = numbers.start / UNITS_PER_FILE;
    for file in first_file..numbers.end.div_ceil(UNITS_PER_FILE) {
        let path = format!("src/file-{file}.ts");
        project
            .upsert_file(&project_file(workspace, &path))
            .expect("project file");
        let first = (file * UNITS_PER_FILE).max(numbers.start);
        let last = ((file + 1) * UNITS_PER_FILE).min(numbers.end);
        let items: Vec<PersistedSemanticUnit> = (first..last)
            .map(|number| {
                plain(unit(
                    workspace,
                    &format!("{workspace}#unit-{number}"),
                    &path,
                ))
            })
            .collect();
        index
            .replace_file_units(workspace, &path, &items)
            .expect("units");
    }
}

/// Starts a run and ends it with `status` and `indexed_units`, as the indexing service does.
fn end_run(database: &Database, id: &str, status: SemanticRunStatus, indexed_units: i64) {
    let index = SemanticIndex::new(database.connection());
    let mut run = SemanticIndexRun {
        id: id.to_string(),
        workspace: WORKSPACE.to_string(),
        kind: SemanticRunKind::Initial,
        status: SemanticRunStatus::Running,
        total_files: 1,
        processed_files: 1,
        indexed_units: 0,
        lexical_only_units: 0,
        failed_files: 0,
        excluded_files: 0,
        started_at: AT.to_string(),
        completed_at: None,
        updated_at: AT.to_string(),
        error: None,
    };
    index.create_run(&run).expect("create run");
    run.status = status;
    run.indexed_units = indexed_units;
    index.update_run(&run).expect("update run");
}

fn project_file(workspace: &str, relative_path: &str) -> ProjectIndexFile {
    ProjectIndexFile {
        workspace: workspace.to_string(),
        relative_path: relative_path.to_string(),
        classification: DiscoveryFileClassification::Source,
        language: "TypeScript".to_string(),
        extension: ".ts".to_string(),
        size: 10,
        mtime_ms: 1000.0,
        ctime_ms: 1000.0,
        mode: 420,
        observed_hash: None,
        analyzed_hash: None,
        state: ProjectIndexFileState::Discovered,
        excluded: false,
        exclusion_reason: None,
        parser_id: None,
        parser_version: None,
        error: None,
        discovered_at: AT.to_string(),
        analyzed_at: None,
    }
}

fn unit(workspace: &str, id: &str, relative_path: &str) -> SemanticUnit {
    let digest = |character: char| character.to_string().repeat(64);
    SemanticUnit {
        id: id.to_string(),
        workspace: workspace.to_string(),
        relative_path: relative_path.to_string(),
        kind: SemanticUnitKind::Symbol,
        language: Some("TypeScript".to_string()),
        symbol_id: None,
        symbol_name: Some(id.to_string()),
        location: SemanticUnitLocation {
            start_line: 1,
            start_column: 1,
            end_line: 3,
            end_column: 2,
            start_offset: None,
            end_offset: None,
        },
        source_hash: digest('a'),
        chunk_hash: digest('b'),
        normalized_text: format!("text of {id}"),
        chunk_strategy_version: "symbols-v2".to_string(),
        embedding_space: None,
        status: SemanticUnitStatus::LexicalOnly,
        error: None,
        created_at: AT.to_string(),
        updated_at: AT.to_string(),
        indexed_at: None,
    }
}

fn plain(unit: SemanticUnit) -> PersistedSemanticUnit {
    PersistedSemanticUnit {
        unit,
        embedding: None,
    }
}

/// The `stat` of an index of `semantic_units` in `sqlite_stat1`, when the table exists and has it.
fn stat(connection: &Connection, index: &str) -> Option<String> {
    let table_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='sqlite_stat1')",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master");
    if !table_exists {
        return None;
    }
    connection
        .query_row(
            "SELECT stat FROM sqlite_stat1 WHERE tbl='semantic_units' AND idx=?1",
            [index],
            |row| row.get(0),
        )
        .optional()
        .expect("sqlite_stat1")
}

/// The rows per workspace in the statistics of the workspace/status index: the second number of the
/// `stat`. Exact statistics report the average of the workspaces, which is the figure the planner uses.
fn rows_per_workspace(connection: &Connection) -> Option<i64> {
    stat(connection, INDEX).map(|value| {
        value
            .split_whitespace()
            .nth(1)
            .expect("second number of the stat")
            .parse()
            .expect("a number")
    })
}

fn user_version(connection: &Connection) -> i64 {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("user_version")
}

/// What `PRAGMA optimize` stores for a table without statistics, on a separate connection, as the
/// TypeScript build does when it closes a database.
fn optimize_on_its_own(directory: &Path) {
    let connection = Connection::open(directory.join(DATABASE_FILE_NAME)).expect("connection");
    connection
        .execute_batch("PRAGMA optimize;")
        .expect("optimize");
}

#[test]
fn opening_a_database_without_statistics_analyzes_it_in_the_background() {
    let directory = scratch("statistics-open-background");
    {
        let database = Database::open(&directory).expect("open");
        fill(&database, WORKSPACE, 0..10_000);
        // Dropped without close, so no statistics are written.
    }

    let (mut database, recording) = open_recording(&directory);
    assert!(
        database.statistics_pending(),
        "a table of 10,000 rows without statistics starts an analysis when the database opens"
    );
    assert_eq!(database.wait_for_statistics(), StatisticsOutcome::Analyzed);
    assert!(!database.statistics_pending());
    assert_eq!(rows_per_workspace(database.connection()), Some(10_000));
    assert_eq!(
        user_version(database.connection()),
        DATABASE_SCHEMA_VERSION as i64,
        "the analysis is not a migration"
    );
    assert!(recording.entries().is_empty(), "nothing failed");
}

#[test]
fn opening_a_small_database_without_statistics_starts_no_analysis() {
    let directory = scratch("statistics-open-small");
    {
        let database = Database::open(&directory).expect("open");
        fill(&database, WORKSPACE, 0..9_999);
    }

    let database = Database::open(&directory).expect("reopen");
    assert!(
        !database.statistics_pending(),
        "below the threshold, the planner's default is kept"
    );
    assert_eq!(rows_per_workspace(database.connection()), None);
}

#[test]
fn opening_a_database_with_exact_statistics_starts_no_analysis() {
    let directory = scratch("statistics-open-exact");
    let database = Database::open(&directory).expect("open");
    fill(&database, WORKSPACE, 0..10_000);
    database.close().expect("close");

    let (database, _recording) = open_recording(&directory);
    assert!(
        !database.statistics_pending(),
        "exact statistics that match the table are kept"
    );
    assert_eq!(rows_per_workspace(database.connection()), Some(10_000));
}

#[test]
fn opening_a_database_with_sampled_statistics_analyzes_it() {
    let directory = scratch("statistics-open-sampled-one");
    {
        let database = Database::open(&directory).expect("open");
        fill(&database, WORKSPACE, 0..10_000);
    }
    optimize_on_its_own(&directory);
    {
        let connection = Connection::open(directory.join(DATABASE_FILE_NAME)).expect("connection");
        assert_ne!(
            rows_per_workspace(&connection),
            Some(10_000),
            "the scenario holds: PRAGMA optimize stored sampled per-workspace estimates"
        );
    }

    let (mut database, _recording) = open_recording(&directory);
    assert!(
        database.statistics_pending(),
        "the table total is right but the per-workspace figure is sampled: the check still sees it"
    );
    assert_eq!(database.wait_for_statistics(), StatisticsOutcome::Analyzed);
    assert_eq!(rows_per_workspace(database.connection()), Some(10_000));
}

#[test]
fn opening_a_database_sampled_across_ten_workspaces_analyzes_it() {
    let directory = scratch("statistics-open-sampled-ten");
    {
        let database = Database::open(&directory).expect("open");
        for workspace in 0..10 {
            fill(&database, &format!("/work/w{workspace}"), 0..1_000);
        }
    }
    optimize_on_its_own(&directory);
    {
        let connection = Connection::open(directory.join(DATABASE_FILE_NAME)).expect("connection");
        assert_ne!(
            rows_per_workspace(&connection),
            Some(1_000),
            "the scenario holds: the per-workspace estimate is not the exact average"
        );
    }

    let (mut database, _recording) = open_recording(&directory);
    assert!(database.statistics_pending());
    assert_eq!(database.wait_for_statistics(), StatisticsOutcome::Analyzed);
    assert_eq!(rows_per_workspace(database.connection()), Some(1_000));
}

#[test]
fn opening_a_database_with_exact_uneven_workspaces_starts_no_analysis() {
    let directory = scratch("statistics-open-uneven");
    let database = Database::open(&directory).expect("open");
    fill(&database, "/work/a", 0..9_000);
    fill(&database, "/work/b", 0..3_000);
    database.close().expect("close");

    let database = Database::open(&directory).expect("reopen");
    assert!(
        !database.statistics_pending(),
        "the average per workspace is the exact figure, so the check accepts it"
    );
    assert_eq!(rows_per_workspace(database.connection()), Some(6_000));
}

#[test]
fn opening_a_database_whose_statistics_predate_a_doubling_analyzes_it() {
    let directory = scratch("statistics-open-doubled");
    let database = Database::open(&directory).expect("open");
    fill(&database, WORKSPACE, 0..10_000);
    database.close().expect("close");

    {
        let database = Database::open(&directory).expect("reopen");
        assert!(!database.statistics_pending());
        fill(&database, WORKSPACE, 10_000..20_000);
        // Dropped without close: the statistics still describe 10,000 rows.
    }

    let (mut database, _recording) = open_recording(&directory);
    assert!(
        database.statistics_pending(),
        "the stored total no longer matches the table"
    );
    assert_eq!(database.wait_for_statistics(), StatisticsOutcome::Analyzed);
    assert_eq!(rows_per_workspace(database.connection()), Some(20_000));
}

#[test]
fn closing_a_database_records_exact_statistics_before_optimize() {
    let directory = scratch("statistics-close-exact");
    let database = Database::open(&directory).expect("open");
    fill(&database, WORKSPACE, 0..50_000);
    database.close().expect("close");

    let connection = Connection::open(directory.join(DATABASE_FILE_NAME)).expect("connection");
    assert_eq!(
        rows_per_workspace(&connection),
        Some(50_000),
        "the statistics are exact, not a sample"
    );
}

#[test]
fn a_failed_refresh_at_close_is_logged_and_the_next_open_recovers() {
    let directory = scratch("statistics-close-failure");
    let (database, recording) = open_recording(&directory);
    fill(&database, WORKSPACE, 0..10_000);
    // Read-only: the refresh at close cannot run ANALYZE. PRAGMA optimize cannot write either, so the
    // close reports its own failure. The statistics failure itself is only logged.
    database
        .connection()
        .pragma_update(None, "query_only", true)
        .expect("query only");

    let error = database
        .close()
        .expect_err("PRAGMA optimize cannot write in a read-only connection");

    assert!(
        error.to_string().contains("readonly"),
        "the close reports the optimize's failure, not the statistics: {error}"
    );
    let entries = recording.entries();
    assert_eq!(entries.len(), 1, "the failure is logged once: {entries:?}");
    assert!(entries[0].starts_with(EVENT), "{entries:?}");
    assert!(entries[0].contains("on close"), "{entries:?}");

    let (mut database, _recording) = open_recording(&directory);
    assert!(
        database.statistics_pending(),
        "the failed refresh left no statistics"
    );
    assert_eq!(database.wait_for_statistics(), StatisticsOutcome::Analyzed);
    assert_eq!(rows_per_workspace(database.connection()), Some(10_000));
}

#[test]
fn a_large_round_that_ends_refreshes_the_statistics() {
    for name in ["completed", "failed", "cancelled"] {
        let status = match name {
            "completed" => SemanticRunStatus::Completed,
            "failed" => SemanticRunStatus::Failed,
            _ => SemanticRunStatus::Cancelled,
        };
        let directory = scratch(&format!("statistics-round-{name}"));
        let database = Database::open(&directory).expect("open");
        fill(&database, WORKSPACE, 0..10_000);
        assert_eq!(
            rows_per_workspace(database.connection()),
            None,
            "no statistics before the round ends"
        );

        end_run(&database, &format!("run-{name}"), status, 10_000);

        assert_eq!(
            rows_per_workspace(database.connection()),
            Some(10_000),
            "a {name} round of 10,000 units refreshes the statistics"
        );
    }
}

#[test]
fn a_small_round_leaves_the_statistics_alone() {
    let directory = scratch("statistics-round-small");
    let database = Database::open(&directory).expect("open");
    fill(&database, WORKSPACE, 0..100);

    end_run(&database, "run-small", SemanticRunStatus::Completed, 100);

    assert_eq!(rows_per_workspace(database.connection()), None);
}

#[test]
fn a_large_round_still_running_does_not_refresh_the_statistics() {
    let directory = scratch("statistics-round-running");
    let database = Database::open(&directory).expect("open");
    fill(&database, WORKSPACE, 0..10_000);

    end_run(&database, "run-running", SemanticRunStatus::Running, 10_000);

    assert_eq!(rows_per_workspace(database.connection()), None);
}

/// Measurement, not a check: `cargo test --release -p noct-db --test semantic_statistics -- --ignored --nocapture`.
/// Prints what `PRAGMA optimize` alone stores at each size (the state a database closed without an
/// analysis is in), and what the check and an exact `ANALYZE` cost. The figures are in
/// `docs/perf/semantic-index-query-plans.md`.
#[test]
#[ignore = "measurement: run in release mode with --ignored --nocapture"]
fn measure_statistics_by_size() {
    use std::time::Instant;
    for rows in [10_000usize, 50_000, 200_000] {
        let directory = scratch(&format!("statistics-measure-{rows}"));
        let database = Database::open(&directory).expect("open");
        fill(&database, WORKSPACE, 0..rows);
        let connection = database.connection();

        let started = Instant::now();
        connection
            .execute_batch("PRAGMA optimize;")
            .expect("optimize");
        let optimize_ms = started.elapsed().as_millis();
        let sampled = stat(connection, INDEX);

        let started = Instant::now();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM semantic_units", [], |row| row.get(0))
            .expect("count");
        let count_us = started.elapsed().as_micros();

        let started = Instant::now();
        let workspaces: i64 = connection
            .query_row(
                "SELECT COUNT(DISTINCT workspace) FROM semantic_units",
                [],
                |row| row.get(0),
            )
            .expect("distinct");
        let distinct_us = started.elapsed().as_micros();

        let started = Instant::now();
        connection
            .execute_batch("ANALYZE semantic_units;")
            .expect("analyze");
        let analyze_us = started.elapsed().as_micros();
        let exact = stat(connection, INDEX);

        println!(
            "BENCH rows={rows} count={count} workspaces={workspaces} optimize_ms={optimize_ms} \
             sampled=[{}] count_us={count_us} distinct_us={distinct_us} analyze_us={analyze_us} exact=[{}]",
            sampled.unwrap_or_default(),
            exact.unwrap_or_default()
        );
        database.close().expect("close");
    }
}
