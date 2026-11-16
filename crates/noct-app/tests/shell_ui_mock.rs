//! The browser check of the settings area (`scripts/check-shell-ui.mjs`) answers the shell's `invoke` with a
//! mock. The mock follows one version of Tauri, and it names that version in `MOCKED_TAURI_VERSION`. This test
//! fails when Cargo.lock names another version of Tauri, so a Tauri update is not taken until the mock has
//! been checked against the new `invoke`.

const CARGO_LOCK: &str = include_str!("../../../Cargo.lock");
const MOCK: &str = include_str!("../../../scripts/check-shell-ui.mjs");

/// The versions of the `tauri` package in Cargo.lock: the `version` line that follows each `name = "tauri"`.
fn locked_tauri_versions() -> Vec<String> {
    let lines: Vec<&str> = CARGO_LOCK.lines().collect();
    lines
        .windows(2)
        .filter(|pair| pair[0] == "name = \"tauri\"")
        .filter_map(|pair| {
            pair[1]
                .strip_prefix("version = \"")
                .and_then(|rest| rest.strip_suffix('"'))
                .map(str::to_string)
        })
        .collect()
}

/// The version the mock declares, from `const MOCKED_TAURI_VERSION = '...'`.
fn mocked_tauri_version() -> String {
    MOCK.lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("const MOCKED_TAURI_VERSION = '")
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .expect("the mock declares MOCKED_TAURI_VERSION")
        .to_string()
}

#[test]
fn the_mock_follows_the_tauri_version_in_cargo_lock() {
    let locked = locked_tauri_versions();

    assert_eq!(
        locked.len(),
        1,
        "Cargo.lock must name exactly one tauri package, found {locked:?}"
    );
    assert_eq!(
        mocked_tauri_version(),
        locked[0],
        "Cargo.lock names tauri {}, but scripts/check-shell-ui.mjs mocks another version. Check the mock's \
         invoke against the new tauri (scripts/core.js) and then update MOCKED_TAURI_VERSION",
        locked[0]
    );
}
