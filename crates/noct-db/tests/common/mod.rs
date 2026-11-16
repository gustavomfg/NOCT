//! Helpers shared by the integration tests.

use std::fs;
use std::path::PathBuf;

/// A clean directory under the Cargo target dir, so tests need no extra dependency.
pub fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch directory");
    dir
}
