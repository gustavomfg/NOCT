//! Schema migrations 1..=32. `generated` is transpiled from
//! `electron/database/migrations.ts`; this module owns the transaction and the
//! `user_version` bookkeeping, mirroring `migrateDatabase`.

mod generated;

pub(crate) use crate::Result;
pub(crate) use crate::schema::has_column;

use noct_core::limits::DATABASE_SCHEMA_VERSION;
use rusqlite::Connection;

/// Applies every migration after `current` up to and including `target`, inside one
/// transaction. An existing database (`current > 0`) first re-runs migration 1, which
/// is idempotent and repairs legacy snapshots that recorded version 1 too early.
pub fn migrate_to(conn: &mut Connection, current: u32, target: u32) -> Result<()> {
    let transaction = conn.transaction()?;
    if current > 0 {
        generated::apply(1, &transaction)?;
    }
    for version in (current + 1)..=target {
        generated::apply(version, &transaction)?;
        transaction.pragma_update(None, "user_version", version)?;
    }
    transaction.commit()?;
    Ok(())
}

/// Migrates to the schema version this build supports.
pub fn migrate_latest(conn: &mut Connection, current: u32) -> Result<()> {
    migrate_to(conn, current, DATABASE_SCHEMA_VERSION as u32)
}
