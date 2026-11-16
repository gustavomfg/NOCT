//! NOCT local persistence: SQLite runtime, schema migrations and repositories.
//!
//! Port of `electron/database`. The on-disk format is unchanged: the same
//! `nocturne.db` file, the same schema version and the same migration history, so
//! databases written by the TypeScript application open here and the reverse stays
//! possible until the TypeScript implementation is retired.

mod clock;
mod codec;
pub mod diagnostics;
pub mod file_log;
pub mod migrations;
pub mod observer;
pub mod persistence;
pub mod recovery;
pub mod repositories;
pub mod runtime;
pub mod schema;
mod statistics;
pub mod store;
pub mod transaction;

pub use runtime::{Database, OpenOptions};
pub use statistics::StatisticsOutcome;

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

impl From<noct_core::validation::ContractError> for Error {
    fn from(error: noct_core::validation::ContractError) -> Self {
        Self::Message(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
