//! NOCT local persistence: SQLite runtime, schema migrations and repositories.
//!
//! Port of `electron/database`. The on-disk format is unchanged: the same
//! `nocturne.db` file, the same schema version and the same migration history, so
//! databases written by the TypeScript application open here and the reverse stays
//! possible until the TypeScript implementation is retired.

mod clock;
pub mod migrations;
pub mod repositories;
pub mod runtime;
pub mod schema;
pub mod transaction;

pub use runtime::Database;

/// Persistence error. `Message` carries the user-facing text the application shows.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, Error>;
