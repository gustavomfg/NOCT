//! The IPC contracts of the first slice, checked on the Rust side against the samples written
//! from the TypeScript side at the tag `electron-final`. The settings inputs are the ones
//! zod accepts and refuses; the outputs are the ones the TypeScript types accept and refuse.

use noct_core::contracts::Conversation;
use noct_core::ipc::commands;
use noct_core::ipc::settings::{AppSettings, SettingsUpdate};
use noct_core::ipc::workspaces::WorkspacePage;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

const SETTINGS_SET: &str = include_str!("fixtures/ipc/settings-set.json");
const SETTINGS_GET: &str = include_str!("fixtures/ipc/settings-get.json");
const WORKSPACES_LIST: &str = include_str!("fixtures/ipc/workspaces-list.json");
const CONVERSATIONS_LIST: &str = include_str!("fixtures/ipc/conversations-list.json");
const TYPESCRIPT_CHANNELS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/ts-reference/channels.ts.txt"
);

fn fixture(text: &str) -> Value {
    serde_json::from_str(text).expect("the sample file is JSON")
}

fn samples(fixture: &Value, key: &str) -> Vec<Value> {
    let samples = fixture[key]
        .as_array()
        .expect("an array of samples")
        .clone();
    assert!(!samples.is_empty(), "the samples cover {key}");
    samples
}

/// A sample the Rust type accepts must come back unchanged: the same keys and the same values.
fn accepts<T: DeserializeOwned + Serialize>(sample: &Value) -> T {
    let value: T = serde_json::from_value(sample.clone())
        .unwrap_or_else(|error| panic!("refused {sample}: {error}"));
    assert_eq!(&serde_json::to_value(&value).expect("serializes"), sample);
    value
}

fn refuses<T: DeserializeOwned>(sample: &Value) {
    assert!(
        serde_json::from_value::<T>(sample.clone()).is_err(),
        "accepted {sample}"
    );
}

#[test]
fn settings_set_accepts_what_zod_accepts_and_gives_the_output_zod_gives() {
    for sample in samples(&fixture(SETTINGS_SET), "valid") {
        let update: SettingsUpdate = serde_json::from_value(sample["input"].clone())
            .unwrap_or_else(|error| panic!("refused {}: {error}", sample["input"]));
        assert_eq!(
            serde_json::to_value(&update).expect("serializes"),
            sample["output"],
            "input {}",
            sample["input"]
        );
    }
}

#[test]
fn settings_set_refuses_what_zod_refuses() {
    for sample in samples(&fixture(SETTINGS_SET), "invalid") {
        refuses::<SettingsUpdate>(&sample["input"]);
    }
}

#[test]
fn settings_get_outputs_are_the_app_settings_the_typescript_type_accepts() {
    let fixture = fixture(SETTINGS_GET);
    for sample in samples(&fixture, "valid") {
        accepts::<AppSettings>(&sample);
    }
    for sample in samples(&fixture, "invalid") {
        refuses::<AppSettings>(&sample);
    }
}

#[test]
fn workspaces_list_outputs_are_the_pages_the_typescript_types_accept() {
    let fixture = fixture(WORKSPACES_LIST);
    for sample in samples(&fixture, "valid") {
        accepts::<WorkspacePage>(&sample);
    }
    for sample in samples(&fixture, "invalid") {
        refuses::<WorkspacePage>(&sample);
    }
}

#[test]
fn conversations_list_outputs_are_the_conversations_the_typescript_type_accepts() {
    let fixture = fixture(CONVERSATIONS_LIST);
    for sample in samples(&fixture, "valid") {
        accepts::<Vec<Conversation>>(&sample);
    }
    for sample in samples(&fixture, "invalid") {
        refuses::<Vec<Conversation>>(&sample);
    }
}

/// Each command replaces a channel of `ts-reference/channels.ts.txt`. The channel names are compared with the
/// TypeScript file, so a renamed channel fails here.
#[test]
fn the_command_names_replace_the_typescript_channels() {
    let channels =
        std::fs::read_to_string(TYPESCRIPT_CHANNELS).expect("ts-reference/channels.ts.txt");
    for (command, channel) in [
        (commands::SETTINGS_GET, "settings:get"),
        (commands::SETTINGS_SET, "settings:set"),
        (commands::WORKSPACES_LIST, "workspaces:list"),
        (commands::CONVERSATIONS_LIST, "conversations:list"),
    ] {
        assert!(
            channels.contains(&format!("'{channel}'")),
            "{channel} is not a channel of ts-reference/channels.ts.txt"
        );
        assert!(
            !command.contains(':'),
            "{command} must be a Tauri command name"
        );
    }
}
