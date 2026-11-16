//! Schema helpers: column probes used by migrations, and a canonical schema dump.
//!
//! The dump format is shared with the TypeScript fixture generator. The migration
//! parity test compares SHA-256 digests of these dumps, so the two implementations
//! must produce byte-identical text.

use std::fmt::Write as _;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension};

use crate::Result;

/// True when `table` already declares `column`. `table` must be a literal name.
pub fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn user_version(conn: &Connection) -> Result<u32> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(u32::try_from(version).unwrap_or(0))
}

/// `PRAGMA quick_check` first row: `"ok"` for a healthy database.
pub fn quick_check(conn: &Connection) -> Result<String> {
    let result = conn
        .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .optional()?;
    Ok(result.unwrap_or_default())
}

/// Canonical text dump: user version, every schema object's normalized SQL, and every
/// table column. Whitespace in stored SQL is collapsed so formatting cannot diverge.
pub fn dump(conn: &Connection) -> Result<String> {
    let mut out = String::new();
    writeln!(out, "user_version\t{}", user_version(conn)?).unwrap();

    let mut objects = conn.prepare(
        "SELECT type, name, tbl_name, sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY type, name",
    )?;
    let mut rows = objects.query([])?;
    while let Some(row) = rows.next()? {
        let kind: String = row.get(0)?;
        let name: String = row.get(1)?;
        let table: String = row.get(2)?;
        let sql: String = row.get(3)?;
        let normalized = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        writeln!(out, "obj\t{kind}\t{name}\t{table}\t{normalized}").unwrap();
    }

    let mut tables =
        conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?;
    let table_names: Vec<String> = tables
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    for table in table_names {
        let mut columns = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = columns.query([])?;
        while let Some(row) = rows.next()? {
            let cid: i64 = row.get(0)?;
            let name: String = row.get(1)?;
            let declared = text_or_empty(row.get_ref(2)?);
            let notnull: i64 = row.get(3)?;
            let default = match row.get_ref(4)? {
                ValueRef::Null => "null".to_string(),
                other => text_or_empty(other),
            };
            let pk: i64 = row.get(5)?;
            writeln!(
                out,
                "col\t{table}\t{cid}\t{name}\t{declared}\t{notnull}\t{default}\t{pk}"
            )
            .unwrap();
        }
    }
    Ok(out)
}

fn text_or_empty(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        ValueRef::Integer(number) => number.to_string(),
        ValueRef::Real(number) => number.to_string(),
        ValueRef::Blob(_) | ValueRef::Null => String::new(),
    }
}
