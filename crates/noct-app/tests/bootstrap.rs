//! The data directory of the shell and its opening. Every directory here is a scratch directory.

mod common;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use common::Scratch;
use noct_app::bootstrap::{self, BootstrapError};

#[test]
fn without_a_configured_directory_the_default_is_used() {
    let default = PathBuf::from("/default/noct");

    let resolved = bootstrap::resolve_data_directory(None, default.clone()).expect("resolves");

    assert_eq!(resolved, default);
}

#[test]
fn a_configured_absolute_directory_replaces_the_default() {
    let scratch = Scratch::new("configured");

    let resolved = bootstrap::resolve_data_directory(
        Some(scratch.path().as_os_str().to_owned()),
        PathBuf::from("/default/noct"),
    )
    .expect("resolves");

    assert_eq!(resolved, scratch.path());
}

#[test]
fn a_relative_or_empty_directory_is_refused() {
    for configured in ["relative/noct", ""] {
        let error = bootstrap::resolve_data_directory(
            Some(OsString::from(configured)),
            PathBuf::from("/default/noct"),
        )
        .expect_err("a directory must be absolute");

        assert!(
            matches!(error, BootstrapError::RelativeDataDirectory),
            "{configured:?} is refused"
        );
    }
}

#[test]
fn opening_creates_the_database_and_the_log_in_the_directory() {
    let scratch = Scratch::new("open");
    let directory = scratch.path().join("data");

    let persistence = bootstrap::open(&directory).expect("the shell opens");

    assert!(directory.join("nocturne.db").is_file());
    assert!(directory.join("nocturne.log").is_file());
    assert!(!persistence.diagnostics.is_enabled());
}

/// The diagnostic mode saved by an earlier run is in force from the start of the next one, as the
/// TypeScript logger reads it at startup.
#[test]
fn the_saved_diagnostic_mode_is_in_force_when_the_shell_opens_again() {
    let scratch = Scratch::new("diagnostic");
    {
        let persistence = bootstrap::open(scratch.path()).expect("the shell opens");
        let mut values = BTreeMap::new();
        values.insert("diagnosticMode".to_string(), "true".to_string());
        persistence
            .store
            .lock()
            .expect("the store is not poisoned")
            .settings()
            .set(&values)
            .expect("the setting is saved");
    }

    let reopened = bootstrap::open(scratch.path()).expect("the shell opens again");

    assert!(reopened.diagnostics.is_enabled());
}

#[test]
fn a_data_directory_that_is_a_file_is_refused() {
    let scratch = Scratch::new("file");
    let file = scratch.path().join("not-a-folder");
    std::fs::write(&file, b"x").expect("the file is written");

    match bootstrap::open(&file) {
        Err(BootstrapError::DataDirectory(_)) => {}
        Err(other) => panic!("unexpected error: {other}"),
        Ok(_) => panic!("a file was opened as a data directory"),
    }
}
