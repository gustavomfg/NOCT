//! Atomic units of work. Port of `DatabaseTransactionRunner` in `DatabaseTransaction.ts`.
//!
//! A call made outside a transaction opens one. A call made inside another becomes a
//! savepoint, which is how better-sqlite3 treats nested `db.transaction()` calls. A
//! failed unit rolls back its own writes and nothing else.

use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::Connection;

use crate::Result;

static SAVEPOINTS: AtomicU64 = AtomicU64::new(0);

pub fn run<T>(conn: &Connection, operation: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    if conn.is_autocommit() {
        conn.execute_batch("BEGIN")?;
        let outcome = operation(conn);
        let finished = match &outcome {
            Ok(_) => conn.execute_batch("COMMIT"),
            Err(_) => Ok(()),
        };
        match (outcome, finished) {
            (Ok(value), Ok(())) => Ok(value),
            (Ok(_), Err(error)) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(error.into())
            }
            (Err(error), _) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    } else {
        let name = format!(
            "noct_savepoint_{}",
            SAVEPOINTS.fetch_add(1, Ordering::Relaxed)
        );
        conn.execute_batch(&format!("SAVEPOINT {name}"))?;
        let outcome = operation(conn);
        let finished = match &outcome {
            Ok(_) => conn.execute_batch(&format!("RELEASE {name}")),
            Err(_) => Ok(()),
        };
        match (outcome, finished) {
            (Ok(value), Ok(())) => Ok(value),
            (Ok(_), Err(error)) => {
                let _ = conn.execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name}"));
                Err(error.into())
            }
            (Err(error), _) => {
                let _ = conn.execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name}"));
                Err(error)
            }
        }
    }
}
