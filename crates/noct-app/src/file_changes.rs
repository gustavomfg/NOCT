//! Proposed changes to the text files of a project. A change is proposed, approved or rejected one at a time, and an applied
//! change can be undone. Nothing is written until the person approves it. The rules that need no disk (the relative path,
//! the sensitive names, the moves of a change) are in `noct-core::file_change`; this module checks the disk and writes.
//!
//! Every operation holds `Persistence::journal_lock`, reads the journal file, checks, and saves the journal before it touches
//! the project file. A failed write sets the journal back, so the journal never says a change is in effect when the file does
//! not hold it. A crash between the journal save and the write leaves the change marked applied while the file is unchanged;
//! undo then refuses, because the file does not hold the content.

use std::fs;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use noct_core::file_change::{
    FileChange, FileChangeJournal, FileChangeProposal, FileChangeStatus, JournalRefusal,
    PathRefusal, check_relative_path, fits_limit,
};
use noct_core::limits::file_change::MAXIMUM_FILE_BYTES;
use noct_db::persistence::atomic_file::write_atomic_file;
use uuid::Uuid;

use crate::bootstrap::Persistence;
use crate::error::CommandError;
use crate::services;

/// The name of the journal file, in the data folder. It is never inside a project.
pub const JOURNAL_FILE: &str = "file-changes.json";
/// The permission of the journal, which holds the text of project files.
const JOURNAL_MODE: u32 = 0o600;
/// The permission of a file that did not exist, and so has no mode to keep.
const NEW_FILE_MODE: u32 = 0o644;

/// The changes of one project, in the order they were proposed.
pub fn list(
    persistence: &Persistence,
    blocked: &[PathBuf],
    workspace: &str,
) -> Result<Vec<FileChange>, CommandError> {
    let _guard = lock(persistence)?;
    authorize(persistence, blocked, workspace)?;
    Ok(load(&persistence.data_directory)?.for_workspace(workspace))
}

/// Proposes a change to one file. The file is read as its base, and nothing is written. The path must pass the rules of
/// `noct-core`, every folder on it must be a real folder inside the project, and the file must be a regular text file within
/// the limit, or not exist.
pub fn propose(
    persistence: &Persistence,
    blocked: &[PathBuf],
    proposal: FileChangeProposal,
) -> Result<FileChange, CommandError> {
    let _guard = lock(persistence)?;
    if !fits_limit(&proposal.content) {
        return Err(CommandError::FileNotEditable);
    }
    let names = check_relative_path(&proposal.path).map_err(path_refusal)?;
    authorize(persistence, blocked, &proposal.workspace)?;
    let target = resolve_target(Path::new(&proposal.workspace), &names)?;
    let base = read_base(&target)?;
    let change = FileChange {
        id: Uuid::new_v4().to_string(),
        workspace: proposal.workspace,
        path: names.join("/"),
        status: FileChangeStatus::Pending,
        base,
        content: proposal.content,
    };
    let mut journal = load(&persistence.data_directory)?;
    journal.add(change.clone()).map_err(journal_refusal)?;
    save(&persistence.data_directory, &journal)?;
    Ok(change)
}

/// Approves a pending change and writes its content. The file must still hold the base the change was proposed against; if
/// it does not, nothing is written and the change stays pending.
pub fn approve(
    persistence: &Persistence,
    blocked: &[PathBuf],
    id: &str,
) -> Result<FileChange, CommandError> {
    let _guard = lock(persistence)?;
    let mut journal = load(&persistence.data_directory)?;
    let change = find(&journal, id)?;
    if !change.status.allows(FileChangeStatus::Applied) {
        return Err(CommandError::InvalidChangeTransition);
    }
    authorize(persistence, blocked, &change.workspace)?;
    let target = target_of(&change)?;
    match read_base(&target) {
        Ok(current) if current == change.base => {}
        Err(CommandError::Persistence) => return Err(CommandError::Persistence),
        _ => return Err(CommandError::ChangeConflict),
    }
    let mode = mode_of(&target);
    set_status(&mut journal, id, FileChangeStatus::Applied)?;
    save(&persistence.data_directory, &journal)?;
    if write_content(&target, &change.content, mode).is_err() {
        set_status(&mut journal, id, FileChangeStatus::Pending)?;
        save(&persistence.data_directory, &journal)?;
        return Err(CommandError::ChangeWrite);
    }
    Ok(FileChange {
        status: FileChangeStatus::Applied,
        ..change
    })
}

/// Rejects a pending change. Nothing in the project is touched, so the project does not need to be authorized.
pub fn reject(persistence: &Persistence, id: &str) -> Result<FileChange, CommandError> {
    let _guard = lock(persistence)?;
    let mut journal = load(&persistence.data_directory)?;
    let change = find(&journal, id)?;
    if !change.status.allows(FileChangeStatus::Rejected) {
        return Err(CommandError::InvalidChangeTransition);
    }
    set_status(&mut journal, id, FileChangeStatus::Rejected)?;
    save(&persistence.data_directory, &journal)?;
    Ok(FileChange {
        status: FileChangeStatus::Rejected,
        ..change
    })
}

/// Undoes an applied change. The file must still hold the content the change wrote; if it changed since, nothing is written.
/// An undone change that created its file removes it; otherwise the base is written back.
pub fn undo(
    persistence: &Persistence,
    blocked: &[PathBuf],
    id: &str,
) -> Result<FileChange, CommandError> {
    let _guard = lock(persistence)?;
    let mut journal = load(&persistence.data_directory)?;
    let change = find(&journal, id)?;
    if !change.status.allows(FileChangeStatus::Undone) {
        return Err(CommandError::InvalidChangeTransition);
    }
    authorize(persistence, blocked, &change.workspace)?;
    let target = target_of(&change)?;
    match read_base(&target) {
        Ok(Some(current)) if current == change.content => {}
        Err(CommandError::Persistence) => return Err(CommandError::Persistence),
        _ => return Err(CommandError::ChangeConflict),
    }
    let mode = mode_of(&target);
    set_status(&mut journal, id, FileChangeStatus::Undone)?;
    save(&persistence.data_directory, &journal)?;
    let written = match &change.base {
        Some(base) => write_content(&target, base, mode),
        None => fs::remove_file(&target).map_err(|_| CommandError::ChangeWrite),
    };
    if written.is_err() {
        set_status(&mut journal, id, FileChangeStatus::Applied)?;
        save(&persistence.data_directory, &journal)?;
        return Err(CommandError::ChangeWrite);
    }
    Ok(FileChange {
        status: FileChangeStatus::Undone,
        ..change
    })
}

fn lock(persistence: &Persistence) -> Result<std::sync::MutexGuard<'_, ()>, CommandError> {
    persistence
        .journal_lock
        .lock()
        .map_err(|_| CommandError::Internal)
}

/// The project must be saved, authorized and available, as for every write of the shell.
fn authorize(
    persistence: &Persistence,
    blocked: &[PathBuf],
    workspace: &str,
) -> Result<(), CommandError> {
    let store = persistence
        .store
        .lock()
        .map_err(|_| CommandError::Internal)?;
    services::authorized_workspace(&store, blocked, workspace)
}

fn find(journal: &FileChangeJournal, id: &str) -> Result<FileChange, CommandError> {
    journal.find(id).cloned().ok_or(CommandError::UnknownChange)
}

/// The path of a change, checked again before it is used. A change read from the journal is trusted no more than a request.
fn target_of(change: &FileChange) -> Result<PathBuf, CommandError> {
    let names = check_relative_path(&change.path).map_err(path_refusal)?;
    resolve_target(Path::new(&change.workspace), &names)
}

fn set_status(
    journal: &mut FileChangeJournal,
    id: &str,
    next: FileChangeStatus,
) -> Result<(), CommandError> {
    journal.set_status(id, next).map_err(journal_refusal)
}

fn path_refusal(_: PathRefusal) -> CommandError {
    CommandError::PathNotAllowed
}

fn journal_refusal(refusal: JournalRefusal) -> CommandError {
    match refusal {
        JournalRefusal::Unknown => CommandError::UnknownChange,
        JournalRefusal::InvalidTransition => CommandError::InvalidChangeTransition,
        JournalRefusal::TooManyOpen => CommandError::TooManyOpenChanges,
    }
}

/// Walks the names from the project folder. Every name that exists must be a real file or folder, not a symbolic link; every
/// folder on the way must be a folder; and the parent must resolve inside the project, so that no link on the way can lead
/// out. The last name may not exist yet (a new file). Nothing is created.
pub fn resolve_target(root: &Path, names: &[String]) -> Result<PathBuf, CommandError> {
    let canonical_root = fs::canonicalize(root).map_err(|_| CommandError::UnknownWorkspace)?;
    let mut current = root.to_path_buf();
    for (index, name) in names.iter().enumerate() {
        let last = index + 1 == names.len();
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(CommandError::PathNotAllowed);
                }
                if !last && !metadata.is_dir() {
                    return Err(CommandError::PathNotAllowed);
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if !last {
                    return Err(CommandError::PathNotAllowed);
                }
            }
            Err(_) => return Err(CommandError::Persistence),
        }
    }
    let parent = current.parent().ok_or(CommandError::PathNotAllowed)?;
    let canonical_parent = fs::canonicalize(parent).map_err(|_| CommandError::PathNotAllowed)?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(CommandError::PathNotAllowed);
    }
    Ok(current)
}

/// The text of a file now, or `None` when it does not exist. A link, a folder, a file over the limit, or a file that is not
/// UTF-8 is refused, and nothing of it is kept.
pub fn read_base(target: &Path) -> Result<Option<String>, CommandError> {
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(CommandError::Persistence),
    };
    if !metadata.is_file() {
        return Err(CommandError::PathNotAllowed);
    }
    if metadata.len() > MAXIMUM_FILE_BYTES {
        return Err(CommandError::FileNotEditable);
    }
    let file = fs::File::open(target).map_err(|_| CommandError::Persistence)?;
    let mut bytes = Vec::new();
    file.take(MAXIMUM_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CommandError::Persistence)?;
    if bytes.len() as u64 > MAXIMUM_FILE_BYTES {
        return Err(CommandError::FileNotEditable);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| CommandError::FileNotEditable)
}

fn write_content(target: &Path, content: &str, mode: u32) -> Result<(), CommandError> {
    write_atomic_file(target, content.as_bytes(), mode).map_err(|_| CommandError::ChangeWrite)
}

/// The permission the file has now, so a write keeps it. A file that does not exist gets the usual mode.
#[cfg(unix)]
fn mode_of(target: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(target)
        .map(|metadata| metadata.permissions().mode() & 0o7777)
        .unwrap_or(NEW_FILE_MODE)
}

#[cfg(not(unix))]
fn mode_of(_target: &Path) -> u32 {
    NEW_FILE_MODE
}

fn load(data_directory: &Path) -> Result<FileChangeJournal, CommandError> {
    match fs::read(data_directory.join(JOURNAL_FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| CommandError::Persistence),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(FileChangeJournal::default()),
        Err(_) => Err(CommandError::Persistence),
    }
}

/// A journal that cannot be read is refused and left as it is, so it is never overwritten by an empty one.
fn save(data_directory: &Path, journal: &FileChangeJournal) -> Result<(), CommandError> {
    let bytes = serde_json::to_vec(journal).map_err(|_| CommandError::Internal)?;
    write_atomic_file(&data_directory.join(JOURNAL_FILE), &bytes, JOURNAL_MODE)
        .map_err(|_| CommandError::Persistence)
}
