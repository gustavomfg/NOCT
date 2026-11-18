//! Proposed changes to one text file of a project. A change moves from pending to applied, and an applied change may be
//! undone. This module decides what a path may be, which names are never written, the shape of the journal and which moves
//! are allowed. The filesystem work (the checks on the disk and the write) is in `noct-app`.

use serde::{Deserialize, Serialize};

use crate::limits::file_change::{
    MAXIMUM_CLOSED_CHANGES, MAXIMUM_FILE_BYTES, MAXIMUM_OPEN_CHANGES, MAXIMUM_PATH_BYTES,
};

/// The state of a change. A change is created pending; approving it applies it; undoing an applied change undoes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileChangeStatus {
    Pending,
    Applied,
    Rejected,
    Undone,
}

impl FileChangeStatus {
    /// A change that still matters to the project: it is waiting for a decision, or it is in effect and may be undone.
    pub fn is_open(self) -> bool {
        matches!(self, Self::Pending | Self::Applied)
    }

    /// Whether a change may move from `self` to `next`. The moves `Applied -> Pending` and `Undone -> Applied` are the
    /// recovery of a failed write: the journal is set back to the state the file is in.
    pub fn allows(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Pending, Self::Applied)
                | (Self::Pending, Self::Rejected)
                | (Self::Applied, Self::Undone)
                | (Self::Applied, Self::Pending)
                | (Self::Undone, Self::Applied)
        )
    }
}

/// One proposed change to one file of a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub id: String,
    /// The project folder, as the store saves it.
    pub workspace: String,
    /// The relative path of the file, with `/` between the names.
    pub path: String,
    pub status: FileChangeStatus,
    /// The text the file held when the change was proposed. `None` when the file did not exist.
    pub base: Option<String>,
    /// The whole new text of the file.
    pub content: String,
}

/// What the interface sends to propose a change.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileChangeProposal {
    pub workspace: String,
    pub path: String,
    pub content: String,
}

/// The journal of the changes of every project. It is one document, and the shell keeps it in the data folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChangeJournal {
    pub changes: Vec<FileChange>,
}

/// Why the journal refuses an operation. The shell turns each one into its own message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalRefusal {
    Unknown,
    InvalidTransition,
    TooManyOpen,
}

impl FileChangeJournal {
    pub fn find(&self, id: &str) -> Option<&FileChange> {
        self.changes.iter().find(|change| change.id == id)
    }

    /// The changes of one project, in the order they were proposed.
    pub fn for_workspace(&self, workspace: &str) -> Vec<FileChange> {
        self.changes
            .iter()
            .filter(|change| change.workspace == workspace)
            .cloned()
            .collect()
    }

    /// Adds a pending change. It is refused when the open limit is reached. Closed changes beyond their limit are dropped,
    /// oldest first, so the journal stays bounded without losing a change that is still in effect.
    pub fn add(&mut self, change: FileChange) -> Result<(), JournalRefusal> {
        if self
            .changes
            .iter()
            .filter(|item| item.status.is_open())
            .count()
            >= MAXIMUM_OPEN_CHANGES
        {
            return Err(JournalRefusal::TooManyOpen);
        }
        self.changes.push(change);
        self.drop_oldest_closed();
        Ok(())
    }

    /// Moves a change to `next`, when its status allows the move.
    pub fn set_status(&mut self, id: &str, next: FileChangeStatus) -> Result<(), JournalRefusal> {
        let change = self
            .changes
            .iter_mut()
            .find(|change| change.id == id)
            .ok_or(JournalRefusal::Unknown)?;
        if !change.status.allows(next) {
            return Err(JournalRefusal::InvalidTransition);
        }
        change.status = next;
        Ok(())
    }

    fn drop_oldest_closed(&mut self) {
        let closed = self
            .changes
            .iter()
            .filter(|change| !change.status.is_open())
            .count();
        let mut excess = closed.saturating_sub(MAXIMUM_CLOSED_CHANGES);
        if excess == 0 {
            return;
        }
        self.changes.retain(|change| {
            if !change.status.is_open() && excess > 0 {
                excess -= 1;
                false
            } else {
                true
            }
        });
    }
}

/// Why a path may not be written. Every refusal is shown the same way, so the message does not say which rule matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRefusal {
    Empty,
    TooLong,
    NotRelative,
    Traversal,
    Forbidden,
    Sensitive,
}

/// Names of folders that never receive a change, at any depth.
const SENSITIVE_FOLDERS: [&str; 4] = [".git", ".ssh", ".gnupg", ".aws"];
/// Names of files that hold credentials, by their whole name.
const SENSITIVE_FILES: [&str; 6] = [
    ".env",
    ".npmrc",
    ".netrc",
    ".pypirc",
    ".git-credentials",
    ".pgpass",
];
/// Endings of files that hold keys or keystores.
const SENSITIVE_ENDINGS: [&str; 7] = [".pem", ".key", ".p12", ".pfx", ".jks", ".keystore", ".kdbx"];
/// Start of the names of private keys of SSH.
const SENSITIVE_PREFIXES: [&str; 4] = ["id_rsa", "id_ed25519", "id_ecdsa", "id_dsa"];
/// Words that mark a file as holding a secret, anywhere in its name.
const SENSITIVE_WORDS: [&str; 4] = ["credential", "secret", "private-key", "private_key"];

/// Whether a name of a path must never be written. The check is on the lower-case name.
pub fn is_sensitive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SENSITIVE_FOLDERS.contains(&lower.as_str())
        || SENSITIVE_FILES.contains(&lower.as_str())
        || lower.starts_with(".env.")
        || SENSITIVE_ENDINGS
            .iter()
            .any(|ending| lower.ends_with(ending))
        || SENSITIVE_PREFIXES
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        || SENSITIVE_WORDS.iter().any(|word| lower.contains(word))
}

/// Checks the relative path of a change and returns its names. The path must be relative, use `/`, have no empty, `.` or
/// `..` name, no NUL, no backslash and no colon, and no name that Windows would read differently (a trailing dot or space).
/// No name may be sensitive.
pub fn check_relative_path(path: &str) -> Result<Vec<String>, PathRefusal> {
    if path.is_empty() {
        return Err(PathRefusal::Empty);
    }
    if path.len() > MAXIMUM_PATH_BYTES {
        return Err(PathRefusal::TooLong);
    }
    if path.starts_with('/') {
        return Err(PathRefusal::NotRelative);
    }
    if path.contains(['\\', '\0', ':']) {
        return Err(PathRefusal::Forbidden);
    }
    let mut names = Vec::new();
    for name in path.split('/') {
        if name == ".." {
            return Err(PathRefusal::Traversal);
        }
        if name.is_empty() || name == "." || name.ends_with(['.', ' ']) {
            return Err(PathRefusal::Forbidden);
        }
        if is_sensitive_name(name) {
            return Err(PathRefusal::Sensitive);
        }
        names.push(name.to_string());
    }
    Ok(names)
}

/// Whether the text of a change fits the limit. The limit is in bytes, the same as the one the disk read uses.
pub fn fits_limit(text: &str) -> bool {
    text.len() as u64 <= MAXIMUM_FILE_BYTES
}
