//! The semantic index: rebuildable, workspace-scoped units of source text and their runs. Port of
//! `electron/database/SemanticIndexRepository.ts`.
//!
//! Units are derived data. A file's units are replaced as one unit of work, and an embedding is
//! stored as little-endian `f32` values, the layout the TypeScript repository writes. Ending a run
//! records its units as workspace evidence in the same unit of work.

use noct_core::limits::semantic_index::{DEFAULT_LEXICAL_RESULTS, MAX_LEXICAL_RESULTS, MAX_UNITS};
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

/// An indexed unit with the embedding stored for it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoredSemanticEmbedding {
    pub unit: SemanticUnit,
    pub embedding: Vec<f64>,
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

    /// The units of one file, in source order.
    pub fn list_file_units(
        &self,
        workspace: &str,
        relative_path: &str,
    ) -> Result<Vec<SemanticUnit>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {} FROM semantic_units WHERE workspace=?1 AND relative_path=?2 \
             ORDER BY start_line,start_column,id",
            unit_columns("")
        ))?;
        let rows = statement.query_map(params![workspace, relative_path], unit_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    /// The indexed units embedded in the given space. A space with no dimensions does not restrict
    /// the dimensions. A stored vector whose length disagrees with its space fails the read.
    pub fn list_indexed_embeddings(
        &self,
        workspace: &str,
        space: &SemanticEmbeddingSpace,
        filters: &SemanticSearchFilters,
    ) -> Result<Vec<StoredSemanticEmbedding>> {
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
        let mut statement = self.conn.prepare(&format!(
            "SELECT {}, embedding FROM semantic_units WHERE {}",
            unit_columns(""),
            clauses.join(" AND ")
        ))?;
        let rows = statement.query_map(params_from_iter(parameters), |row| {
            Ok((
                unit_row(row)?,
                row.get::<_, Option<Vec<u8>>>(UNIT_COLUMNS.len())?,
                row.get::<_, Option<i64>>(20)?,
            ))
        })?;
        let mut embeddings = Vec::new();
        for row in rows {
            let (unit, bytes, dimensions) = row?;
            let Some(bytes) = bytes else { continue };
            embeddings.push(StoredSemanticEmbedding {
                unit,
                embedding: deserialize_embedding(&bytes, dimensions)?,
            });
        }
        Ok(embeddings)
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
