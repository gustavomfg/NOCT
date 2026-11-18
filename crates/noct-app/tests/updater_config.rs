//! Locks the updater configuration of the shell (`plugins.updater` in `tauri.conf.json`). The configuration is
//! complete, so `updater.rs` registers the plugin at start. Registration makes no request, and nothing calls the
//! plugin's check, download or install commands: the window is granted no updater permission, which
//! `capabilities.rs` locks. The tests keep the values a release depends on from changing silently: the key the
//! installed application trusts, the one HTTPS endpoint, the version binding, and the absence of any option that
//! could weaken verification. They also keep the plugin registered in one place only.
//!
//! `requireSignedVersion` is a field of the plugin's configuration in tauri-plugin-updater 2.12.0 and 2.14.1, the
//! pinned version. docs/shell-updater.md records the rest.

use std::fs;
use std::path::Path;

use serde_json::Value;

const CONFIG: &str = include_str!("../tauri.conf.json");

/// The public key of the release signing key, as Tauri stores it: the base64 of the minisign `.pub` file. Its
/// decoded first line is `untrusted comment: minisign public key: 40503B624D87D646`. Rotation is described in
/// docs/shell-updater.md.
const UPDATER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQwNTAzQjYyNEQ4N0Q2NDYKUldSRzFvZE5ZanRRUU54c3A1UHRkUG1yNE5nL3Q5cm9IMHJONjBIVVRZRGZDNVZXMVNaRFJxQncK";

const RELEASE_ENDPOINT: &str =
    "https://github.com/gustavomfg/NOCT/releases/latest/download/latest.json";

fn config() -> Value {
    serde_json::from_str(CONFIG).expect("tauri.conf.json is JSON")
}

fn updater(config: &Value) -> &serde_json::Map<String, Value> {
    config["plugins"]["updater"]
        .as_object()
        .expect("plugins.updater is an object")
}

#[test]
fn the_updater_trusts_only_the_release_key() {
    let config = config();

    assert_eq!(updater(&config)["pubkey"], UPDATER_PUBKEY);
}

/// The public key file committed to the repository is the configured key. A rotation changes both, and this test fails
/// when only one of them changes.
#[test]
fn the_committed_public_key_file_is_the_configured_key() {
    let file = include_str!("../../../docs/noct-updater.key.pub");

    assert_eq!(file.trim_end(), UPDATER_PUBKEY);
}

#[test]
fn the_updater_reads_one_https_endpoint_of_the_release() {
    let config = config();

    assert_eq!(
        updater(&config)["endpoints"],
        serde_json::json!([RELEASE_ENDPOINT])
    );
    assert!(RELEASE_ENDPOINT.starts_with("https://"));
}

/// The updater accepts an update only when its signature names the version the manifest announces. Without this
/// flag a crafted manifest could pair a newer version number with an older, validly signed package.
#[test]
fn the_updater_binds_the_signed_version() {
    let config = config();

    assert_eq!(updater(&config)["requireSignedVersion"], true);
}

/// The only keys are the key, the endpoint and the version binding. Any other key under `plugins.updater` is a
/// new option, such as `dangerousInsecureTransportProtocol` (a non-HTTPS endpoint), `allowDowngrades` or a
/// `windows` install mode, and must be added here on purpose, with its reason.
#[test]
fn the_updater_has_no_other_option() {
    let config = config();
    let mut keys: Vec<&str> = updater(&config).keys().map(String::as_str).collect();
    keys.sort_unstable();

    assert_eq!(keys, ["endpoints", "pubkey", "requireSignedVersion"]);
}

/// The plugin is a dependency of the shell, pinned in `Cargo.toml`. Changing where it is registered or used changes what
/// the shell does at start-up, so that change must come with its own review and tests, and this one fails first.
///
/// The plugin is registered in one place, `src/updater.rs`, which registers it only when the configuration is
/// complete and safe. No other source names the plugin or calls its commands. A second registration, or a call to
/// its check, download or install, fails here and needs a review.
#[test]
fn the_plugin_is_registered_and_called_only_by_the_updater_module() {
    let sources = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in fs::read_dir(&sources).expect("the sources are readable") {
        let path = entry.expect("a source entry is readable").path();
        let is_updater = path.file_name().is_some_and(|name| name == "updater.rs");
        if is_updater || path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("a source file is readable");
        for name in [
            "tauri_plugin_updater",
            "plugin:updater",
            ".updater()",
            "updater_builder",
        ] {
            assert!(
                !text.contains(name),
                "{} uses the updater plugin ({name}); only src/updater.rs may",
                path.display()
            );
        }
    }
}
