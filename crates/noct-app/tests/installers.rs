//! Locks what the installers depend on: the bundle settings of `tauri.conf.json` and the `build-installers` workflow.
//!
//! The configuration keeps `bundle.active` false, so an ordinary build makes no installer; the workflow turns it on for
//! its own build. The identifier is the one the data folder and the credential store already use, so an installed copy
//! finds the data of an earlier one. The workflow runs only by hand, reads no secret and gets a read-only token.

use std::fs;
use std::path::Path;

use serde_json::Value;

const CONFIG: &str = include_str!("../tauri.conf.json");
const WORKFLOW_FILE: &str = include_str!("../../../.github/workflows/build-installers.yml");

/// The workflow with LF line ends: a Windows checkout can turn them into CRLF, and the checks below match on `\n`.
fn workflow() -> String {
    WORKFLOW_FILE.replace("\r\n", "\n")
}

fn config() -> Value {
    serde_json::from_str(CONFIG).expect("tauri.conf.json is JSON")
}

#[test]
fn the_bundle_names_the_targets_and_icons_of_the_three_systems() {
    let config = config();
    let bundle = &config["bundle"];
    assert_eq!(
        bundle["active"], false,
        "only the installer builds turn the bundle on"
    );
    let targets: Vec<&str> = bundle["targets"]
        .as_array()
        .expect("bundle.targets is a list")
        .iter()
        .map(|target| target.as_str().expect("a target is a string"))
        .collect();
    assert_eq!(targets, ["deb", "appimage", "nsis", "dmg"]);
    let icons: Vec<&str> = bundle["icon"]
        .as_array()
        .expect("bundle.icon is a list")
        .iter()
        .map(|icon| icon.as_str().expect("an icon is a path"))
        .collect();
    assert!(
        icons.iter().any(|icon| icon.ends_with(".png")),
        "Linux and macOS take the PNG"
    );
    assert!(
        icons.iter().any(|icon| icon.ends_with(".ico")),
        "Windows takes the ICO"
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for icon in icons {
        assert!(
            fs::metadata(root.join(icon)).is_ok_and(|file| file.len() > 0),
            "{icon} exists"
        );
    }
}

#[test]
fn the_installers_hold_the_release_interface_and_keep_the_data_folder() {
    let config = config();
    assert_eq!(config["build"]["frontendDist"], "../noct-ui/dist");
    assert_eq!(
        config["build"]["beforeBuildCommand"],
        "cd noct-ui && trunk build --release"
    );
    assert_eq!(
        config["identifier"], "com.nocturne.codex",
        "the data folder and the credential store are named after the identifier"
    );
    assert!(
        noct_secrets::SERVICE_NAME.starts_with("com.nocturne.codex."),
        "the credential store's service follows the identifier"
    );
}

#[test]
fn the_installer_workflow_runs_by_hand_reads_no_secret_and_cannot_write() {
    let workflow = workflow();
    let on = workflow
        .split("\non:\n")
        .nth(1)
        .and_then(|rest| rest.split("\npermissions:").next())
        .expect("the workflow has an `on` block before its permissions");
    assert_eq!(on.trim(), "workflow_dispatch:", "it starts only by hand");
    assert!(!workflow.contains("secrets."), "no secret is read");
    assert!(
        !workflow.contains("environment:"),
        "no environment, and so no environment secret"
    );
    for line in workflow.lines().map(str::trim) {
        assert!(!line.ends_with(": write"), "no permission to write: {line}");
        assert!(!line.contains("TAURI_SIGNING"), "no signing key: {line}");
        if let Some(action) = line
            .strip_prefix("uses: ")
            .or_else(|| line.strip_prefix("- uses: "))
        {
            let pin = action.split('@').nth(1).unwrap_or_default();
            assert!(
                pin.len() >= 40 && pin[..40].chars().all(|c| c.is_ascii_hexdigit()),
                "each action is pinned by commit: {line}"
            );
        }
    }
    assert!(workflow.contains("permissions:\n  contents: read\n"));
    assert!(workflow.contains("persist-credentials: false"));
    assert!(workflow.contains("--no-sign"));
    assert!(
        !workflow.contains("gh release")
            && !workflow.contains("git push")
            && !workflow.contains("git tag")
    );
    for installer in [
        "deb,appimage",
        "nsis",
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "SHA256SUMS",
    ] {
        assert!(workflow.contains(installer), "{installer}");
    }
}
