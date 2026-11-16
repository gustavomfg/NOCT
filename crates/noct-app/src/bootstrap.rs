//! Opens the persistence of the shell: the data directory, the database and the diagnostic file log.
//!
//! The directory is the one named by `NOCT_DATA_DIR` when it is set, and the application data folder
//! of Tauri otherwise. That default is not the folder the Electron application uses (`Nocturne Studio`),
//! so the shell never opens the database of an existing installation by default.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use noct_core::ipc::settings::from_saved;
use noct_db::OpenOptions;
use noct_db::diagnostics::DiagnosticSink;
use noct_db::file_log::FileLogSink;
use noct_db::store::Store;
use thiserror::Error;

use crate::logging::DiagnosticGate;

/// The environment variable that names the data directory. It must be an absolute path.
pub const DATA_DIRECTORY_VARIABLE: &str = "NOCT_DATA_DIR";

/// The open persistence of the shell. The store is shared by the commands, and the gate is the sink the
/// store writes its diagnostics to.
pub struct Persistence {
    pub store: Arc<Mutex<Store>>,
    pub diagnostics: Arc<DiagnosticGate>,
}

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("NOCT_DATA_DIR precisa ser um caminho absoluto.")]
    RelativeDataDirectory,
    #[error("Não foi possível preparar a pasta de dados.")]
    DataDirectory(#[source] io::Error),
    #[error("Não foi possível abrir o registro de diagnóstico.")]
    Log(#[source] io::Error),
    #[error("Não foi possível abrir o banco de dados.")]
    Database(#[source] noct_db::Error),
}

/// The data directory for this run: the configured one, or the default when nothing is configured.
pub fn resolve_data_directory(
    configured: Option<OsString>,
    default: PathBuf,
) -> Result<PathBuf, BootstrapError> {
    match configured {
        None => Ok(default),
        Some(value) => {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                Ok(path)
            } else {
                Err(BootstrapError::RelativeDataDirectory)
            }
        }
    }
}

/// Opens the store and the file log in `directory`, creating the directory when it does not exist. The
/// diagnostic gate starts with the saved `diagnosticMode`, as the TypeScript logger does at startup.
pub fn open(directory: &Path) -> Result<Persistence, BootstrapError> {
    fs::create_dir_all(directory).map_err(BootstrapError::DataDirectory)?;
    let log = Arc::new(FileLogSink::open(directory).map_err(BootstrapError::Log)?);
    let diagnostics = Arc::new(DiagnosticGate::new(log, false));
    let sink: Arc<dyn DiagnosticSink> = diagnostics.clone();
    let store = Store::open_with(
        directory,
        OpenOptions {
            diagnostics: sink,
            ..OpenOptions::default()
        },
    )
    .map_err(BootstrapError::Database)?;
    let saved = store.settings().get().map_err(BootstrapError::Database)?;
    diagnostics.set_enabled(from_saved(&saved).diagnostic_mode == Some(true));
    Ok(Persistence {
        store: Arc::new(Mutex::new(store)),
        diagnostics,
    })
}
