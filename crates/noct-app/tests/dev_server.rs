//! The dev server of the shell. The dev URL, the command that the Tauri CLI runs before a dev build, and
//! the port that Trunk serves must name the same port. A mismatch leaves the debug window with nothing to
//! load, which is what happened when the dev URL and the port of the run instructions disagreed.

use serde_json::Value;

const TAURI_CONFIG: &str = include_str!("../tauri.conf.json");
const TRUNK_CONFIG: &str = include_str!("../../noct-ui/Trunk.toml");

#[test]
fn the_dev_url_and_the_dev_command_name_the_port_trunk_serves() {
    let config: Value = serde_json::from_str(TAURI_CONFIG).expect("tauri.conf.json is JSON");

    assert_eq!(config["build"]["devUrl"], "http://localhost:1420");
    let before_dev = config["build"]["beforeDevCommand"]
        .as_str()
        .expect("beforeDevCommand is a string");
    assert!(
        before_dev.ends_with("trunk serve --port 1420"),
        "beforeDevCommand serves another port: {before_dev}"
    );
}

#[test]
fn trunk_serves_the_same_port_when_started_without_the_flag() {
    assert!(
        TRUNK_CONFIG
            .lines()
            .any(|line| line.trim() == "port = 1420"),
        "Trunk.toml does not fix the dev port to 1420"
    );
}
