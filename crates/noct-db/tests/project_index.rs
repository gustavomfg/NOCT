//! The project index, checked against `ProjectIndexRepository.ts`. The row gate compares the
//! reads and the change scenario with the TypeScript repository; these tests cover the write
//! units, the evidence of a run and the bounds of the queries.

use noct_core::code_intelligence::{
    DetectedStack, DiscoveryExclusion, DiscoveryFileClassification, ImportResolution,
    ProjectExport, ProjectExportKind, ProjectImport, ProjectImportKind, ProjectIndexFile,
    ProjectIndexFileState, ProjectIndexPhase, ProjectIndexRun, ProjectIndexRunKind,
    ProjectIndexRunStatus, ProjectSymbol, ProjectSymbolKind, StackEvidence, StackEvidenceCategory,
    SymbolLocation,
};
use noct_db::Database;
use noct_db::repositories::project_index::{PersistedFileAnalysis, ProjectIndex};

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";
const ALPHA: &str = "/work/alpha";

fn open(name: &str) -> Database {
    let db = Database::open(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) VALUES \
                 ('{ALPHA}','alpha','{AT}','{AT}'),('/work/beta','beta','{AT}','{AT}');"
        ))
        .expect("workspaces");
    db
}

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}

fn file(path: &str, state: ProjectIndexFileState) -> ProjectIndexFile {
    ProjectIndexFile {
        workspace: ALPHA.to_string(),
        relative_path: path.to_string(),
        classification: DiscoveryFileClassification::Source,
        language: "TypeScript".to_string(),
        extension: ".ts".to_string(),
        size: 10,
        mtime_ms: 1000.5,
        ctime_ms: 1000.5,
        mode: 420,
        observed_hash: Some(digest('a')),
        analyzed_hash: Some(digest('a')),
        state,
        excluded: false,
        exclusion_reason: None,
        parser_id: Some("typescript".to_string()),
        parser_version: Some("5".to_string()),
        error: None,
        discovered_at: AT.to_string(),
        analyzed_at: Some(AT.to_string()),
    }
}

fn location(line: i64) -> SymbolLocation {
    SymbolLocation {
        start_line: line,
        start_column: 1,
        end_line: line,
        end_column: 9,
    }
}

fn symbol(id: &str, path: &str, name: &str) -> ProjectSymbol {
    ProjectSymbol {
        id: id.to_string(),
        workspace: ALPHA.to_string(),
        relative_path: path.to_string(),
        analyzed_hash: digest('a'),
        kind: ProjectSymbolKind::Function,
        name: name.to_string(),
        qualified_name: None,
        scope: None,
        signature: None,
        location: location(1),
        exported: false,
        parser_id: "typescript".to_string(),
        parser_version: "5".to_string(),
    }
}

fn import(id: &str, path: &str) -> ProjectImport {
    ProjectImport {
        id: id.to_string(),
        workspace: ALPHA.to_string(),
        source_path: path.to_string(),
        source_hash: digest('a'),
        specifier: "./util".to_string(),
        target_path: None,
        target_hash: None,
        kind: ProjectImportKind::Named,
        imported_names: vec!["helper".to_string()],
        location: location(2),
        resolution: ImportResolution::Unresolved,
    }
}

fn export(id: &str, path: &str) -> ProjectExport {
    ProjectExport {
        id: id.to_string(),
        workspace: ALPHA.to_string(),
        source_path: path.to_string(),
        source_hash: digest('a'),
        name: "main".to_string(),
        kind: ProjectExportKind::Named,
        target_path: None,
        target_hash: None,
        location: location(3),
    }
}

fn analysis(path: &str, symbols: Vec<ProjectSymbol>) -> PersistedFileAnalysis {
    PersistedFileAnalysis {
        file: file(path, ProjectIndexFileState::Indexed),
        symbols,
        imports: vec![import(&format!("imp-{path}"), path)],
        exports: vec![export(&format!("exp-{path}"), path)],
    }
}

fn run(id: &str, status: ProjectIndexRunStatus, updated_at: &str) -> ProjectIndexRun {
    ProjectIndexRun {
        id: id.to_string(),
        workspace: ALPHA.to_string(),
        index_version: 2,
        kind: ProjectIndexRunKind::Initial,
        status,
        phase: ProjectIndexPhase::Parsing,
        total_files: 1,
        processed_files: 0,
        failed_files: 0,
        unsupported_files: 0,
        pending_files: 1,
        started_at: AT.to_string(),
        updated_at: updated_at.to_string(),
        completed_at: None,
        error: None,
    }
}

fn evidence_count(db: &Database, kind: &str) -> i64 {
    db.connection()
        .query_row(
            "SELECT COUNT(*) FROM workspace_evidence WHERE workspace=?1 AND kind=?2",
            [ALPHA, kind],
            |row| row.get(0),
        )
        .expect("count evidence")
}

#[test]
fn a_file_analysis_replaces_the_previous_one_as_one_unit() {
    let db = open("project-replace");
    let index = ProjectIndex::new(db.connection());
    index
        .save_file_analysis(&analysis(
            "src/app.ts",
            vec![
                symbol("s1", "src/app.ts", "a"),
                symbol("s2", "src/app.ts", "b"),
            ],
        ))
        .expect("first analysis");
    index
        .save_file_analysis(&analysis(
            "src/app.ts",
            vec![symbol("s3", "src/app.ts", "c")],
        ))
        .expect("second analysis");

    let symbols = index
        .list_symbols_for_file(ALPHA, "src/app.ts", None)
        .expect("symbols");
    assert_eq!(
        symbols
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["s3"]
    );
    assert_eq!(
        index
            .list_imports(ALPHA, Some("src/app.ts"))
            .expect("imports")
            .len(),
        1
    );
    assert_eq!(
        index
            .list_exports(ALPHA, Some("src/app.ts"))
            .expect("exports")
            .len(),
        1
    );
}

#[test]
fn removing_a_file_removes_its_symbols_imports_and_exports() {
    let db = open("project-cascade");
    let index = ProjectIndex::new(db.connection());
    index
        .save_file_analysis(&analysis(
            "src/old.ts",
            vec![symbol("s1", "src/old.ts", "legacy")],
        ))
        .expect("analysis");
    index
        .upsert_file(&file("src/kept.ts", ProjectIndexFileState::Indexed))
        .expect("kept");

    index
        .remove_missing_files(ALPHA, &["src/kept.ts".to_string()])
        .expect("remove");

    assert!(index.get_file(ALPHA, "src/old.ts").expect("get").is_none());
    assert!(
        index
            .list_symbols(ALPHA, "", None)
            .expect("symbols")
            .is_empty()
    );
    assert!(index.list_imports(ALPHA, None).expect("imports").is_empty());
    assert!(index.list_exports(ALPHA, None).expect("exports").is_empty());
    assert!(index.get_file(ALPHA, "src/kept.ts").expect("get").is_some());
}

#[test]
fn only_a_run_that_ends_is_recorded_as_evidence() {
    let db = open("project-evidence");
    let index = ProjectIndex::new(db.connection());
    index
        .upsert_file(&file("src/app.ts", ProjectIndexFileState::Indexed))
        .expect("file");
    index
        .create_run(&run("pix-1", ProjectIndexRunStatus::Queued, AT))
        .expect("create");
    index
        .update_run(&run(
            "pix-1",
            ProjectIndexRunStatus::Running,
            "2026-01-01T00:00:01.000Z",
        ))
        .expect("running");
    assert_eq!(evidence_count(&db, "project-index"), 0);

    index
        .update_run(&run(
            "pix-1",
            ProjectIndexRunStatus::Completed,
            "2026-01-01T00:00:02.000Z",
        ))
        .expect("completed");
    assert_eq!(evidence_count(&db, "project-index"), 1);

    let latest = index.latest_run(ALPHA).expect("latest").expect("a run");
    assert_eq!(latest.id, "pix-1");
    assert_eq!(latest.status, ProjectIndexRunStatus::Completed);
}

#[test]
fn the_stack_is_named_by_the_last_path_segment_and_reads_its_commands() {
    let db = open("project-stack");
    let index = ProjectIndex::new(db.connection());
    let evidence =
        |id: &str, category: StackEvidenceCategory, value: &str, at: &str| StackEvidence {
            id: id.to_string(),
            workspace: ALPHA.to_string(),
            category,
            value: value.to_string(),
            confidence: 90,
            source_path: "package.json".to_string(),
            source_hash: digest('d'),
            source_line: None,
            reason: format!("motivo {id}"),
            detected_at: at.to_string(),
        };
    index
        .replace_stack_evidence(
            ALPHA,
            &[
                evidence(
                    "a",
                    StackEvidenceCategory::Language,
                    "TypeScript",
                    "2026-01-01T00:00:01.000Z",
                ),
                evidence(
                    "b",
                    StackEvidenceCategory::Script,
                    "test=vitest run=now",
                    "2026-01-01T00:00:03.000Z",
                ),
                evidence(
                    "c",
                    StackEvidenceCategory::Script,
                    "lint",
                    "2026-01-01T00:00:02.000Z",
                ),
                evidence(
                    "d",
                    StackEvidenceCategory::Convention,
                    "ESLint",
                    "2026-01-01T00:00:09.000Z",
                ),
            ],
        )
        .expect("stack evidence");

    let summary = index.summary(ALPHA).expect("summary");
    let stack: DetectedStack = summary.stack.expect("a stack");
    assert_eq!(stack.name, "alpha");
    assert_eq!(stack.primary_language, "TypeScript");
    assert_eq!(
        stack.commands.get("test").map(String::as_str),
        Some("vitest run=now")
    );
    assert_eq!(stack.commands.get("lint").map(String::as_str), Some("lint"));
    assert_eq!(
        stack.stack,
        Vec::<String>::new(),
        "conventions are not part of the stack"
    );
    assert_eq!(stack.detected_at, "2026-01-01T00:00:09.000Z");
}

#[test]
fn a_windows_workspace_path_is_named_by_its_last_segment() {
    let db = open("project-name");
    let index = ProjectIndex::new(db.connection());
    db.connection()
        .execute(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) VALUES('C:\\work\\beta\\','beta',?1,?1)",
            [AT],
        )
        .expect("workspace");
    index
        .replace_stack_evidence(
            "C:\\work\\beta\\",
            &[StackEvidence {
                id: "w".to_string(),
                workspace: "C:\\work\\beta\\".to_string(),
                category: StackEvidenceCategory::Language,
                value: "Rust".to_string(),
                confidence: 100,
                source_path: "Cargo.toml".to_string(),
                source_hash: digest('e'),
                source_line: None,
                reason: "manifesto".to_string(),
                detected_at: AT.to_string(),
            }],
        )
        .expect("evidence");
    assert_eq!(
        index
            .summary("C:\\work\\beta\\")
            .expect("summary")
            .stack
            .expect("stack")
            .name,
        "beta"
    );
}

#[test]
fn symbol_queries_ignore_case_and_keep_their_limits() {
    let db = open("project-symbols");
    let index = ProjectIndex::new(db.connection());
    let mut app = symbol("s1", "src/app.ts", "App");
    app.qualified_name = Some("App.run".to_string());
    index
        .save_file_analysis(&analysis(
            "src/app.ts",
            vec![app, symbol("s2", "src/app.ts", "helper")],
        ))
        .expect("analysis");

    let by_qualified = index.list_symbols(ALPHA, "  RUN  ", None).expect("query");
    assert_eq!(
        by_qualified
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["s1"]
    );
    assert_eq!(
        index
            .list_symbols(ALPHA, "", Some(0))
            .expect("low limit")
            .len(),
        1
    );
    assert_eq!(
        index
            .list_symbols(ALPHA, "", Some(5_000))
            .expect("high limit")
            .len(),
        2
    );
}

#[test]
fn a_corrupt_stored_kind_fails_the_read() {
    let db = open("project-corrupt");
    let index = ProjectIndex::new(db.connection());
    index
        .upsert_file(&file("src/app.ts", ProjectIndexFileState::Indexed))
        .expect("file");
    db.connection()
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;\
             UPDATE project_index_files SET state='bogus' WHERE relative_path='src/app.ts';",
        )
        .expect("corrupt the row");

    assert!(index.list_files(ALPHA, None).is_err());
}

#[test]
fn exclusions_replace_or_merge_by_path() {
    let db = open("project-exclusions");
    let index = ProjectIndex::new(db.connection());
    let excluded = |path: &str, reason: &str| DiscoveryExclusion {
        relative_path: path.to_string(),
        reason: reason.to_string(),
    };
    index
        .replace_exclusions(
            ALPHA,
            &[excluded("dist", "build"), excluded("tmp", "cache")],
            Some(AT),
        )
        .expect("replace");
    index
        .upsert_exclusions(
            ALPHA,
            &[
                excluded("dist", "generated"),
                excluded("coverage", "generated"),
            ],
            Some(AT),
        )
        .expect("upsert");

    let listed: Vec<(String, String)> = index
        .list_exclusions(ALPHA)
        .expect("list")
        .into_iter()
        .map(|item| (item.relative_path, item.reason))
        .collect();
    assert_eq!(
        listed,
        [
            ("coverage".to_string(), "generated".to_string()),
            ("dist".to_string(), "generated".to_string()),
            ("tmp".to_string(), "cache".to_string()),
        ]
    );
}
