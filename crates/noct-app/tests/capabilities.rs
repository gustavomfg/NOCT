//! Locks the capability of the main window. A change to the granted commands must change this test too,
//! so that a new command is never granted by accident.

use noct_app::command_names;
use serde_json::Value;

const MAIN_CAPABILITY: &str = include_str!("../capabilities/default.json");

fn permission_of(command: &str) -> String {
    format!("allow-{}", command.replace('_', "-"))
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

#[test]
fn the_main_window_capability_grants_exactly_the_first_slice() {
    let capability: Value = serde_json::from_str(MAIN_CAPABILITY).expect("the capability is JSON");

    assert_eq!(capability["identifier"], "main-window");
    assert_eq!(capability["windows"], serde_json::json!(["main"]));

    let granted: Vec<String> = capability["permissions"]
        .as_array()
        .expect("the permissions are a list")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("a permission is a string")
                .to_string()
        })
        .collect();
    let mut expected = vec!["core:default".to_string()];
    expected.extend(
        command_names::ALL
            .iter()
            .map(|command| permission_of(command)),
    );

    assert_eq!(sorted(granted), sorted(expected));
}

#[test]
fn the_first_slice_is_the_commands_of_the_contract() {
    assert_eq!(
        command_names::ALL,
        &[
            "settings_get",
            "settings_set",
            "workspaces_list",
            "brain_create",
            "brain_extract",
            "brain_update",
            "suggestions_create",
            "suggestions_status",
            "models_set_bindings",
            "ai_save_assistant",
            "conversations_delete",
            "workspace_remove",
            "brain_delete",
            "artifacts_delete",
            "conversations_create",
            "conversations_list",
            "workspace_set_favorite"
        ]
    );
}

#[test]
fn only_the_main_window_capability_exists() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
    let mut files: Vec<String> = std::fs::read_dir(directory)
        .expect("the capability folder exists")
        .map(|entry| {
            entry
                .expect("an entry is readable")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    files.sort();

    assert_eq!(files, ["default.json"]);
}
