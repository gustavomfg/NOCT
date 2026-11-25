//! The files a provider call carries: the project tree (names and sizes, no content) and the text of the chosen files.
//!
//! The tree is walked from the project folder without following a symbolic link, skips the ignored folders, and stops at
//! `MAXIMUM_TREE_ENTRIES` files. A sensitive file is listed for the window as blocked, and is never sent, not even by name.
//! The person's choice of files is kept per project in the settings, under the hidden `maintenance.` prefix; without a
//! choice, the files of the top level are chosen, as before. Whatever is chosen, the files pass `context::select` again
//! (32 KiB per file, 128 KiB in all) and are read with the path rules of a proposed change (`resolve_target`, `read_base`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use noct_core::ai::context::{self, ContextFile, TreeEntry, is_sendable_path};
use noct_core::file_change::check_relative_path;
use noct_core::limits::provider_context::{MAXIMUM_SELECTED_FILES, MAXIMUM_TREE_ENTRIES};
use noct_db::store::Store;
use serde::{Deserialize, Serialize};

use crate::error::CommandError;
use crate::file_changes::{read_base, resolve_target};
use crate::services;

const SELECTION_PREFIX: &str = "maintenance.context.";

/// The request of the tree.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TreeRequest {
    pub workspace: String,
}

/// The request that keeps a choice of files. `paths: null` goes back to the default choice.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SelectionRequest {
    pub workspace: String,
    pub paths: Option<Vec<String>>,
}

/// One file of the tree as the window lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowEntry {
    pub path: String,
    pub bytes: u64,
    /// A sensitive or ignored file: shown, never chosen, never sent.
    pub blocked: bool,
}

/// The tree and the choice of a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextTree {
    pub entries: Vec<WindowEntry>,
    /// Whether the project has more files than the tree lists.
    pub truncated: bool,
    /// The chosen files, which fit the caps.
    pub selected: Vec<String>,
    /// Whether the choice is the person's, not the default.
    pub custom: bool,
}

/// What one call carries.
pub struct Gathered {
    pub files: Vec<(String, String)>,
    pub tree: String,
}

impl Gathered {
    pub fn sent_paths(&self) -> Vec<String> {
        self.files.iter().map(|(path, _)| path.clone()).collect()
    }
}

/// Walks the project: files only, in name order, folder by folder, no link followed, ignored folders skipped.
pub fn walk(root: &Path) -> Result<(Vec<WindowEntry>, bool), CommandError> {
    let mut entries = Vec::new();
    let mut folders = vec![String::new()];
    let mut truncated = false;
    while let Some(folder) = folders.pop() {
        let directory = if folder.is_empty() {
            root.to_path_buf()
        } else {
            root.join(&folder)
        };
        let Ok(read) = fs::read_dir(&directory) else {
            if folder.is_empty() {
                return Err(CommandError::UnknownWorkspace);
            }
            continue;
        };
        let mut names: Vec<(String, fs::Metadata)> = read
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let metadata = entry.path().symlink_metadata().ok()?;
                Some((name, metadata))
            })
            .collect();
        names.sort_by(|left, right| left.0.cmp(&right.0));
        let mut subfolders = Vec::new();
        for (name, metadata) in names {
            let path = if folder.is_empty() {
                name
            } else {
                format!("{folder}/{name}")
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if !context::is_ignored_relative_path(&path) {
                    subfolders.push(path);
                }
            } else if metadata.is_file() {
                if entries.len() >= MAXIMUM_TREE_ENTRIES {
                    truncated = true;
                    break;
                }
                let blocked = !is_sendable_path(&path) || check_relative_path(&path).is_err();
                entries.push(WindowEntry {
                    path,
                    bytes: metadata.len(),
                    blocked,
                });
            }
        }
        if truncated {
            break;
        }
        // The stack takes the last folder first, so the folders are pushed in reverse to be walked in name order.
        folders.extend(subfolders.into_iter().rev());
    }
    Ok((entries, truncated))
}

fn setting_of(workspace: &str) -> String {
    format!("{SELECTION_PREFIX}{workspace}")
}

fn saved_selection(store: &Store, workspace: &str) -> Result<Option<Vec<String>>, CommandError> {
    let saved = store
        .settings()
        .value(&setting_of(workspace))
        .map_err(|_| CommandError::Persistence)?;
    Ok(saved
        .filter(|text| !text.is_empty())
        .and_then(|text| serde_json::from_str(&text).ok()))
}

/// The chosen files that exist in the tree, are not blocked, and fit the caps, in the order given.
fn fitting(entries: &[WindowEntry], wanted: &[String]) -> Vec<String> {
    let candidates: Vec<ContextFile> = wanted
        .iter()
        .filter_map(|path| {
            entries
                .iter()
                .find(|entry| &entry.path == path && !entry.blocked)
                .map(|entry| ContextFile {
                    relative_path: entry.path.clone(),
                    bytes: entry.bytes,
                })
        })
        .collect();
    context::select(&candidates).included
}

/// The default choice: the files of the top level, in name order, as the chat chose before.
fn default_selection(entries: &[WindowEntry]) -> Vec<String> {
    let top: Vec<String> = entries
        .iter()
        .filter(|entry| !entry.path.contains('/'))
        .map(|entry| entry.path.clone())
        .collect();
    fitting(entries, &top)
}

fn tree_of(store: &Store, workspace: &str) -> Result<ContextTree, CommandError> {
    let (entries, truncated) = walk(Path::new(workspace))?;
    let saved = saved_selection(store, workspace)?;
    let (selected, custom) = match saved {
        Some(paths) => (fitting(&entries, &paths), true),
        None => (default_selection(&entries), false),
    };
    Ok(ContextTree {
        entries,
        truncated,
        selected,
        custom,
    })
}

/// The tree and the choice of an authorized project.
pub fn tree(
    store: &Store,
    blocked: &[PathBuf],
    workspace: &str,
) -> Result<ContextTree, CommandError> {
    services::authorized_workspace(store, blocked, workspace)?;
    tree_of(store, workspace)
}

/// Keeps the choice of files of a project, or goes back to the default. Every path must be a project path that may be
/// sent; a list with any other path is refused whole.
pub fn set_selection(
    store: &Store,
    blocked: &[PathBuf],
    request: &SelectionRequest,
) -> Result<ContextTree, CommandError> {
    services::authorized_workspace(store, blocked, &request.workspace)?;
    let value = match &request.paths {
        None => String::new(),
        Some(paths) => {
            if paths.len() > MAXIMUM_SELECTED_FILES
                || paths
                    .iter()
                    .any(|path| !is_sendable_path(path) || check_relative_path(path).is_err())
            {
                return Err(CommandError::InvalidInput);
            }
            let mut unique: Vec<&String> = Vec::new();
            for path in paths {
                if !unique.contains(&path) {
                    unique.push(path);
                }
            }
            serde_json::to_string(&unique).map_err(|_| CommandError::Internal)?
        }
    };
    let values = BTreeMap::from([(setting_of(&request.workspace), value)]);
    store
        .settings()
        .set(&values)
        .map_err(|_| CommandError::Persistence)?;
    tree_of(store, &request.workspace)
}

/// What a call of an authorized project carries: the chosen files read as text, and the tree of the files that may be
/// sent. A file that cannot be read within the rules at the time it is read is left out.
pub fn gather(
    store: &Store,
    blocked: &[PathBuf],
    workspace: &str,
) -> Result<Gathered, CommandError> {
    let chosen = tree(store, blocked, workspace)?;
    let root = Path::new(workspace);
    let files = chosen
        .selected
        .iter()
        .filter_map(|path| Some((path.clone(), read_project_text(root, path)?)))
        .collect();
    let tree_entries: Vec<TreeEntry> = chosen
        .entries
        .iter()
        .filter(|entry| !entry.blocked)
        .map(|entry| TreeEntry {
            path: entry.path.clone(),
            bytes: entry.bytes,
        })
        .collect();
    Ok(Gathered {
        files,
        tree: context::tree_text(&tree_entries),
    })
}

/// The text of a project file, read with the rules of a proposed change: a project path that may be sent, no link on the
/// way, nothing outside the project, text within the per-file cap.
pub fn read_project_text(root: &Path, path: &str) -> Option<String> {
    if !is_sendable_path(path) {
        return None;
    }
    let names = check_relative_path(path).ok()?;
    let target = resolve_target(root, &names).ok()?;
    read_base(&target).ok().flatten()
}
