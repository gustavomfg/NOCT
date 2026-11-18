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
use noct_db::observer::OperationObserver;
use noct_db::store::Store;
use thiserror::Error;

use crate::logging::{DiagnosticGate, ShellObserver};

/// The environment variable that names the data directory. It must be an absolute path.
pub const DATA_DIRECTORY_VARIABLE: &str = "NOCT_DATA_DIR";

/// The open persistence of the shell. The store is shared by the commands, and the gate is the sink the
/// store writes its diagnostics to. The observer measures the commands and warns through the same gate.
#[derive(Clone)]
pub struct Persistence {
    pub store: Arc<Mutex<Store>>,
    pub diagnostics: Arc<DiagnosticGate>,
    pub observer: Arc<dyn OperationObserver>,
    /// The folder of the database, the file log and the provider key vault. Never sent to the interface.
    pub data_directory: PathBuf,
    /// Held while a proposed change is read or written, so two commands never race on the journal file.
    pub journal_lock: Arc<Mutex<()>>,
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

/// The category of the warning about a data directory outside the user's profile.
pub const PROFILE_WARNING_EVENT: &str = "bootstrap.data-directory";

/// The warning written when the data directory is outside the user's profile. It is written in English, like the
/// other diagnostics. It names no path, because the log is shared when it is sent for support.
pub const OUTSIDE_PROFILE_MESSAGE: &str = "The data directory is outside the user profile. Its files are protected only by the permissions of that folder.";

/// Opens the store and the file log in `directory`, creating the directory when it does not exist. The
/// diagnostic gate starts with the saved `diagnosticMode`, as the TypeScript logger does at startup.
///
/// On Windows the directory is checked against the user's profile folder, and a warning is written when it is
/// outside it. The shell still opens the directory: the check informs, it does not refuse.
pub fn open(directory: &Path) -> Result<Persistence, BootstrapError> {
    open_with_profile(directory, user_profile().as_deref())
}

/// Opens as `open` does, and checks the directory against `profile`, the user's profile folder. `None` means the
/// profile is not known or not checked, and then no warning is written.
pub fn open_with_profile(
    directory: &Path,
    profile: Option<&Path>,
) -> Result<Persistence, BootstrapError> {
    fs::create_dir_all(directory).map_err(BootstrapError::DataDirectory)?;
    let log = Arc::new(FileLogSink::open(directory).map_err(BootstrapError::Log)?);
    warn_if_outside_profile(log.as_ref(), directory, profile);
    let diagnostics = Arc::new(DiagnosticGate::new(log, false));
    let sink: Arc<dyn DiagnosticSink> = diagnostics.clone();
    let observer: Arc<dyn OperationObserver> = Arc::new(ShellObserver::new(sink.clone()));
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
        observer,
        data_directory: directory.to_path_buf(),
        journal_lock: Arc::new(Mutex::new(())),
    })
}

/// The profile folder of the user on Windows, where the user's files are kept. The other systems protect the data
/// folder with POSIX modes (0700 and 0600), so no profile is checked there.
fn user_profile() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    } else {
        None
    }
}

/// Writes the profile warning when `directory` is outside `profile`. Returns whether it was written.
fn warn_if_outside_profile(
    sink: &dyn DiagnosticSink,
    directory: &Path,
    profile: Option<&Path>,
) -> bool {
    let Some(profile) = profile else {
        return false;
    };
    if !is_outside_profile(directory, profile) {
        return false;
    }
    sink.warn(PROFILE_WARNING_EVENT, OUTSIDE_PROFILE_MESSAGE);
    true
}

/// Whether `directory` is outside `profile`. Both paths are resolved when they exist, and compared without case and
/// with `\` as the separator, as Windows names them. A folder that only starts with the profile's name, such as
/// `C:\Users\ana2` for `C:\Users\ana`, is outside it.
pub fn is_outside_profile(directory: &Path, profile: &Path) -> bool {
    let directory = comparable(directory);
    let profile = comparable(profile);
    let inside_prefix = if profile.ends_with('\\') {
        profile.clone()
    } else {
        format!("{profile}\\")
    };
    !(directory == profile || directory.starts_with(&inside_prefix))
}

fn comparable(path: &Path) -> String {
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy().into_owned();
    let unprefixed = text.strip_prefix(r"\\?\").unwrap_or(&text);
    unprefixed.replace('/', r"\").to_lowercase()
}
