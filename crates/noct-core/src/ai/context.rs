//! Which project files may be sent to a provider as context, and how much of them. Port of the path rules of
//! `electron/semantic-index/SemanticPrivacyPolicy.ts` (`isPotentiallySensitivePath`) and of the ignored directories of
//! `electron/workspaces/WorkspacePathPolicy.ts`.
//!
//! One deliberate difference from the TypeScript rules, recorded in `docs/status.md`: a file whose whole name is a
//! sensitive extension, such as `.pem`, is sensitive here. The TypeScript check reads such a name as a dotfile with no
//! extension and lets it through.
//!
//! The rules do no I/O. The app layer lists the candidates and reads only the files that `select` includes.

use crate::limits::provider_context as limits;

/// Directory names that are never sent. A match on any segment of the path excludes the file, as in the TypeScript rule.
pub const IGNORED_DIRECTORIES: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "release",
    "out",
    "coverage",
    ".nocturne",
    ".next",
    ".turbo",
    ".cache",
    ".parcel-cache",
    ".pytest_cache",
    ".venv",
    "target",
    "build",
];

const IGNORED_PREFIX: &str = ".nocturne-rollback-";

const SENSITIVE_NAMES: &[&str] = &[
    ".env",
    ".env.local",
    ".env.development",
    ".env.production",
    "credentials.json",
    "secrets.json",
    "secrets.yaml",
    "secrets.yml",
];

const SENSITIVE_PARTS: &[&str] = &["secret", "credential", "token"];

const SENSITIVE_EXTENSIONS: &[&str] = &[".pem", ".key", ".p12", ".pfx", ".crt", ".cer"];

/// Whether any segment of the relative path is an ignored directory.
pub fn is_ignored_relative_path(relative: &str) -> bool {
    normalize(relative).split('/').any(|segment| {
        IGNORED_DIRECTORIES.contains(&segment) || segment.starts_with(IGNORED_PREFIX)
    })
}

/// Whether the file's content must not leave the machine. The check is on the lowercase base name: a fixed name, a part
/// of the name, or a sensitive extension.
pub fn is_potentially_sensitive_path(relative: &str) -> bool {
    let normalized = normalize(relative);
    let base = normalized
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_lowercase();
    SENSITIVE_NAMES.contains(&base.as_str())
        || SENSITIVE_PARTS.iter().any(|part| base.contains(part))
        || SENSITIVE_EXTENSIONS
            .iter()
            .any(|extension| base.ends_with(extension))
}

/// Whether the path names a place inside the project: relative, with no `..` segment and no empty segment.
pub fn is_inside_project(relative: &str) -> bool {
    let normalized = normalize(relative);
    !normalized.is_empty()
        && !normalized.starts_with('/')
        && !normalized.contains(':')
        && normalized
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "..")
}

fn normalize(relative: &str) -> String {
    relative.replace('\\', "/")
}

/// A file the caller lists as a candidate for the context, with its size in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFile {
    pub relative_path: String,
    pub bytes: u64,
}

/// Why a candidate is not sent. The codes are stable, and they name no content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exclusion {
    OutsideProject,
    Ignored,
    Sensitive,
    TooLarge,
    OverBudget,
}

impl Exclusion {
    pub fn code(self) -> &'static str {
        match self {
            Self::OutsideProject => "outside-project",
            Self::Ignored => "ignored",
            Self::Sensitive => "sensitive",
            Self::TooLarge => "too-large",
            Self::OverBudget => "over-budget",
        }
    }
}

/// The files that will be sent, in the order given, and the ones that will not, with the reason of each.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub included: Vec<String>,
    pub excluded: Vec<(String, Exclusion)>,
    pub bytes: u64,
}

/// Selects the files for the context. The order of the candidates is kept, so the caller decides which files come
/// first. A file that is outside the project, ignored, sensitive, or larger than one file may be is excluded and does
/// not count against the budget. Once a file does not fit in the budget, that file and every later file are excluded
/// as over budget: the selection never skips ahead to a smaller file.
pub fn select(candidates: &[ContextFile]) -> Selection {
    let mut selection = Selection::default();
    let mut full = false;
    for candidate in candidates {
        let path = candidate.relative_path.clone();
        let exclusion = if !is_inside_project(&path) {
            Some(Exclusion::OutsideProject)
        } else if is_ignored_relative_path(&path) {
            Some(Exclusion::Ignored)
        } else if is_potentially_sensitive_path(&path) {
            Some(Exclusion::Sensitive)
        } else if candidate.bytes > limits::MAXIMUM_FILE_BYTES {
            Some(Exclusion::TooLarge)
        } else if full || selection.bytes + candidate.bytes > limits::MAXIMUM_CONTEXT_BYTES {
            full = true;
            Some(Exclusion::OverBudget)
        } else {
            None
        };
        match exclusion {
            Some(reason) => selection.excluded.push((path, reason)),
            None => {
                selection.bytes += candidate.bytes;
                selection.included.push(path);
            }
        }
    }
    selection
}
