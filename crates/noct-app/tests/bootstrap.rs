//! The data directory of the shell and its opening. Every directory here is a scratch directory.

mod common;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

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

/// The records of the profile warning in the log of `directory`, read after the shell has closed.
fn profile_warnings(directory: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(directory.join("nocturne.log"))
        .expect("the log file")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("each line is JSON"))
        .filter(|entry| entry["category"] == bootstrap::PROFILE_WARNING_EVENT)
        .collect()
}

#[test]
fn a_directory_outside_the_profile_is_warned_and_still_opens() {
    let scratch = Scratch::new("outside-profile");
    let profile = scratch.path().join("ana");
    let directory = scratch.path().join("shared").join("noct");
    fs::create_dir_all(&profile).expect("the profile is created");

    drop(bootstrap::open_with_profile(&directory, Some(&profile)).expect("the shell still opens"));

    let warnings = profile_warnings(&directory);
    assert_eq!(warnings.len(), 1, "one warning is written");
    assert_eq!(warnings[0]["level"], "warn");
    assert_eq!(warnings[0]["message"], bootstrap::OUTSIDE_PROFILE_MESSAGE);
    assert!(
        directory.join("nocturne.db").is_file(),
        "the database is created"
    );
    let log = fs::read_to_string(directory.join("nocturne.log")).expect("the log file");
    assert!(
        !log.contains(&*directory.to_string_lossy()),
        "the warning names no path"
    );
}

#[test]
fn a_directory_inside_the_profile_is_not_warned() {
    let scratch = Scratch::new("inside-profile");
    let profile = scratch.path().join("ana");
    let directory = profile.join("AppData").join("noct");

    drop(bootstrap::open_with_profile(&directory, Some(&profile)).expect("the shell opens"));

    assert!(profile_warnings(&directory).is_empty());
}

#[test]
fn without_a_known_profile_nothing_is_warned() {
    let scratch = Scratch::new("no-profile");
    let directory = scratch.path().join("noct");

    drop(bootstrap::open_with_profile(&directory, None).expect("the shell opens"));

    assert!(profile_warnings(&directory).is_empty());
}

#[test]
fn a_sibling_that_only_starts_with_the_profile_name_is_outside_it() {
    let scratch = Scratch::new("sibling");
    let profile = scratch.path().join("ana");
    let sibling = scratch.path().join("ana2");
    let inside = profile.join("noct");
    for folder in [&profile, &sibling, &inside] {
        fs::create_dir_all(folder).expect("the folder is created");
    }

    assert!(bootstrap::is_outside_profile(&sibling, &profile));
    assert!(!bootstrap::is_outside_profile(&profile, &profile));
    assert!(!bootstrap::is_outside_profile(&inside, &profile));
}
