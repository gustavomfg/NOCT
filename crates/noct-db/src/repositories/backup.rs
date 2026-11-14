//! Backup export and transactional import. Port of `electron/database/BackupRepository.ts`.
//!
//! The export is the database-shaped contract: each row keeps its column names, and the settings
//! are read through the settings repository. The import replaces the data of a scope as one unit
//! of work. Only the columns of the allow list are written, and text is refused when it is longer
//! than the limit or holds a NUL. The IPC layer checks the backup document before it reaches this
//! repository; the repository checks every value it writes.

use std::collections::BTreeMap;

use noct_core::limits::DATABASE_SCHEMA_VERSION;
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, Row, ToSql};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::clock::iso_now;
use crate::repositories::settings::Settings;
use crate::{Error, Result, runtime, transaction};

/// The longest text one column accepts from a backup, in UTF-16 code units.
const MAX_IMPORT_TEXT_UNITS: usize = 10_000_000;
/// The estimate of the bytes one record takes beyond its content.
const RECORD_OVERHEAD_BYTES: i64 = 64;
/// The estimate of the bytes a backup takes beyond its records.
const DOCUMENT_OVERHEAD_BYTES: i64 = 1_024;

const DELETE_SCOPED_SQL: &str = "DELETE FROM workspace_model_bindings;
    DELETE FROM brain_memory_history;
    DELETE FROM brain_memories;
    DELETE FROM suggestion_decisions;
    DELETE FROM suggestions;
    DELETE FROM artifacts;
    DELETE FROM messages;
    DELETE FROM conversations;
    DELETE FROM workspaces;
    DELETE FROM workspace_memory;";
const DELETE_FULL_SQL: &str =
    "DELETE FROM model_catalog; DELETE FROM provider_configs; DELETE FROM settings;";
/// A backup of a brain memory without history gets a `created` entry, as the legacy format did.
const LEGACY_HISTORY_SQL: &str = "INSERT OR IGNORE INTO brain_memory_history(
        id,memory_id,action,from_status,to_status,summary,created_at
      ) SELECT 'created-' || id,id,'created',NULL,status,
        'Memória restaurada de backup legado.',created_at FROM brain_memories";

/// The collections of a backup and the query that reads each one. The order is the order of the
/// export document.
const EXPORT_QUERIES: [(&str, &str); 12] = [
    ("conversations", "SELECT * FROM conversations"),
    ("workspaces", "SELECT * FROM workspaces"),
    ("messages", "SELECT * FROM messages ORDER BY created_at"),
    ("artifacts", "SELECT * FROM artifacts ORDER BY created_at"),
    ("memories", "SELECT * FROM workspace_memory"),
    (
        "brainMemories",
        "SELECT * FROM brain_memories ORDER BY created_at",
    ),
    (
        "brainMemoryHistory",
        "SELECT * FROM brain_memory_history ORDER BY created_at",
    ),
    ("suggestions", "SELECT * FROM suggestions"),
    ("suggestionDecisions", "SELECT * FROM suggestion_decisions"),
    (
        "providerConfigs",
        "SELECT id,provider_type,display_name,source,
        base_url,enabled,requires_authentication,timeout_ms,created_at,updated_at
        FROM provider_configs ORDER BY created_at",
    ),
    (
        "modelCatalog",
        "SELECT provider_id,model_id,descriptor,updated_at
        FROM model_catalog ORDER BY provider_id,model_id",
    ),
    (
        "workspaceModelBindings",
        "SELECT workspace_id,bindings,updated_at
        FROM workspace_model_bindings ORDER BY workspace_id",
    ),
];

/// The tables whose size the export metrics measure.
const EXPORT_TABLES: [&str; 13] = [
    "conversations",
    "workspaces",
    "messages",
    "artifacts",
    "workspace_memory",
    "brain_memories",
    "brain_memory_history",
    "suggestions",
    "suggestion_decisions",
    "provider_configs",
    "model_catalog",
    "workspace_model_bindings",
    "settings",
];

const WORKSPACE_COLUMNS: &[&str] = &[
    "path",
    "name",
    "favorite",
    "authorized",
    "created_at",
    "last_opened_at",
];
const CONVERSATION_COLUMNS: &[&str] = &["id", "title", "workspace", "created_at", "updated_at"];
const MESSAGE_COLUMNS: &[&str] = &[
    "id",
    "conversation_id",
    "role",
    "content",
    "metadata",
    "created_at",
];
const ARTIFACT_COLUMNS: &[&str] = &[
    "id",
    "conversation_id",
    "workspace",
    "type",
    "title",
    "file_path",
    "content",
    "metadata",
    "created_at",
    "updated_at",
];
const MEMORY_COLUMNS: &[&str] = &["workspace", "content", "updated_at"];
const SUGGESTION_COLUMNS: &[&str] = &[
    "id",
    "workspace_id",
    "conversation_id",
    "title",
    "description",
    "reasoning",
    "category",
    "severity",
    "affected_files",
    "proposed_changes",
    "expected_benefits",
    "complexity",
    "risk",
    "evidence",
    "confidence",
    "source",
    "responsible",
    "status",
    "result",
    "created_at",
    "updated_at",
];
const DECISION_COLUMNS: &[&str] = &["id", "suggestion_id", "status", "result", "created_at"];
const BRAIN_MEMORY_COLUMNS: &[&str] = &[
    "id",
    "workspace_id",
    "conversation_id",
    "kind",
    "scope",
    "status",
    "content",
    "confidence",
    "source_type",
    "source_id",
    "created_at",
    "updated_at",
    "last_confirmed_at",
    "last_used_at",
    "use_count",
];
const BRAIN_HISTORY_COLUMNS: &[&str] = &[
    "id",
    "memory_id",
    "action",
    "from_status",
    "to_status",
    "summary",
    "created_at",
];
const PROVIDER_COLUMNS: &[&str] = &[
    "id",
    "provider_type",
    "display_name",
    "source",
    "base_url",
    "enabled",
    "requires_authentication",
    "timeout_ms",
    "created_at",
    "updated_at",
];
const MODEL_CATALOG_COLUMNS: &[&str] = &["provider_id", "model_id", "descriptor", "updated_at"];
const BINDING_COLUMNS: &[&str] = &["workspace_id", "bindings", "updated_at"];

/// The part of a dataset that an import restores. Each collection is optional in the document and
/// empty when it is missing. A row keeps its JSON form until the allow list has chosen its columns.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportData {
    pub conversations: Vec<Value>,
    pub workspaces: Vec<Value>,
    pub messages: Vec<Value>,
    pub artifacts: Vec<Value>,
    pub memories: Vec<Value>,
    pub brain_memories: Vec<Value>,
    pub brain_memory_history: Vec<Value>,
    pub suggestions: Vec<Value>,
    pub suggestion_decisions: Vec<Value>,
    pub provider_configs: Vec<Value>,
    pub model_catalog: Vec<Value>,
    pub workspace_model_bindings: Vec<Value>,
    pub settings: Option<BTreeMap<String, String>>,
}

/// What an import replaces. A `ProjectData` import keeps the providers, their catalog and
/// bindings, and the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportScope {
    Full,
    ProjectData,
}

/// The records of the database and the estimated size of the backup that holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportMetrics {
    pub records: i64,
    pub estimated_bytes: i64,
}

pub struct Backup<'a> {
    conn: &'a Connection,
}

impl<'a> Backup<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// The database as a backup document. The export time is the clock's, as in the TypeScript
    /// repository, so compare documents without it.
    pub fn export(&self) -> Result<Value> {
        let mut document = Map::new();
        document.insert("schemaVersion".to_string(), json!(DATABASE_SCHEMA_VERSION));
        document.insert("exportedAt".to_string(), json!(iso_now()?));
        for (collection, sql) in EXPORT_QUERIES {
            document.insert(collection.to_string(), Value::Array(self.rows(sql)?));
        }
        document.insert(
            "settings".to_string(),
            json!(Settings::new(self.conn).get()?),
        );
        Ok(Value::Object(document))
    }

    /// The records of every exported table, and the size of their content.
    pub fn export_metrics(&self) -> Result<ExportMetrics> {
        let mut records = 0_i64;
        let mut content_bytes = 0_i64;
        for table in EXPORT_TABLES {
            let columns: Vec<String> = {
                let mut statement = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
                statement
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<rusqlite::Result<_>>()?
            };
            let bytes = if columns.is_empty() {
                "0".to_string()
            } else {
                columns
                    .iter()
                    .map(|column| format!("COALESCE(LENGTH(CAST(\"{column}\" AS BLOB)),0)"))
                    .collect::<Vec<_>>()
                    .join("+")
            };
            let (table_records, table_bytes): (i64, i64) = self.conn.query_row(
                &format!("SELECT COUNT(*), COALESCE(SUM({bytes}),0) FROM {table}"),
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            records += table_records;
            content_bytes += table_bytes;
        }
        Ok(ExportMetrics {
            records,
            estimated_bytes: content_bytes
                + records * RECORD_OVERHEAD_BYTES
                + DOCUMENT_OVERHEAD_BYTES,
        })
    }

    /// Replaces the data of `scope` with `data`, as one unit of work. A refused value rolls the
    /// whole import back, and the orphans left by the replacement are removed before it commits.
    pub fn import(&self, data: &ImportData, scope: ImportScope) -> Result<()> {
        transaction::run(self.conn, |conn| {
            conn.execute_batch(DELETE_SCOPED_SQL)?;
            if scope == ImportScope::Full {
                conn.execute_batch(DELETE_FULL_SQL)?;
            }
            let workspaces: Vec<Value> = data
                .workspaces
                .iter()
                .map(|row| with_field(row, "authorized", json!(0)))
                .collect();
            insert(conn, "workspaces", WORKSPACE_COLUMNS, &workspaces)?;
            insert(
                conn,
                "conversations",
                CONVERSATION_COLUMNS,
                &data.conversations,
            )?;
            insert(conn, "messages", MESSAGE_COLUMNS, &data.messages)?;
            insert(conn, "artifacts", ARTIFACT_COLUMNS, &data.artifacts)?;
            insert(conn, "workspace_memory", MEMORY_COLUMNS, &data.memories)?;
            insert(
                conn,
                "brain_memories",
                BRAIN_MEMORY_COLUMNS,
                &data.brain_memories,
            )?;
            insert(
                conn,
                "brain_memory_history",
                BRAIN_HISTORY_COLUMNS,
                &data.brain_memory_history,
            )?;
            insert(conn, "suggestions", SUGGESTION_COLUMNS, &data.suggestions)?;
            insert(
                conn,
                "suggestion_decisions",
                DECISION_COLUMNS,
                &data.suggestion_decisions,
            )?;
            conn.execute_batch(LEGACY_HISTORY_SQL)?;
            if scope == ImportScope::Full {
                // A provider written by the earlier format keeps its endpoint and becomes a
                // chat-completions provider.
                let providers: Vec<Value> = data
                    .provider_configs
                    .iter()
                    .map(|row| {
                        let mut row = row.as_object().cloned().unwrap_or_default();
                        if row.get("provider_type") == Some(&json!("openai-compatible")) {
                            row.insert("provider_type".to_string(), json!("chat-completions"));
                        }
                        Value::Object(row)
                    })
                    .collect();
                insert(conn, "provider_configs", PROVIDER_COLUMNS, &providers)?;
                insert(
                    conn,
                    "model_catalog",
                    MODEL_CATALOG_COLUMNS,
                    &data.model_catalog,
                )?;
                insert(
                    conn,
                    "workspace_model_bindings",
                    BINDING_COLUMNS,
                    &data.workspace_model_bindings,
                )?;
                if let Some(settings) = &data.settings {
                    Settings::new(conn).set(settings)?;
                }
            }
            runtime::remove_orphans(conn)
        })
    }

    fn rows(&self, sql: &str) -> Result<Vec<Value>> {
        let mut statement = self.conn.prepare(sql)?;
        let names: Vec<String> = statement
            .column_names()
            .map(|name| name.to_string())
            .collect();
        let mut rows = statement.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(Value::Object(row_object(row, &names)?));
        }
        Ok(items)
    }
}

fn row_object(row: &Row<'_>, names: &[String]) -> Result<Map<String, Value>> {
    let mut object = Map::new();
    for (index, name) in names.iter().enumerate() {
        let value = match row.get_ref(index)? {
            rusqlite::types::ValueRef::Null => Value::Null,
            rusqlite::types::ValueRef::Integer(number) => json!(number),
            rusqlite::types::ValueRef::Real(number) => json!(number),
            rusqlite::types::ValueRef::Text(_) => Value::String(row.get(index)?),
            rusqlite::types::ValueRef::Blob(_) => {
                return Err(Error::Message(
                    "Valor binário não pode ser exportado.".to_string(),
                ));
            }
        };
        object.insert(name.clone(), value);
    }
    Ok(object)
}

/// A copy of a row with one more field. A row that is not an object starts empty, as the spread of
/// the TypeScript repository does.
fn with_field(row: &Value, key: &str, value: Value) -> Value {
    let mut row = row.as_object().cloned().unwrap_or_default();
    row.insert(key.to_string(), value);
    Value::Object(row)
}

/// Writes the allowed columns of each object row. Other keys are ignored, and a row with none of
/// the allowed keys is skipped, as in the TypeScript repository.
fn insert(conn: &Connection, table: &str, columns: &[&str], rows: &[Value]) -> Result<()> {
    for raw in rows {
        let Some(row) = raw.as_object() else {
            continue;
        };
        let keys: Vec<&str> = columns
            .iter()
            .copied()
            .filter(|column| row.contains_key(*column))
            .collect();
        if keys.is_empty() {
            continue;
        }
        let mut values: Vec<(String, SqlValue)> = Vec::with_capacity(keys.len());
        for key in &keys {
            values.push((format!(":{key}"), sql_value(table, key, &row[*key])?));
        }
        let sql = format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            keys.join(","),
            keys.iter()
                .map(|key| format!(":{key}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut statement = conn.prepare_cached(&sql)?;
        let parameters: Vec<(&str, &dyn ToSql)> = values
            .iter()
            .map(|(name, value)| (name.as_str(), value as &dyn ToSql))
            .collect();
        statement.execute(parameters.as_slice())?;
    }
    Ok(())
}

/// A backup value as a stored value. Text is checked for its length and for NUL; numbers are
/// stored as doubles, as better-sqlite3 binds them. A boolean or a nested value cannot be stored.
fn sql_value(table: &str, column: &str, value: &Value) -> Result<SqlValue> {
    match value {
        Value::Null => Ok(SqlValue::Null),
        Value::String(text) => {
            if text.encode_utf16().count() > MAX_IMPORT_TEXT_UNITS || text.contains('\0') {
                return Err(Error::Message(format!(
                    "Valor inválido na coluna {column} da tabela {table}."
                )));
            }
            Ok(SqlValue::Text(text.clone()))
        }
        Value::Number(number) => {
            let number = number.as_f64().ok_or_else(|| {
                Error::Message(format!(
                    "Valor numérico inválido na coluna {column} da tabela {table}."
                ))
            })?;
            Ok(SqlValue::Real(number))
        }
        Value::Bool(_) | Value::Array(_) | Value::Object(_) => Err(Error::Message(format!(
            "Tipo de valor não suportado na coluna {column} da tabela {table}."
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_binds_as_a_double() {
        assert_eq!(
            sql_value("t", "c", &json!(3)).expect("number"),
            SqlValue::Real(3.0)
        );
    }

    #[test]
    fn a_nested_value_is_refused() {
        assert!(sql_value("t", "c", &json!({"a": 1})).is_err());
        assert!(sql_value("t", "c", &json!(true)).is_err());
    }

    #[test]
    fn text_is_counted_in_utf16_units() {
        // Each emoji is two UTF-16 units, so this text is one unit over the limit.
        let emoji = "😀".repeat(MAX_IMPORT_TEXT_UNITS / 2 + 1);
        assert!(sql_value("t", "c", &json!(emoji)).is_err());
        assert!(sql_value("t", "c", &json!("é".repeat(10))).is_ok());
    }
}
