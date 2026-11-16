//! The semantic index: rebuildable, workspace-scoped units of source text and their runs. Port of
//! `electron/database/SemanticIndexRepository.ts`.
//!
//! Units are derived data. A file's units are replaced as one unit of work, and an embedding is
//! stored as little-endian `f32` values, the layout the TypeScript repository writes. Ending a run
//! records its units as workspace evidence in the same unit of work.

use noct_core::limits::semantic_index::{
    DEFAULT_LEXICAL_RESULTS, MAX_LEXICAL_RESULTS, MAX_UNITS, PAGE_LIMIT_MAX,
};
use noct_core::semantic_index::{
    SEMANTIC_INDEX_VERSION, SemanticEmbeddingSpace, SemanticIndexRun, SemanticIndexSummary,
    SemanticRunKind, SemanticRunStatus, SemanticSearchFilters, SemanticUnit, SemanticUnitKind,
    SemanticUnitLocation, SemanticUnitStatus,
};
use noct_core::text::is_js_whitespace;
use rusqlite::types::Value;
use rusqlite::{Connection, Row, Statement, named_params, params, params_from_iter};
use serde::Serialize;

use crate::clock::iso_now;
use crate::codec::{Stored, enum_text};
use crate::repositories::workspace_evidence::WorkspaceEvidence;
use crate::{Error, Result, transaction};

const DEFAULT_STALE_REASON: &str = "O arquivo mudou após a análise.";
const INVALID_EMBEDDING: &str = "Vetor de embedding inválido.";
const INVALID_STORED_DIMENSIONS: &str = "Vetor persistido com dimensões inválidas.";

const RUN_COLUMNS: &str = "id, workspace, kind, status, total_files, processed_files, \
    indexed_units, lexical_only_units, failed_files, excluded_files, started_at, completed_at, \
    updated_at, error";

/// The columns of a unit, in the order `unit_row` reads them.
const UNIT_COLUMNS: [&str; 26] = [
    "id",
    "workspace",
    "relative_path",
    "kind",
    "language",
    "symbol_id",
    "symbol_name",
    "start_line",
    "start_column",
    "end_line",
    "end_column",
    "start_offset",
    "end_offset",
    "source_hash",
    "chunk_hash",
    "normalized_text",
    "chunk_strategy_version",
    "embedding_provider_id",
    "embedding_model_id",
    "embedding_model_version",
    "embedding_dimensions",
    "status",
    "error",
    "created_at",
    "updated_at",
    "indexed_at",
];

const UPSERT_UNIT: &str = "INSERT INTO semantic_units(id,workspace,relative_path,kind,language,\
    symbol_id,symbol_name,start_line,start_column,end_line,end_column,start_offset,end_offset,\
    source_hash,chunk_hash,normalized_text,chunk_strategy_version,embedding_provider_id,\
    embedding_model_id,embedding_model_version,embedding_dimensions,embedding,status,error,\
    created_at,updated_at,indexed_at) \
    VALUES(:id,:workspace,:relative_path,:kind,:language,:symbol_id,:symbol_name,:start_line,\
    :start_column,:end_line,:end_column,:start_offset,:end_offset,:source_hash,:chunk_hash,\
    :normalized_text,:chunk_strategy_version,:embedding_provider_id,:embedding_model_id,\
    :embedding_model_version,:embedding_dimensions,:embedding,:status,:error,:created_at,\
    :updated_at,:indexed_at) \
    ON CONFLICT(id) DO UPDATE SET workspace=excluded.workspace,\
    relative_path=excluded.relative_path,kind=excluded.kind,language=excluded.language,\
    symbol_id=excluded.symbol_id,symbol_name=excluded.symbol_name,start_line=excluded.start_line,\
    start_column=excluded.start_column,end_line=excluded.end_line,end_column=excluded.end_column,\
    start_offset=excluded.start_offset,end_offset=excluded.end_offset,\
    source_hash=excluded.source_hash,chunk_hash=excluded.chunk_hash,\
    normalized_text=excluded.normalized_text,chunk_strategy_version=excluded.chunk_strategy_version,\
    embedding_provider_id=excluded.embedding_provider_id,\
    embedding_model_id=excluded.embedding_model_id,\
    embedding_model_version=excluded.embedding_model_version,\
    embedding_dimensions=excluded.embedding_dimensions,embedding=excluded.embedding,\
    status=excluded.status,error=excluded.error,updated_at=excluded.updated_at,\
    indexed_at=excluded.indexed_at";

/// A unit to store, with its embedding when it has one.
#[derive(Debug, Clone, PartialEq)]
pub struct PersistedSemanticUnit {
    pub unit: SemanticUnit,
    pub embedding: Option<Vec<f64>>,
}

/// A unit found by a lexical search, with its rank. Lower ranks are better matches.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SemanticLexicalMatch {
    pub unit: SemanticUnit,
    pub rank: f64,
}

/// An indexed unit with the embedding stored for it. The vector is for ranking inside the backend.
/// It does not implement `Serialize`, so it cannot reach a response by accident. An IPC response
/// maps it to a type without the vector.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredSemanticEmbedding {
    pub unit: SemanticUnit,
    pub embedding: Vec<f64>,
}

/// One page of a keyset listing. `has_more` says whether rows follow the page. It is learned from
/// one row more than the limit, so no further query is needed to know it. `next` is the cursor
/// after the last row of the page, and is `Some` exactly when `has_more` is set.
#[derive(Debug, Clone, PartialEq)]
pub struct Page<T, C> {
    pub items: Vec<T>,
    pub has_more: bool,
    pub next: Option<C>,
}

/// A unit's place in source order. It is the sort key of a file's units, and the cursor that
/// resumes a listing of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitPosition {
    pub start_line: i64,
    pub start_column: i64,
    pub id: String,
}

pub struct SemanticIndex<'a> {
    conn: &'a Connection,
}

impl<'a> SemanticIndex<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create_run(&self, run: &SemanticIndexRun) -> Result<()> {
        self.conn.execute(
            "INSERT INTO semantic_index_runs(id,workspace,index_version,kind,status,total_files,\
             processed_files,indexed_units,lexical_only_units,failed_files,excluded_files,\
             started_at,completed_at,updated_at,error) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                run.id,
                run.workspace,
                SEMANTIC_INDEX_VERSION,
                enum_text(&run.kind)?,
                enum_text(&run.status)?,
                run.total_files,
                run.processed_files,
                run.indexed_units,
                run.lexical_only_units,
                run.failed_files,
                run.excluded_files,
                run.started_at,
                run.completed_at,
                run.updated_at,
                run.error,
            ],
        )?;
        Ok(())
    }

    /// Updates a run. A run that ends is first recorded as workspace evidence, in the same unit.
    pub fn update_run(&self, run: &SemanticIndexRun) -> Result<()> {
        transaction::run(self.conn, |conn| {
            if matches!(
                run.status,
                SemanticRunStatus::Completed
                    | SemanticRunStatus::Failed
                    | SemanticRunStatus::Cancelled
            ) {
                WorkspaceEvidence::new(conn).record_semantic_index(
                    &run.workspace,
                    &run.id,
                    &run.started_at,
                )?;
            }
            conn.execute(
                "UPDATE semantic_index_runs SET status=?1,total_files=?2,processed_files=?3,\
                 indexed_units=?4,lexical_only_units=?5,failed_files=?6,excluded_files=?7,\
                 completed_at=?8,updated_at=?9,error=?10 WHERE id=?11 AND workspace=?12",
                params![
                    enum_text(&run.status)?,
                    run.total_files,
                    run.processed_files,
                    run.indexed_units,
                    run.lexical_only_units,
                    run.failed_files,
                    run.excluded_files,
                    run.completed_at,
                    run.updated_at,
                    run.error,
                    run.id,
                    run.workspace,
                ],
            )?;
            Ok(())
        })
    }

    pub fn latest_run(&self, workspace: &str) -> Result<Option<SemanticIndexRun>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM semantic_index_runs WHERE workspace=?1 \
             ORDER BY updated_at DESC LIMIT 1"
        ))?;
        let mut rows = statement.query_map([workspace], run_row)?;
        rows.next().transpose().map_err(Error::from)
    }

    pub fn get_unit(&self, workspace: &str, id: &str) -> Result<Option<SemanticUnit>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {} FROM semantic_units WHERE workspace=?1 AND id=?2",
            unit_columns("")
        ))?;
        let mut rows = statement.query_map(params![workspace, id], unit_row)?;
        rows.next().transpose().map_err(Error::from)
    }

    /// The units of a workspace matching the filters, ordered by path and position. The limit is
    /// kept between 1 and 50,000.
    pub fn list_units(
        &self,
        workspace: &str,
        filters: &SemanticSearchFilters,
        limit: Option<usize>,
    ) -> Result<Vec<SemanticUnit>> {
        let limit = limit
            .unwrap_or(MAX_UNITS as usize)
            .clamp(1, MAX_UNITS as usize);
        let mut clauses = vec!["workspace=?".to_string()];
        let mut parameters = vec![Value::Text(workspace.to_string())];
        push_filters(&mut clauses, &mut parameters, filters, "")?;
        parameters.push(Value::Integer(limit as i64));
        let mut statement = self.conn.prepare(&format!(
            "SELECT {} FROM semantic_units WHERE {} \
             ORDER BY relative_path,start_line,id LIMIT ?",
            unit_columns(""),
            clauses.join(" AND ")
        ))?;
        let rows = statement.query_map(params_from_iter(parameters), unit_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    /// One page of the units of a file, in source order. `after` is the position of the last unit
    /// of the previous page, or `None` for the first page. `limit` is kept between 1 and 500.
    /// The order is part of the contract, so the cursor is the whole sort key, not the identifier
    /// alone.
    pub fn list_file_units(
        &self,
        workspace: &str,
        relative_path: &str,
        after: Option<&UnitPosition>,
        limit: usize,
    ) -> Result<Page<SemanticUnit, UnitPosition>> {
        let limit = clamp_page(limit);
        let (sql, parameters) = file_units_query(workspace, relative_path, after, limit + 1);
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(parameters), unit_row)?;
        let units = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(page(units, limit, |unit| UnitPosition {
            start_line: unit.location.start_line,
            start_column: unit.location.start_column,
            id: unit.id.clone(),
        }))
    }

    /// Every unit of one file, in source order, read one page at a time. Internal routines use it
    /// instead of loading a whole file into memory.
    pub fn file_units<'s>(
        &'s self,
        workspace: &'s str,
        relative_path: &'s str,
    ) -> impl Iterator<Item = Result<SemanticUnit>> + 's {
        paged(move |after: Option<&UnitPosition>| {
            self.list_file_units(workspace, relative_path, after, PAGE_LIMIT_MAX as usize)
        })
    }

    /// One page of the indexed units embedded in the given space, in identifier order. `after` is
    /// the identifier of the last unit of the previous page, or `None` for the first page. `limit`
    /// is kept between 1 and 500. A space with no dimensions does not restrict the dimensions. A
    /// stored vector whose length disagrees with its space fails the read.
    pub fn list_indexed_embeddings(
        &self,
        workspace: &str,
        space: &SemanticEmbeddingSpace,
        filters: &SemanticSearchFilters,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Page<StoredSemanticEmbedding, String>> {
        let limit = clamp_page(limit);
        let (sql, parameters) = embeddings_query(workspace, space, filters, after, limit + 1)?;
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(parameters), embedding_row)?;
        // The page is cut before the vectors are decoded, so a page never decodes rows it drops.
        let page = page(rows.collect::<rusqlite::Result<Vec<_>>>()?, limit, |row| {
            row.0.id.clone()
        });
        let items = page
            .items
            .into_iter()
            .map(|(unit, bytes, dimensions)| {
                Ok(StoredSemanticEmbedding {
                    unit,
                    embedding: deserialize_embedding(&bytes, dimensions)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Page {
            items,
            has_more: page.has_more,
            next: page.next,
        })
    }

    /// Visits every indexed embedding in the space, in identifier order, in one pass over one query.
    /// Rows are decoded one at a time, so the space is never held in memory as a whole. A failed
    /// read, or an error returned by `visit`, stops the pass and is returned. The rows visited before
    /// it stay visited. A routine that ranks the space by similarity reads it this way. No production
    /// routine does yet; the row gate and the tests do.
    pub fn for_each_indexed_embedding(
        &self,
        workspace: &str,
        space: &SemanticEmbeddingSpace,
        filters: &SemanticSearchFilters,
        mut visit: impl FnMut(StoredSemanticEmbedding) -> Result<()>,
    ) -> Result<()> {
        let (sql, parameters) = embeddings_scan_query(workspace, space, filters)?;
        let mut statement = self.conn.prepare(&sql)?;
        let mut rows = statement.query(params_from_iter(parameters))?;
        while let Some(row) = rows.next()? {
            let (unit, bytes, dimensions) = embedding_row(row)?;
            visit(StoredSemanticEmbedding {
                unit,
                embedding: deserialize_embedding(&bytes, dimensions)?,
            })?;
        }
        Ok(())
    }

    /// The units whose text matches every token of the query, best matches first. The limit is
    /// kept between 1 and 100, and 50 when none is given. A query without tokens matches nothing.
    pub fn search_lexical(
        &self,
        workspace: &str,
        query: &str,
        filters: &SemanticSearchFilters,
        limit: Option<usize>,
    ) -> Result<Vec<SemanticLexicalMatch>> {
        let fts_query = fts_query(query);
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }
        let limit = limit
            .unwrap_or(DEFAULT_LEXICAL_RESULTS as usize)
            .clamp(1, MAX_LEXICAL_RESULTS as usize);
        let mut clauses = vec![
            "unit.workspace=?".to_string(),
            "semantic_units_fts MATCH ?".to_string(),
            "unit.status IN ('indexed','lexical-only')".to_string(),
        ];
        let mut parameters = vec![Value::Text(workspace.to_string()), Value::Text(fts_query)];
        push_filters(&mut clauses, &mut parameters, filters, "unit.")?;
        parameters.push(Value::Integer(limit as i64));
        let mut statement = self.conn.prepare(&format!(
            "SELECT {}, bm25(semantic_units_fts) AS rank FROM semantic_units_fts \
             JOIN semantic_units unit ON unit.rowid=semantic_units_fts.rowid \
             WHERE {} ORDER BY rank LIMIT ?",
            unit_columns("unit."),
            clauses.join(" AND ")
        ))?;
        let rows = statement.query_map(params_from_iter(parameters), |row| {
            Ok(SemanticLexicalMatch {
                unit: unit_row(row)?,
                rank: row.get(UNIT_COLUMNS.len())?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    /// Replaces the units of a file with the given ones, as one unit of work. Units the file no
    /// longer has are removed. Each unit is stored under its own workspace and path.
    pub fn replace_file_units(
        &self,
        workspace: &str,
        relative_path: &str,
        units: &[PersistedSemanticUnit],
    ) -> Result<()> {
        transaction::run(self.conn, |conn| {
            if units.is_empty() {
                conn.execute(
                    "DELETE FROM semantic_units WHERE workspace=?1 AND relative_path=?2",
                    params![workspace, relative_path],
                )?;
                return Ok(());
            }
            let placeholders = vec!["?"; units.len()].join(",");
            let mut parameters = vec![
                Value::Text(workspace.to_string()),
                Value::Text(relative_path.to_string()),
            ];
            parameters.extend(units.iter().map(|item| Value::Text(item.unit.id.clone())));
            conn.execute(
                &format!(
                    "DELETE FROM semantic_units WHERE workspace=? AND relative_path=? \
                     AND id NOT IN ({placeholders})"
                ),
                params_from_iter(parameters),
            )?;
            let mut statement = conn.prepare(UPSERT_UNIT)?;
            for item in units {
                write_unit(&mut statement, item)?;
            }
            Ok(())
        })
    }

    /// Marks the units of a file stale, with the reason the caller gives.
    pub fn mark_file_stale(
        &self,
        workspace: &str,
        relative_path: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        let reason = reason.unwrap_or(DEFAULT_STALE_REASON);
        let now = iso_now()?;
        transaction::run(self.conn, |conn| {
            conn.execute(
                "UPDATE semantic_units SET status='stale',error=?1,updated_at=?2 \
                 WHERE workspace=?3 AND relative_path=?4",
                params![reason, now, workspace, relative_path],
            )?;
            Ok(())
        })
    }

    pub fn delete_file(&self, workspace: &str, relative_path: &str) -> Result<()> {
        transaction::run(self.conn, |conn| {
            conn.execute(
                "DELETE FROM semantic_units WHERE workspace=?1 AND relative_path=?2",
                params![workspace, relative_path],
            )?;
            Ok(())
        })
    }

    pub fn summary(&self, workspace: &str) -> Result<SemanticIndexSummary> {
        let counts = self.conn.query_row(
            "SELECT COUNT(*), COUNT(DISTINCT relative_path), \
             SUM(CASE WHEN status='indexed' THEN 1 ELSE 0 END), \
             SUM(CASE WHEN status='lexical-only' THEN 1 ELSE 0 END), \
             SUM(CASE WHEN status='stale' THEN 1 ELSE 0 END), \
             SUM(CASE WHEN status IN ('failed','incompatible') THEN 1 ELSE 0 END), \
             SUM(CASE WHEN status='excluded' THEN 1 ELSE 0 END) \
             FROM semantic_units WHERE workspace=?1",
            [workspace],
            |row| {
                Ok([
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(5)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(6)?.unwrap_or(0),
                ])
            },
        )?;
        let [
            units,
            files,
            indexed_units,
            lexical_only_units,
            stale_units,
            failed_units,
            excluded_units,
        ] = counts;
        Ok(SemanticIndexSummary {
            workspace: workspace.to_string(),
            index_version: SEMANTIC_INDEX_VERSION,
            latest_run: self.latest_run(workspace)?,
            files,
            units,
            indexed_units,
            lexical_only_units,
            stale_units,
            failed_units,
            excluded_units,
        })
    }
}

/// A page limit, kept between 1 and the canonical ceiling.
fn clamp_page(limit: usize) -> usize {
    limit.clamp(1, PAGE_LIMIT_MAX as usize)
}

/// The page of a listing, from rows fetched with one row more than the limit. The extra row only
/// shows that rows follow, and it is dropped. The cursor is the last row kept, and exists exactly
/// when rows follow.
fn page<T, C>(mut rows: Vec<T>, limit: usize, cursor: impl Fn(&T) -> C) -> Page<T, C> {
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    let next = if has_more {
        rows.last().map(cursor)
    } else {
        None
    };
    Page {
        items: rows,
        has_more,
        next,
    }
}

/// The query of one page of a file's units, and its parameters. `fetch` rows are requested. The
/// keyset compares the whole sort key, so the order is total and the cursor resumes exactly after
/// the last unit of the previous page.
fn file_units_query(
    workspace: &str,
    relative_path: &str,
    after: Option<&UnitPosition>,
    fetch: usize,
) -> (String, Vec<Value>) {
    let mut parameters = vec![
        Value::Text(workspace.to_string()),
        Value::Text(relative_path.to_string()),
    ];
    let keyset = match after {
        Some(position) => {
            parameters.push(Value::Integer(position.start_line));
            parameters.push(Value::Integer(position.start_column));
            parameters.push(Value::Text(position.id.clone()));
            " AND (start_line,start_column,id) > (?,?,?)"
        }
        None => "",
    };
    parameters.push(Value::Integer(fetch as i64));
    let sql = format!(
        "SELECT {} FROM semantic_units WHERE workspace=? AND relative_path=?{keyset} \
         ORDER BY start_line,start_column,id LIMIT ?",
        unit_columns("")
    );
    (sql, parameters)
}

/// The clauses and parameters that select the indexed embeddings of a space. Rows with a vector are
/// the only ones read, so the vector is never null. The page query and the single-pass scan share
/// them, so both read the same rows.
fn embedding_selection(
    workspace: &str,
    space: &SemanticEmbeddingSpace,
    filters: &SemanticSearchFilters,
) -> Result<(Vec<String>, Vec<Value>)> {
    let mut clauses = vec![
        "workspace=?".to_string(),
        "status='indexed'".to_string(),
        "embedding_provider_id=?".to_string(),
        "embedding_model_id=?".to_string(),
        "embedding_model_version IS ?".to_string(),
    ];
    let mut parameters = vec![
        Value::Text(workspace.to_string()),
        Value::Text(space.provider_id.clone()),
        Value::Text(space.model_id.clone()),
        Value::from(space.model_version.clone()),
    ];
    if space.dimensions > 0 {
        clauses.push("embedding_dimensions=?".to_string());
        parameters.push(Value::Integer(space.dimensions));
    }
    clauses.push("embedding IS NOT NULL".to_string());
    push_filters(&mut clauses, &mut parameters, filters, "")?;
    Ok((clauses, parameters))
}

/// The select of indexed embeddings in identifier order, without a limit.
fn embedding_select(clauses: &[String]) -> String {
    format!(
        "SELECT {}, embedding FROM semantic_units WHERE {} ORDER BY id",
        unit_columns(""),
        clauses.join(" AND ")
    )
}

/// The query of one page of indexed embeddings in a space, and its parameters. The page starts after
/// the identifier `after` when it is given, and `fetch` rows are requested.
fn embeddings_query(
    workspace: &str,
    space: &SemanticEmbeddingSpace,
    filters: &SemanticSearchFilters,
    after: Option<&str>,
    fetch: usize,
) -> Result<(String, Vec<Value>)> {
    let (mut clauses, mut parameters) = embedding_selection(workspace, space, filters)?;
    if let Some(after) = after {
        clauses.push("id > ?".to_string());
        parameters.push(Value::Text(after.to_string()));
    }
    parameters.push(Value::Integer(fetch as i64));
    Ok((
        format!("{} LIMIT ?", embedding_select(&clauses)),
        parameters,
    ))
}

/// The query of a single pass over every indexed embedding in a space, and its parameters.
fn embeddings_scan_query(
    workspace: &str,
    space: &SemanticEmbeddingSpace,
    filters: &SemanticSearchFilters,
) -> Result<(String, Vec<Value>)> {
    let (clauses, parameters) = embedding_selection(workspace, space, filters)?;
    Ok((embedding_select(&clauses), parameters))
}

/// One row of an embedding select: the unit, its stored vector, and the dimensions the vector was
/// stored with. The dimensions column is at index 20 of the unit columns, and the vector follows them.
fn embedding_row(row: &Row<'_>) -> rusqlite::Result<(SemanticUnit, Vec<u8>, Option<i64>)> {
    Ok((
        unit_row(row)?,
        row.get::<_, Vec<u8>>(UNIT_COLUMNS.len())?,
        row.get::<_, Option<i64>>(20)?,
    ))
}

/// Streams the rows of a keyset listing one page at a time, so an internal routine never holds a
/// whole listing in memory. The first error ends the stream.
struct Paged<T, C, F> {
    fetch: F,
    after: Option<C>,
    buffered: std::vec::IntoIter<T>,
    finished: bool,
}

fn paged<T, C, F>(fetch: F) -> Paged<T, C, F>
where
    F: FnMut(Option<&C>) -> Result<Page<T, C>>,
{
    Paged {
        fetch,
        after: None,
        buffered: Vec::new().into_iter(),
        finished: false,
    }
}

impl<T, C, F> Iterator for Paged<T, C, F>
where
    F: FnMut(Option<&C>) -> Result<Page<T, C>>,
{
    type Item = Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(item) = self.buffered.next() {
                return Some(Ok(item));
            }
            if self.finished {
                return None;
            }
            match (self.fetch)(self.after.as_ref()) {
                Ok(page) => {
                    // A page without a cursor cannot resume, so it ends the stream as well.
                    self.finished = !page.has_more || page.next.is_none();
                    self.after = page.next;
                    self.buffered = page.items.into_iter();
                }
                Err(error) => {
                    self.finished = true;
                    return Some(Err(error));
                }
            }
        }
    }
}

/// The unit columns, qualified by `prefix`, as a select list.
fn unit_columns(prefix: &str) -> String {
    UNIT_COLUMNS
        .iter()
        .map(|column| format!("{prefix}{column}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn run_row(row: &Row<'_>) -> rusqlite::Result<SemanticIndexRun> {
    Ok(SemanticIndexRun {
        id: row.get(0)?,
        workspace: row.get(1)?,
        kind: row.get::<_, Stored<SemanticRunKind>>(2)?.0,
        status: row.get::<_, Stored<SemanticRunStatus>>(3)?.0,
        total_files: row.get(4)?,
        processed_files: row.get(5)?,
        indexed_units: row.get(6)?,
        lexical_only_units: row.get(7)?,
        failed_files: row.get(8)?,
        excluded_files: row.get(9)?,
        started_at: row.get(10)?,
        completed_at: row.get(11)?,
        updated_at: row.get(12)?,
        error: row.get(13)?,
    })
}

/// A unit row, with the columns of `UNIT_COLUMNS` in order. The embedding itself is not read here.
fn unit_row(row: &Row<'_>) -> rusqlite::Result<SemanticUnit> {
    Ok(SemanticUnit {
        id: row.get(0)?,
        workspace: row.get(1)?,
        relative_path: row.get(2)?,
        kind: row.get::<_, Stored<SemanticUnitKind>>(3)?.0,
        language: row.get(4)?,
        symbol_id: row.get(5)?,
        symbol_name: row.get(6)?,
        location: SemanticUnitLocation {
            start_line: row.get(7)?,
            start_column: row.get(8)?,
            end_line: row.get(9)?,
            end_column: row.get(10)?,
            start_offset: row.get(11)?,
            end_offset: row.get(12)?,
        },
        source_hash: row.get(13)?,
        chunk_hash: row.get(14)?,
        normalized_text: row.get(15)?,
        chunk_strategy_version: row.get(16)?,
        embedding_space: embedding_space(row)?,
        status: row.get::<_, Stored<SemanticUnitStatus>>(21)?.0,
        error: row.get(22)?,
        created_at: row.get(23)?,
        updated_at: row.get(24)?,
        indexed_at: row.get(25)?,
    })
}

/// The embedding space of a unit. A space exists only when its provider, model and dimensions are
/// all set, as the TypeScript repository reads them.
fn embedding_space(row: &Row<'_>) -> rusqlite::Result<Option<SemanticEmbeddingSpace>> {
    let provider_id: Option<String> = row.get(17)?;
    let model_id: Option<String> = row.get(18)?;
    let dimensions: Option<i64> = row.get(20)?;
    match (provider_id, model_id, dimensions) {
        (Some(provider_id), Some(model_id), Some(dimensions))
            if !provider_id.is_empty() && !model_id.is_empty() && dimensions != 0 =>
        {
            Ok(Some(SemanticEmbeddingSpace {
                provider_id,
                model_id,
                model_version: row.get(19)?,
                dimensions,
            }))
        }
        _ => Ok(None),
    }
}

/// Appends the clauses and parameters of the filters. Each filter list matches any of its values.
fn push_filters(
    clauses: &mut Vec<String>,
    parameters: &mut Vec<Value>,
    filters: &SemanticSearchFilters,
    prefix: &str,
) -> Result<()> {
    let kinds = filters
        .kinds
        .iter()
        .map(enum_text)
        .collect::<Result<Vec<_>>>()?;
    push_any(
        clauses,
        parameters,
        &format!("{prefix}relative_path"),
        &filters.paths,
    );
    push_any(
        clauses,
        parameters,
        &format!("{prefix}language"),
        &filters.languages,
    );
    push_any(clauses, parameters, &format!("{prefix}kind"), &kinds);
    push_any(
        clauses,
        parameters,
        &format!("{prefix}symbol_name"),
        &filters.symbols,
    );
    Ok(())
}

fn push_any(
    clauses: &mut Vec<String>,
    parameters: &mut Vec<Value>,
    column: &str,
    values: &[String],
) {
    if values.is_empty() {
        return;
    }
    let alternatives = vec![format!("{column}=?"); values.len()].join(" OR ");
    clauses.push(format!("({alternatives})"));
    parameters.extend(values.iter().map(|value| Value::Text(value.clone())));
}

/// Quotes each token of the query and joins them with `AND`. Tokens are the runs of text between
/// JavaScript whitespace; a double quote inside a token is doubled, as FTS5 requires.
fn fts_query(query: &str) -> String {
    query
        .split(is_js_whitespace)
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn write_unit(statement: &mut Statement<'_>, item: &PersistedSemanticUnit) -> Result<()> {
    let unit = &item.unit;
    let kind = enum_text(&unit.kind)?;
    let status = enum_text(&unit.status)?;
    let embedding = item
        .embedding
        .as_deref()
        .map(serialize_embedding)
        .transpose()?;
    let space = unit.embedding_space.as_ref();
    let provider_id = space.map(|space| space.provider_id.clone());
    let model_id = space.map(|space| space.model_id.clone());
    let model_version = space.and_then(|space| space.model_version.clone());
    let dimensions = space.map(|space| space.dimensions);
    statement.execute(named_params! {
        ":id": unit.id,
        ":workspace": unit.workspace,
        ":relative_path": unit.relative_path,
        ":kind": kind,
        ":language": unit.language,
        ":symbol_id": unit.symbol_id,
        ":symbol_name": unit.symbol_name,
        ":start_line": unit.location.start_line,
        ":start_column": unit.location.start_column,
        ":end_line": unit.location.end_line,
        ":end_column": unit.location.end_column,
        ":start_offset": unit.location.start_offset,
        ":end_offset": unit.location.end_offset,
        ":source_hash": unit.source_hash,
        ":chunk_hash": unit.chunk_hash,
        ":normalized_text": unit.normalized_text,
        ":chunk_strategy_version": unit.chunk_strategy_version,
        ":embedding_provider_id": provider_id,
        ":embedding_model_id": model_id,
        ":embedding_model_version": model_version,
        ":embedding_dimensions": dimensions,
        ":embedding": embedding,
        ":status": status,
        ":error": unit.error,
        ":created_at": unit.created_at,
        ":updated_at": unit.updated_at,
        ":indexed_at": unit.indexed_at,
    })?;
    Ok(())
}

/// The stored form of an embedding: little-endian `f32` values. Empty vectors and non-finite
/// values are refused.
fn serialize_embedding(values: &[f64]) -> Result<Vec<u8>> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return Err(Error::Message(INVALID_EMBEDDING.to_string()));
    }
    Ok(values
        .iter()
        .flat_map(|value| (*value as f32).to_le_bytes())
        .collect())
}

/// Reads an embedding back. Its byte length must match the dimensions the row records.
fn deserialize_embedding(bytes: &[u8], dimensions: Option<i64>) -> Result<Vec<f64>> {
    let expected = dimensions
        .and_then(|dimensions| usize::try_from(dimensions).ok())
        .and_then(|dimensions| dimensions.checked_mul(4));
    if expected != Some(bytes.len()) {
        return Err(Error::Message(INVALID_STORED_DIMENSIONS.to_string()));
    }
    bytes
        .chunks_exact(4)
        .map(|chunk| {
            let array: [u8; 4] = chunk
                .try_into()
                .map_err(|_| Error::Message(INVALID_STORED_DIMENSIONS.to_string()))?;
            Ok(f64::from(f32::from_le_bytes(array)))
        })
        .collect()
}

/// The query plans of the keyset listings, as SQLite chooses them. The plans are read from the same
/// query builders the listings run, so they cannot drift apart. The database is empty, so the plans
/// reflect the schema's indexes, not the statistics of a large history.
#[cfg(test)]
mod plan_tests {
    use super::*;

    fn database() -> Connection {
        let mut connection = Connection::open_in_memory().expect("memory database");
        crate::migrations::migrate_latest(&mut connection, 0).expect("schema");
        connection
    }

    /// The `detail` column of EXPLAIN QUERY PLAN, one line per step.
    fn plan(connection: &Connection, (sql, parameters): (String, Vec<Value>)) -> Vec<String> {
        let mut statement = connection
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .expect("the page query prepares");
        statement
            .query_map(params_from_iter(parameters), |row| row.get::<_, String>(3))
            .expect("the plan runs")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("plan rows")
    }

    fn space() -> SemanticEmbeddingSpace {
        SemanticEmbeddingSpace {
            provider_id: "local".to_string(),
            model_id: "mini".to_string(),
            model_version: None,
            dimensions: 2,
        }
    }

    fn resume_after_unit() -> UnitPosition {
        UnitPosition {
            start_line: 10,
            start_column: 2,
            id: "unit".to_string(),
        }
    }

    fn assert_file_index_search(details: &[String]) {
        assert!(
            details
                .iter()
                .any(|detail| detail.starts_with("SEARCH semantic_units")
                    && detail.contains("idx_semantic_units_workspace_path")),
            "the file's units are found through their index: {details:?}"
        );
        assert!(
            details
                .iter()
                .all(|detail| !detail.starts_with("SCAN semantic_units")),
            "no step scans the whole table: {details:?}"
        );
    }

    #[test]
    fn the_first_page_of_a_file_is_an_index_search() {
        let connection = database();
        let details = plan(
            &connection,
            file_units_query("/work/alpha", "src/app.ts", None, 501),
        );
        assert_file_index_search(&details);
    }

    /// The keyset resumes with a seek on the first column of the sort key, so the page does not
    /// read the rows before the cursor. The rest of the key is checked within that range.
    #[test]
    fn a_keyset_page_of_a_file_seeks_past_the_cursor_line() {
        let connection = database();
        let details = plan(
            &connection,
            file_units_query("/work/alpha", "src/app.ts", Some(&resume_after_unit()), 501),
        );
        assert_file_index_search(&details);
        assert!(
            details.iter().any(|detail| detail.contains("start_line>?")),
            "the cursor is a seek, not a filter over the whole file: {details:?}"
        );
    }

    /// The identifier keyset is not a seek yet: the planner reads the workspace's indexed units
    /// through the status index and sorts them. The test keeps the guarantee that holds today, that
    /// the page never scans the table. ADR 0002 records the temporary sort and the index that would
    /// remove it.
    #[test]
    fn a_keyset_page_of_embeddings_never_scans_the_table() {
        let connection = database();
        let details = plan(
            &connection,
            embeddings_query(
                "/work/alpha",
                &space(),
                &SemanticSearchFilters::default(),
                Some("unit"),
                501,
            )
            .expect("embeddings query"),
        );
        assert!(
            details
                .iter()
                .any(|detail| detail.starts_with("SEARCH semantic_units")),
            "the page is found through an index: {details:?}"
        );
        assert!(
            details
                .iter()
                .all(|detail| !detail.starts_with("SCAN semantic_units")),
            "no step scans the whole table: {details:?}"
        );
    }
}

/// Synthetic benchmark of the embedding listings at 50 thousand and 200 thousand indexed embeddings.
/// It is ignored by default, because it takes minutes. It prints lines starting with `BENCH` and
/// asserts no timing, so it cannot fail on a slow machine. Run it in release mode:
///
/// `cargo test --release -p noct-db --lib benchmark_tests -- --ignored --nocapture`
///
/// The criterion is fixed before the numbers are read. The page walk is a problem when its time at
/// 200 thousand rows is at least 8 times its time at 50 thousand. Linear growth gives 4 and quadratic
/// growth gives 16. Each page of the walk sorts the whole space, so the walk should grow quadratically.
#[cfg(test)]
mod benchmark_tests {
    use std::time::{Duration, Instant};

    use noct_core::code_intelligence::{
        DiscoveryFileClassification, ProjectIndexFile, ProjectIndexFileState,
    };

    use super::*;
    use crate::repositories::project_index::ProjectIndex;

    const WORKSPACE: &str = "/work/bench";
    const AT: &str = "2026-01-01T00:00:00.000Z";
    /// Units per file. Each file is replaced in one call, as the indexer does.
    const UNITS_PER_FILE: usize = 200;
    /// A small vector keeps the size of the database close to its row count.
    const DIMENSIONS: i64 = 8;
    /// The ratio at which growth is counted as super-linear.
    const PROBLEM_RATIO: f64 = 8.0;

    fn database() -> Connection {
        let mut connection = Connection::open_in_memory().expect("memory database");
        crate::migrations::migrate_latest(&mut connection, 0).expect("schema");
        connection
    }

    fn space() -> SemanticEmbeddingSpace {
        SemanticEmbeddingSpace {
            provider_id: "local".to_string(),
            model_id: "mini".to_string(),
            model_version: None,
            dimensions: DIMENSIONS,
        }
    }

    fn project_file(relative_path: &str) -> ProjectIndexFile {
        ProjectIndexFile {
            workspace: WORKSPACE.to_string(),
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

    /// An indexed unit with an embedding. The identifier is unique across the workspace, and the
    /// vector is synthetic.
    fn persisted(relative_path: &str, n: usize) -> PersistedSemanticUnit {
        let line = (n % UNITS_PER_FILE) as i64 + 1;
        PersistedSemanticUnit {
            unit: SemanticUnit {
                id: format!("u{n:08}"),
                workspace: WORKSPACE.to_string(),
                relative_path: relative_path.to_string(),
                kind: SemanticUnitKind::Symbol,
                language: Some("TypeScript".to_string()),
                symbol_id: None,
                symbol_name: Some(format!("s{n}")),
                location: SemanticUnitLocation {
                    start_line: line,
                    start_column: 1,
                    end_line: line,
                    end_column: 2,
                    start_offset: None,
                    end_offset: None,
                },
                source_hash: "a".repeat(64),
                chunk_hash: "b".repeat(64),
                normalized_text: format!("texto sintetico {n}"),
                chunk_strategy_version: "symbols-v2".to_string(),
                embedding_space: Some(space()),
                status: SemanticUnitStatus::Indexed,
                error: None,
                created_at: AT.to_string(),
                updated_at: AT.to_string(),
                indexed_at: None,
            },
            embedding: Some(
                (0..DIMENSIONS)
                    .map(|d| ((n as i64 + d) % 97) as f64 / 97.0)
                    .collect(),
            ),
        }
    }

    fn populate(connection: &Connection, rows: usize) {
        connection
            .execute(
                "INSERT INTO workspaces(path,name,created_at,last_opened_at) VALUES (?1,'bench',?2,?2)",
                params![WORKSPACE, AT],
            )
            .expect("workspace");
        let project = ProjectIndex::new(connection);
        let index = SemanticIndex::new(connection);
        for file in 0..rows / UNITS_PER_FILE {
            let path = format!("src/f{file:06}.ts");
            project
                .upsert_file(&project_file(&path))
                .expect("project file");
            let units: Vec<PersistedSemanticUnit> = (0..UNITS_PER_FILE)
                .map(|line| persisted(&path, file * UNITS_PER_FILE + line))
                .collect();
            index
                .replace_file_units(WORKSPACE, &path, &units)
                .expect("units");
        }
    }

    /// The time of one full read of the space, by the page walk and by the single pass.
    struct Timing {
        pages: usize,
        paged: Duration,
        single: Duration,
    }

    /// Walks the space by pages of the maximum size, and returns the number of pages read.
    fn walk_by_pages(index: &SemanticIndex, rows: usize) -> usize {
        let space = space();
        let filters = SemanticSearchFilters::default();
        let mut after: Option<String> = None;
        let (mut seen, mut pages) = (0, 0);
        loop {
            let page = index
                .list_indexed_embeddings(
                    WORKSPACE,
                    &space,
                    &filters,
                    after.as_deref(),
                    PAGE_LIMIT_MAX as usize,
                )
                .expect("page");
            seen += page.items.len();
            pages += 1;
            match page.next {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        assert_eq!(seen, rows, "the walk reads every row once");
        pages
    }

    fn time_both(connection: &Connection, rows: usize) -> Timing {
        let index = SemanticIndex::new(connection);
        let started = Instant::now();
        let pages = walk_by_pages(&index, rows);
        let paged = started.elapsed();

        let mut passed = 0;
        let started = Instant::now();
        index
            .for_each_indexed_embedding(
                WORKSPACE,
                &space(),
                &SemanticSearchFilters::default(),
                |_| {
                    passed += 1;
                    Ok(())
                },
            )
            .expect("single pass");
        let single = started.elapsed();
        assert_eq!(passed, rows, "the single pass reads every row once");
        Timing {
            pages,
            paged,
            single,
        }
    }

    /// The `detail` column of EXPLAIN QUERY PLAN, printed on one line.
    fn print_plan(connection: &Connection, label: &str, query: (String, Vec<Value>)) {
        let (sql, parameters) = query;
        let mut statement = connection
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .expect("plan prepares");
        let details = statement
            .query_map(params_from_iter(parameters), |row| row.get::<_, String>(3))
            .expect("plan runs")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("plan rows");
        println!("BENCH plan {label}: {}", details.join(" | "));
    }

    fn print_plans(connection: &Connection, state: &str) {
        let space = space();
        let filters = SemanticSearchFilters::default();
        let after = format!("u{:08}", 100_000);
        let cursor = UnitPosition {
            start_line: 100,
            start_column: 1,
            id: "u00000100".to_string(),
        };
        print_plan(
            connection,
            &format!("{state}, embeddings page, first"),
            embeddings_query(WORKSPACE, &space, &filters, None, 501).expect("query"),
        );
        print_plan(
            connection,
            &format!("{state}, embeddings page, identifier cursor"),
            embeddings_query(WORKSPACE, &space, &filters, Some(&after), 501).expect("query"),
        );
        print_plan(
            connection,
            &format!("{state}, embeddings single pass"),
            embeddings_scan_query(WORKSPACE, &space, &filters).expect("query"),
        );
        print_plan(
            connection,
            &format!("{state}, file page, source cursor"),
            file_units_query(WORKSPACE, "src/f000000.ts", Some(&cursor), 501),
        );
    }

    /// Populates a database with `rows` embeddings, times the listings before and after `ANALYZE`,
    /// and prints the plans of both states. The application runs `PRAGMA optimize` when it closes a
    /// database, so both states occur in practice.
    fn measure(rows: usize) -> (Timing, Timing) {
        let connection = database();
        populate(&connection, rows);
        print_plans(&connection, &format!("{rows} rows, no statistics"));
        let before = time_both(&connection, rows);
        connection.execute_batch("ANALYZE").expect("analyze");
        print_plans(&connection, &format!("{rows} rows, after ANALYZE"));
        let after = time_both(&connection, rows);
        (before, after)
    }

    fn report(rows: usize, state: &str, timing: &Timing) {
        println!(
            "BENCH rows={rows} state={state} pages={} paged_ms={:.0} single_pass_ms={:.0}",
            timing.pages,
            timing.paged.as_secs_f64() * 1000.0,
            timing.single.as_secs_f64() * 1000.0,
        );
    }

    #[test]
    #[ignore = "benchmark: run in release mode with --ignored --nocapture"]
    fn embedding_listings_at_fifty_and_two_hundred_thousand_rows() {
        let small_measure = measure(50_000);
        let large_measure = measure(200_000);
        for (state, small, large) in [
            ("no statistics", &small_measure.0, &large_measure.0),
            ("after ANALYZE", &small_measure.1, &large_measure.1),
        ] {
            report(50_000, state, small);
            report(200_000, state, large);
            let paged = large.paged.as_secs_f64() / small.paged.as_secs_f64();
            let single = large.single.as_secs_f64() / small.single.as_secs_f64();
            println!("BENCH ratio {state}: paged={paged:.2} single_pass={single:.2}");
            let verdict = if paged >= PROBLEM_RATIO {
                "problem, the page walk grows super-linearly"
            } else {
                "no problem measured, the page walk grows below the criterion"
            };
            println!("BENCH verdict {state}: {verdict}");
        }
    }
}
