//! The project index: files, runs, symbols and relations of a workspace, with its stack evidence
//! and exclusions. Port of `electron/database/ProjectIndexRepository.ts`.
//!
//! The index keeps derived knowledge and never source contents. A file's symbols, imports and
//! exports are replaced as one unit, and a run that ends is recorded as workspace evidence.

use std::collections::{BTreeMap, HashSet};

use noct_core::code_intelligence::{
    DetectedStack, DiscoveryExclusion, INDEX_VERSION, ProjectExport, ProjectImport,
    ProjectIndexFile, ProjectIndexPhase, ProjectIndexRun, ProjectIndexRunKind,
    ProjectIndexRunStatus, ProjectIndexSummary, ProjectSymbol, StackEvidence,
    StackEvidenceCategory, SymbolLocation,
};
use noct_core::limits::code_intelligence::{MAX_FILES, MAX_QUERY_RESULTS};
use noct_core::text::js_trim;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::{Connection, Row, params, params_from_iter};

use crate::clock::iso_now;
use crate::codec::{Stored, enum_text};
use crate::repositories::workspace_evidence::WorkspaceEvidence;
use crate::{Error, Result, transaction};

const INVALID_IMPORTS: &str = "A relação persistida possui nomes importados inválidos.";
const SYMBOLS_FOR_FILE_LIMIT: usize = 2_000;
const RUNS_DEFAULT_LIMIT: usize = 20;
const SYMBOLS_DEFAULT_LIMIT: usize = 100;

const RUN_COLUMNS: &str = "id, workspace, index_version, kind, status, phase, total_files, \
    processed_files, failed_files, unsupported_files, pending_files, started_at, updated_at, \
    completed_at, error";
const FILE_COLUMNS: &str = "workspace, relative_path, classification, language, extension, size, \
    mtime_ms, ctime_ms, mode, observed_hash, analyzed_hash, state, excluded, exclusion_reason, \
    parser_id, parser_version, error, discovered_at, analyzed_at";
const SYMBOL_COLUMNS: &str = "id, workspace, relative_path, analyzed_hash, kind, name, \
    qualified_name, scope, signature, start_line, start_column, end_line, end_column, exported, \
    parser_id, parser_version";
const IMPORT_COLUMNS: &str = "id, workspace, source_path, source_hash, specifier, target_path, \
    target_hash, kind, imported_names, start_line, start_column, end_line, end_column, resolution";
const EXPORT_COLUMNS: &str = "id, workspace, source_path, source_hash, name, kind, target_path, \
    target_hash, start_line, start_column, end_line, end_column";
const STACK_COLUMNS: &str = "id, workspace, category, value, confidence, source_path, \
    source_hash, source_line, reason, detected_at";

/// The categories that make up the detected stack, as the TypeScript repository lists them.
const STACK_CATEGORIES: [StackEvidenceCategory; 8] = [
    StackEvidenceCategory::Runtime,
    StackEvidenceCategory::Framework,
    StackEvidenceCategory::Bundler,
    StackEvidenceCategory::PackageManager,
    StackEvidenceCategory::Typecheck,
    StackEvidenceCategory::Lint,
    StackEvidenceCategory::Test,
    StackEvidenceCategory::Build,
];

/// A file with the symbols, imports and exports found in its analyzed content.
#[derive(Debug, Clone, PartialEq)]
pub struct PersistedFileAnalysis {
    pub file: ProjectIndexFile,
    pub symbols: Vec<ProjectSymbol>,
    pub imports: Vec<ProjectImport>,
    pub exports: Vec<ProjectExport>,
}

pub struct ProjectIndex<'a> {
    conn: &'a Connection,
}

impl<'a> ProjectIndex<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create_run(&self, run: &ProjectIndexRun) -> Result<()> {
        self.conn.execute(
            "INSERT INTO project_index_runs(id,workspace,index_version,kind,status,phase,\
             total_files,processed_files,failed_files,unsupported_files,pending_files,started_at,\
             updated_at,completed_at,error) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                run.id,
                run.workspace,
                run.index_version,
                enum_text(&run.kind)?,
                enum_text(&run.status)?,
                enum_text(&run.phase)?,
                run.total_files,
                run.processed_files,
                run.failed_files,
                run.unsupported_files,
                run.pending_files,
                run.started_at,
                run.updated_at,
                run.completed_at,
                run.error,
            ],
        )?;
        Ok(())
    }

    /// Updates a run. A run that ends is first recorded as workspace evidence, in the same unit.
    pub fn update_run(&self, run: &ProjectIndexRun) -> Result<()> {
        transaction::run(self.conn, |conn| {
            if matches!(
                run.status,
                ProjectIndexRunStatus::Completed
                    | ProjectIndexRunStatus::Cancelled
                    | ProjectIndexRunStatus::Failed
            ) {
                WorkspaceEvidence::new(conn).record_project_index(
                    &run.workspace,
                    &run.id,
                    &run.started_at,
                )?;
            }
            conn.execute(
                "UPDATE project_index_runs SET status=?1,phase=?2,total_files=?3,\
                 processed_files=?4,failed_files=?5,unsupported_files=?6,pending_files=?7,\
                 updated_at=?8,completed_at=?9,error=?10 WHERE id=?11 AND workspace=?12",
                params![
                    enum_text(&run.status)?,
                    enum_text(&run.phase)?,
                    run.total_files,
                    run.processed_files,
                    run.failed_files,
                    run.unsupported_files,
                    run.pending_files,
                    run.updated_at,
                    run.completed_at,
                    run.error,
                    run.id,
                    run.workspace,
                ],
            )?;
            Ok(())
        })
    }

    pub fn latest_run(&self, workspace: &str) -> Result<Option<ProjectIndexRun>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM project_index_runs WHERE workspace=?1 \
             ORDER BY updated_at DESC LIMIT 1"
        ))?;
        let mut rows = statement.query_map([workspace], run_row)?;
        rows.next().transpose().map_err(Error::from)
    }

    /// The most recent runs of a workspace. The limit is kept between 1 and 100.
    pub fn list_runs(&self, workspace: &str, limit: Option<usize>) -> Result<Vec<ProjectIndexRun>> {
        let limit = limit
            .unwrap_or(RUNS_DEFAULT_LIMIT)
            .clamp(1, MAX_QUERY_RESULTS as usize);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM project_index_runs WHERE workspace=?1 \
             ORDER BY updated_at DESC LIMIT ?2"
        ))?;
        let rows = statement.query_map(params![workspace, limit as i64], run_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    /// The files of a workspace, ordered by path. The limit is kept between 1 and 50,000.
    pub fn list_files(
        &self,
        workspace: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectIndexFile>> {
        let limit = limit
            .unwrap_or(MAX_FILES as usize)
            .clamp(1, MAX_FILES as usize);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {FILE_COLUMNS} FROM project_index_files WHERE workspace=?1 \
             ORDER BY relative_path LIMIT ?2"
        ))?;
        let rows = statement.query_map(params![workspace, limit as i64], file_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    pub fn get_file(
        &self,
        workspace: &str,
        relative_path: &str,
    ) -> Result<Option<ProjectIndexFile>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {FILE_COLUMNS} FROM project_index_files WHERE workspace=?1 AND relative_path=?2"
        ))?;
        let mut rows = statement.query_map(params![workspace, relative_path], file_row)?;
        rows.next().transpose().map_err(Error::from)
    }

    pub fn upsert_file(&self, file: &ProjectIndexFile) -> Result<()> {
        upsert_file(self.conn, file)
    }

    /// Saves a file and its analysis as one unit: the file is upserted and its previous symbols,
    /// imports and exports are replaced.
    pub fn save_file_analysis(&self, value: &PersistedFileAnalysis) -> Result<()> {
        transaction::run(self.conn, |conn| {
            upsert_file(conn, &value.file)?;
            let path = value.file.relative_path.as_str();
            conn.execute(
                "DELETE FROM project_index_symbols WHERE workspace=?1 AND relative_path=?2",
                params![value.file.workspace, path],
            )?;
            conn.execute(
                "DELETE FROM project_index_imports WHERE workspace=?1 AND source_path=?2",
                params![value.file.workspace, path],
            )?;
            conn.execute(
                "DELETE FROM project_index_exports WHERE workspace=?1 AND source_path=?2",
                params![value.file.workspace, path],
            )?;
            let mut symbols = conn.prepare(
                "INSERT INTO project_index_symbols(id,workspace,relative_path,analyzed_hash,kind,\
                 name,qualified_name,scope,signature,start_line,start_column,end_line,end_column,\
                 exported,parser_id,parser_version) \
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            )?;
            for symbol in &value.symbols {
                symbols.execute(params![
                    symbol.id,
                    symbol.workspace,
                    symbol.relative_path,
                    symbol.analyzed_hash,
                    enum_text(&symbol.kind)?,
                    symbol.name,
                    symbol.qualified_name,
                    symbol.scope,
                    symbol.signature,
                    symbol.location.start_line,
                    symbol.location.start_column,
                    symbol.location.end_line,
                    symbol.location.end_column,
                    symbol.exported,
                    symbol.parser_id,
                    symbol.parser_version,
                ])?;
            }
            let mut imports = conn.prepare(
                "INSERT INTO project_index_imports(id,workspace,source_path,source_hash,specifier,\
                 target_path,target_hash,kind,imported_names,start_line,start_column,end_line,\
                 end_column,resolution) \
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            )?;
            for relation in &value.imports {
                imports.execute(params![
                    relation.id,
                    relation.workspace,
                    relation.source_path,
                    relation.source_hash,
                    relation.specifier,
                    relation.target_path,
                    relation.target_hash,
                    enum_text(&relation.kind)?,
                    serde_json::to_string(&relation.imported_names)
                        .map_err(|error| Error::Message(error.to_string()))?,
                    relation.location.start_line,
                    relation.location.start_column,
                    relation.location.end_line,
                    relation.location.end_column,
                    enum_text(&relation.resolution)?,
                ])?;
            }
            let mut exports = conn.prepare(
                "INSERT INTO project_index_exports(id,workspace,source_path,source_hash,name,kind,\
                 target_path,target_hash,start_line,start_column,end_line,end_column) \
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            )?;
            for relation in &value.exports {
                exports.execute(params![
                    relation.id,
                    relation.workspace,
                    relation.source_path,
                    relation.source_hash,
                    relation.name,
                    enum_text(&relation.kind)?,
                    relation.target_path,
                    relation.target_hash,
                    relation.location.start_line,
                    relation.location.start_column,
                    relation.location.end_line,
                    relation.location.end_column,
                ])?;
            }
            Ok(())
        })
    }

    pub fn mark_file_state(&self, file: &ProjectIndexFile) -> Result<()> {
        transaction::run(self.conn, |conn| upsert_file(conn, file))
    }

    /// Removes the files of a workspace that are not among the present paths. Without any present
    /// path, every file of the workspace goes.
    pub fn remove_missing_files(&self, workspace: &str, present_paths: &[String]) -> Result<()> {
        transaction::run(self.conn, |conn| {
            if present_paths.is_empty() {
                conn.execute(
                    "DELETE FROM project_index_files WHERE workspace=?1",
                    [workspace],
                )?;
                return Ok(());
            }
            let placeholders = (2..=present_paths.len() + 1)
                .map(|index| format!("?{index}"))
                .collect::<Vec<_>>()
                .join(",");
            let values: Vec<String> = std::iter::once(workspace.to_string())
                .chain(present_paths.iter().cloned())
                .collect();
            conn.execute(
                &format!(
                    "DELETE FROM project_index_files WHERE workspace=?1 \
                     AND relative_path NOT IN ({placeholders})"
                ),
                params_from_iter(values),
            )?;
            Ok(())
        })
    }

    /// Replaces the stack evidence of a workspace.
    pub fn replace_stack_evidence(
        &self,
        workspace: &str,
        evidence: &[StackEvidence],
    ) -> Result<()> {
        transaction::run(self.conn, |conn| {
            conn.execute(
                "DELETE FROM project_stack_evidence WHERE workspace=?1",
                [workspace],
            )?;
            let mut statement = conn.prepare(
                "INSERT INTO project_stack_evidence(id,workspace,category,value,confidence,\
                 source_path,source_hash,source_line,reason,detected_at) \
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            )?;
            for item in evidence {
                statement.execute(params![
                    item.id,
                    item.workspace,
                    enum_text(&item.category)?,
                    item.value,
                    item.confidence,
                    item.source_path,
                    item.source_hash,
                    item.source_line,
                    item.reason,
                    item.detected_at,
                ])?;
            }
            Ok(())
        })
    }

    /// Replaces the exclusions of a workspace. The detection time defaults to now.
    pub fn replace_exclusions(
        &self,
        workspace: &str,
        exclusions: &[DiscoveryExclusion],
        detected_at: Option<&str>,
    ) -> Result<()> {
        let detected_at = detection_time(detected_at)?;
        transaction::run(self.conn, |conn| {
            conn.execute(
                "DELETE FROM project_index_exclusions WHERE workspace=?1",
                [workspace],
            )?;
            let mut statement = conn.prepare(
                "INSERT INTO project_index_exclusions(workspace,relative_path,reason,detected_at) \
                 VALUES(?1,?2,?3,?4)",
            )?;
            for item in exclusions {
                statement.execute(params![
                    workspace,
                    item.relative_path,
                    item.reason,
                    detected_at
                ])?;
            }
            Ok(())
        })
    }

    /// Adds or updates exclusions, and keeps the others.
    pub fn upsert_exclusions(
        &self,
        workspace: &str,
        exclusions: &[DiscoveryExclusion],
        detected_at: Option<&str>,
    ) -> Result<()> {
        let detected_at = detection_time(detected_at)?;
        transaction::run(self.conn, |conn| {
            let mut statement = conn.prepare(
                "INSERT INTO project_index_exclusions(workspace,relative_path,reason,detected_at) \
                 VALUES(?1,?2,?3,?4) ON CONFLICT(workspace,relative_path) DO UPDATE SET \
                 reason=excluded.reason,detected_at=excluded.detected_at",
            )?;
            for item in exclusions {
                statement.execute(params![
                    workspace,
                    item.relative_path,
                    item.reason,
                    detected_at
                ])?;
            }
            Ok(())
        })
    }

    /// After a change to some paths, removes the indexed files under them that are gone. A path
    /// names a file or a directory, and separators and a leading `./` or trailing `/` are ignored.
    pub fn remove_missing_for_change(
        &self,
        workspace: &str,
        requested_paths: &[String],
        present_paths: &[String],
    ) -> Result<()> {
        transaction::run(self.conn, |conn| {
            let existing: Vec<String> = {
                let mut statement = conn
                    .prepare("SELECT relative_path FROM project_index_files WHERE workspace=?1")?;
                statement
                    .query_map([workspace], |row| row.get(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            };
            let present: HashSet<&str> = present_paths.iter().map(String::as_str).collect();
            let requested: Vec<String> = requested_paths
                .iter()
                .filter(|item| !item.is_empty())
                .map(|item| normalize_requested(item))
                .collect();
            let mut statement = conn.prepare(
                "DELETE FROM project_index_files WHERE workspace=?1 AND relative_path=?2",
            )?;
            for path in &existing {
                let under_change = requested
                    .iter()
                    .any(|item| path == item || path.starts_with(&format!("{item}/")));
                if under_change && !present.contains(path.as_str()) {
                    statement.execute(params![workspace, path])?;
                }
            }
            Ok(())
        })
    }

    pub fn list_exclusions(&self, workspace: &str) -> Result<Vec<DiscoveryExclusion>> {
        let mut statement = self.conn.prepare(
            "SELECT relative_path, reason FROM project_index_exclusions WHERE workspace=?1 \
             ORDER BY relative_path",
        )?;
        let rows = statement.query_map([workspace], |row| {
            Ok(DiscoveryExclusion {
                relative_path: row.get(0)?,
                reason: row.get(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    pub fn list_stack_evidence(&self, workspace: &str) -> Result<Vec<StackEvidence>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {STACK_COLUMNS} FROM project_stack_evidence WHERE workspace=?1 \
             ORDER BY category, value, source_path"
        ))?;
        let rows = statement.query_map([workspace], stack_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    /// Symbols of a workspace whose name or qualified name contains the query, without case.
    /// An empty query lists them all. The limit is kept between 1 and 100.
    pub fn list_symbols(
        &self,
        workspace: &str,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectSymbol>> {
        let query = js_trim(query);
        let limit = limit
            .unwrap_or(SYMBOLS_DEFAULT_LIMIT)
            .clamp(1, MAX_QUERY_RESULTS as usize);
        let pattern = format!("%{query}%");
        let mut statement = self.conn.prepare(&format!(
            "SELECT {SYMBOL_COLUMNS} FROM project_index_symbols \
             WHERE workspace=?1 AND (?2='' OR lower(name) LIKE lower(?3) \
               OR lower(COALESCE(qualified_name,'')) LIKE lower(?3)) \
             ORDER BY relative_path, start_line, name LIMIT ?4"
        ))?;
        let rows =
            statement.query_map(params![workspace, query, pattern, limit as i64], symbol_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    /// The symbols of one file, in source order. The limit is kept between 1 and 2,000.
    pub fn list_symbols_for_file(
        &self,
        workspace: &str,
        relative_path: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectSymbol>> {
        let limit = limit
            .unwrap_or(SYMBOLS_FOR_FILE_LIMIT)
            .clamp(1, SYMBOLS_FOR_FILE_LIMIT);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {SYMBOL_COLUMNS} FROM project_index_symbols WHERE workspace=?1 \
             AND relative_path=?2 ORDER BY start_line, start_column, name LIMIT ?3"
        ))?;
        let rows =
            statement.query_map(params![workspace, relative_path, limit as i64], symbol_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    /// The imports of a workspace, or only those of one file.
    pub fn list_imports(
        &self,
        workspace: &str,
        relative_path: Option<&str>,
    ) -> Result<Vec<ProjectImport>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {IMPORT_COLUMNS} FROM project_index_imports WHERE workspace=?1 \
             AND (?2 IS NULL OR source_path=?2) ORDER BY source_path, start_line"
        ))?;
        let rows = statement.query_map(params![workspace, relative_path], import_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    /// The exports of a workspace, or only those of one file.
    pub fn list_exports(
        &self,
        workspace: &str,
        relative_path: Option<&str>,
    ) -> Result<Vec<ProjectExport>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {EXPORT_COLUMNS} FROM project_index_exports WHERE workspace=?1 \
             AND (?2 IS NULL OR source_path=?2) ORDER BY source_path, start_line"
        ))?;
        let rows = statement.query_map(params![workspace, relative_path], export_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    /// Copies the current hash of each local target into the relations that point at it.
    pub fn refresh_relation_hashes(&self, workspace: &str) -> Result<()> {
        transaction::run(self.conn, |conn| {
            conn.execute(
                "UPDATE project_index_imports SET target_hash=(\
                   SELECT analyzed_hash FROM project_index_files target \
                   WHERE target.workspace=project_index_imports.workspace \
                     AND target.relative_path=project_index_imports.target_path\
                 ) WHERE workspace=?1 AND target_path IS NOT NULL",
                [workspace],
            )?;
            conn.execute(
                "UPDATE project_index_exports SET target_hash=(\
                   SELECT analyzed_hash FROM project_index_files target \
                   WHERE target.workspace=project_index_exports.workspace \
                     AND target.relative_path=project_index_exports.target_path\
                 ) WHERE workspace=?1 AND target_path IS NOT NULL",
                [workspace],
            )?;
            Ok(())
        })
    }

    pub fn summary(&self, workspace: &str) -> Result<ProjectIndexSummary> {
        let (files, indexed_files, failed_files, unsupported_files) = self.conn.query_row(
            "SELECT COUNT(*), \
               SUM(CASE WHEN state='indexed' THEN 1 ELSE 0 END), \
               SUM(CASE WHEN state='failed' THEN 1 ELSE 0 END), \
               SUM(CASE WHEN state='unsupported' THEN 1 ELSE 0 END) \
             FROM project_index_files WHERE workspace=?1",
            [workspace],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                ))
            },
        )?;
        let count = |table: &str| -> Result<i64> {
            Ok(self.conn.query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE workspace=?1"),
                [workspace],
                |row| row.get(0),
            )?)
        };
        let evidence = self.list_stack_evidence(workspace)?;
        let stack = (!evidence.is_empty()).then(|| detected_stack(workspace, evidence));
        Ok(ProjectIndexSummary {
            workspace: workspace.to_string(),
            index_version: INDEX_VERSION,
            latest_run: self.latest_run(workspace)?,
            files,
            indexed_files,
            failed_files,
            unsupported_files,
            symbols: count("project_index_symbols")?,
            imports: count("project_index_imports")?,
            exports: count("project_index_exports")?,
            stack,
        })
    }
}

fn upsert_file(conn: &Connection, file: &ProjectIndexFile) -> Result<()> {
    conn.execute(
        "INSERT INTO project_index_files(workspace,relative_path,classification,language,\
         extension,size,mtime_ms,ctime_ms,mode,observed_hash,analyzed_hash,state,excluded,\
         exclusion_reason,parser_id,parser_version,error,discovered_at,analyzed_at) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19) \
         ON CONFLICT(workspace,relative_path) DO UPDATE SET \
         classification=excluded.classification,language=excluded.language,\
         extension=excluded.extension,size=excluded.size,mtime_ms=excluded.mtime_ms,\
         ctime_ms=excluded.ctime_ms,mode=excluded.mode,observed_hash=excluded.observed_hash,\
         analyzed_hash=excluded.analyzed_hash,state=excluded.state,excluded=excluded.excluded,\
         exclusion_reason=excluded.exclusion_reason,parser_id=excluded.parser_id,\
         parser_version=excluded.parser_version,error=excluded.error,\
         discovered_at=excluded.discovered_at,analyzed_at=excluded.analyzed_at",
        params![
            file.workspace,
            file.relative_path,
            enum_text(&file.classification)?,
            file.language,
            file.extension,
            file.size,
            file.mtime_ms,
            file.ctime_ms,
            file.mode,
            file.observed_hash,
            file.analyzed_hash,
            enum_text(&file.state)?,
            file.excluded,
            file.exclusion_reason,
            file.parser_id,
            file.parser_version,
            file.error,
            file.discovered_at,
            file.analyzed_at,
        ],
    )?;
    Ok(())
}

fn detection_time(value: Option<&str>) -> Result<String> {
    match value {
        Some(value) => Ok(value.to_string()),
        None => iso_now(),
    }
}

/// A path as the TypeScript repository compares it: separators become `/`, a leading `./` and a
/// trailing `/` are removed.
fn normalize_requested(value: &str) -> String {
    let slashes = value.replace('\\', "/");
    let relative = slashes.strip_prefix("./").unwrap_or(&slashes);
    relative.strip_suffix('/').unwrap_or(relative).to_string()
}

/// The imported names, stored as a JSON array of strings.
struct ImportedNames(Vec<String>);

impl FromSql for ImportedNames {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        serde_json::from_str::<Vec<String>>(value.as_str()?)
            .map(ImportedNames)
            .map_err(|_| FromSqlError::Other(INVALID_IMPORTS.into()))
    }
}

fn location(row: &Row<'_>, first: usize) -> rusqlite::Result<SymbolLocation> {
    Ok(SymbolLocation {
        start_line: row.get(first)?,
        start_column: row.get(first + 1)?,
        end_line: row.get(first + 2)?,
        end_column: row.get(first + 3)?,
    })
}

fn run_row(row: &Row<'_>) -> rusqlite::Result<ProjectIndexRun> {
    Ok(ProjectIndexRun {
        id: row.get(0)?,
        workspace: row.get(1)?,
        index_version: row.get(2)?,
        kind: row.get::<_, Stored<ProjectIndexRunKind>>(3)?.0,
        status: row.get::<_, Stored<ProjectIndexRunStatus>>(4)?.0,
        phase: row.get::<_, Stored<ProjectIndexPhase>>(5)?.0,
        total_files: row.get(6)?,
        processed_files: row.get(7)?,
        failed_files: row.get(8)?,
        unsupported_files: row.get(9)?,
        pending_files: row.get(10)?,
        started_at: row.get(11)?,
        updated_at: row.get(12)?,
        completed_at: row.get(13)?,
        error: row.get(14)?,
    })
}

fn file_row(row: &Row<'_>) -> rusqlite::Result<ProjectIndexFile> {
    Ok(ProjectIndexFile {
        workspace: row.get(0)?,
        relative_path: row.get(1)?,
        classification: row.get::<_, Stored<_>>(2)?.0,
        language: row.get(3)?,
        extension: row.get(4)?,
        size: row.get(5)?,
        mtime_ms: row.get(6)?,
        ctime_ms: row.get(7)?,
        mode: row.get(8)?,
        observed_hash: row.get(9)?,
        analyzed_hash: row.get(10)?,
        state: row.get::<_, Stored<_>>(11)?.0,
        excluded: row.get::<_, i64>(12)? != 0,
        exclusion_reason: row.get(13)?,
        parser_id: row.get(14)?,
        parser_version: row.get(15)?,
        error: row.get(16)?,
        discovered_at: row.get(17)?,
        analyzed_at: row.get(18)?,
    })
}

fn symbol_row(row: &Row<'_>) -> rusqlite::Result<ProjectSymbol> {
    Ok(ProjectSymbol {
        id: row.get(0)?,
        workspace: row.get(1)?,
        relative_path: row.get(2)?,
        analyzed_hash: row.get(3)?,
        kind: row.get::<_, Stored<_>>(4)?.0,
        name: row.get(5)?,
        qualified_name: row.get(6)?,
        scope: row.get(7)?,
        signature: row.get(8)?,
        location: location(row, 9)?,
        exported: row.get::<_, i64>(13)? != 0,
        parser_id: row.get(14)?,
        parser_version: row.get(15)?,
    })
}

fn import_row(row: &Row<'_>) -> rusqlite::Result<ProjectImport> {
    Ok(ProjectImport {
        id: row.get(0)?,
        workspace: row.get(1)?,
        source_path: row.get(2)?,
        source_hash: row.get(3)?,
        specifier: row.get(4)?,
        target_path: row.get(5)?,
        target_hash: row.get(6)?,
        kind: row.get::<_, Stored<_>>(7)?.0,
        imported_names: row.get::<_, ImportedNames>(8)?.0,
        location: location(row, 9)?,
        resolution: row.get::<_, Stored<_>>(13)?.0,
    })
}

fn export_row(row: &Row<'_>) -> rusqlite::Result<ProjectExport> {
    Ok(ProjectExport {
        id: row.get(0)?,
        workspace: row.get(1)?,
        source_path: row.get(2)?,
        source_hash: row.get(3)?,
        name: row.get(4)?,
        kind: row.get::<_, Stored<_>>(5)?.0,
        target_path: row.get(6)?,
        target_hash: row.get(7)?,
        location: location(row, 8)?,
    })
}

fn stack_row(row: &Row<'_>) -> rusqlite::Result<StackEvidence> {
    Ok(StackEvidence {
        id: row.get(0)?,
        workspace: row.get(1)?,
        category: row.get::<_, Stored<_>>(2)?.0,
        value: row.get(3)?,
        confidence: row.get(4)?,
        source_path: row.get(5)?,
        source_hash: row.get(6)?,
        source_line: row.get(7)?,
        reason: row.get(8)?,
        detected_at: row.get(9)?,
    })
}

/// The stack of a workspace, derived from its evidence. The name is the last segment of the
/// workspace path, the primary language is the first language found, and the commands come from
/// the `name=command` script evidence.
fn detected_stack(workspace: &str, evidence: Vec<StackEvidence>) -> DetectedStack {
    let values = |category: StackEvidenceCategory| -> Vec<String> {
        unique(
            evidence
                .iter()
                .filter(|item| item.category == category)
                .map(|item| item.value.as_str()),
        )
    };
    let languages = values(StackEvidenceCategory::Language);
    let stack = unique(
        evidence
            .iter()
            .filter(|item| STACK_CATEGORIES.contains(&item.category))
            .map(|item| item.value.as_str()),
    );
    let mut commands = BTreeMap::new();
    for item in evidence
        .iter()
        .filter(|item| item.category == StackEvidenceCategory::Script)
    {
        let (name, command) = match item.value.find('=') {
            Some(index) if index > 0 => (
                item.value[..index].to_string(),
                item.value[index + 1..].to_string(),
            ),
            _ => (item.value.clone(), item.value.clone()),
        };
        commands.insert(name, command);
    }
    let name = workspace
        .split(['\\', '/'])
        .rfind(|part| !part.is_empty())
        .unwrap_or(workspace)
        .to_string();
    let detected_at = evidence.iter().fold(String::new(), |latest, item| {
        if item.detected_at > latest {
            item.detected_at.clone()
        } else {
            latest
        }
    });
    DetectedStack {
        name,
        stack,
        primary_language: languages
            .first()
            .cloned()
            .unwrap_or_else(|| "Desconhecida".to_string()),
        commands,
        evidence,
        detected_at,
    }
}

fn unique<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .filter(|value| seen.insert(*value))
        .map(str::to_string)
        .collect()
}
