//! Whether a workspace path can be used. A port of `inspectWorkspaceScope` in
//! `electron/security/WorkspaceTrust.ts`: the same checks in the same order, and the same messages.
//!
//! The inspection reads the file system, so it lives in the shell and not in the contracts.
//!
//! The TypeScript check asks the operating system with `access(R_OK | X_OK)`. The standard library has
//! no such call, so readability is checked by listing the folder and searchability by looking up its
//! `.` entry. The two agree for the owner of the folder, which is the case that matters for a workspace.

use std::env;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Component, Path, PathBuf};

use noct_core::contracts::WorkspaceAvailability;

/// The result of an inspection: the availability, and the message the interface shows when it is not
/// available. An available folder has no message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspection {
    pub availability: WorkspaceAvailability,
    pub message: Option<&'static str>,
}

const INVALID_PATH: &str = "Caminho de workspace inválido.";
const BLOCKED: &str = "Selecione uma pasta de projeto específica; raízes amplas e diretórios do sistema não são permitidos.";
const NOT_A_FOLDER: &str = "O caminho do workspace não é uma pasta.";
const PERMISSION_DENIED: &str = "Sem permissão para acessar a pasta do projeto.";
const MISSING: &str = "Pasta do projeto não encontrada.";
const UNVALIDATED: &str = "Não foi possível validar a pasta do projeto.";

/// The folders a workspace may not be: the home folder and the system folders of the platform. The
/// file system root of each path is always blocked too, and is added by [`inspect`].
pub fn default_blocked_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    if let Some(value) = env::var_os(home) {
        roots.push(PathBuf::from(value));
    }
    if cfg!(windows) {
        for variable in ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(value) = env::var_os(variable) {
                roots.push(PathBuf::from(value));
            }
        }
    } else {
        for root in [
            "/bin", "/boot", "/dev", "/etc", "/proc", "/run", "/sbin", "/sys", "/usr", "/var",
        ] {
            roots.push(PathBuf::from(root));
        }
    }
    roots
}

/// Inspects the workspace path `value`. `blocked` lists the folders that may not be a workspace; the
/// checks compare canonical paths, so a symbolic link into one of them is refused as well.
pub fn inspect(value: &str, blocked: &[PathBuf]) -> Inspection {
    if value.contains('\0') || is_device_path(value) {
        return invalid(INVALID_PATH);
    }

    let resolved = lexical_absolute(value);
    if is_blocked(&resolved, blocked) {
        return invalid(BLOCKED);
    }

    match fs::metadata(&resolved) {
        Ok(metadata) if !metadata.is_dir() => return invalid(NOT_A_FOLDER),
        Ok(_) => {}
        Err(error) => return unavailable(&error),
    }

    let candidate = match fs::canonicalize(&resolved) {
        Ok(path) => path,
        Err(error) => return unavailable(&error),
    };
    if let Err(error) = check_access(&candidate) {
        return unavailable(&error);
    }
    if is_blocked(&candidate, blocked) {
        return invalid(BLOCKED);
    }

    Inspection {
        availability: WorkspaceAvailability::Available,
        message: None,
    }
}

/// Windows device and UNC paths (`\\?\`, `\\.\` and `//`), which the TypeScript check refuses.
fn is_device_path(value: &str) -> bool {
    value.starts_with(r"\\?\") || value.starts_with(r"\\.\") || value.starts_with("//")
}

/// The absolute form of `value` with `.` and `..` resolved lexically, as `path.resolve` does. Symbolic
/// links are not followed here.
fn lexical_absolute(value: impl AsRef<Path>) -> PathBuf {
    let value = value.as_ref();
    let absolute = std::path::absolute(value).unwrap_or_else(|_| value.to_path_buf());
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// True when `path` is one of the blocked folders, or the file system root of `path`.
fn is_blocked(path: &Path, blocked: &[PathBuf]) -> bool {
    let candidate = canonical_or_lexical(path);
    let root = path.ancestors().last().map(canonical_or_lexical);
    root.as_deref() == Some(candidate.as_path())
        || blocked
            .iter()
            .map(|root| canonical_or_lexical(root))
            .any(|root| root == candidate)
}

/// The canonical form of an existing path. A path that does not exist yet keeps its lexical form.
fn canonical_or_lexical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| lexical_absolute(path))
}

/// Readable and searchable for the current user: the folder can be listed, and its entries can be looked
/// up. See the module documentation for why this is the check used here.
fn check_access(folder: &Path) -> io::Result<()> {
    fs::read_dir(folder)?;
    fs::metadata(folder.join("."))?;
    Ok(())
}

fn unavailable(error: &io::Error) -> Inspection {
    match error.kind() {
        ErrorKind::NotFound | ErrorKind::NotADirectory => Inspection {
            availability: WorkspaceAvailability::Missing,
            message: Some(MISSING),
        },
        ErrorKind::PermissionDenied => Inspection {
            availability: WorkspaceAvailability::PermissionDenied,
            message: Some(PERMISSION_DENIED),
        },
        _ => invalid(UNVALIDATED),
    }
}

fn invalid(message: &'static str) -> Inspection {
    Inspection {
        availability: WorkspaceAvailability::Invalid,
        message: Some(message),
    }
}
