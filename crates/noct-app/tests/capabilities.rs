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
            "models_bindings_get",
            "models_set_default_model",
            "ai_save_assistant",
            "conversations_delete",
            "workspace_remove",
            "brain_delete",
            "artifacts_delete",
            "conversations_create",
            "conversations_list",
            "workspace_set_favorite",
            "provider_key_status",
            "provider_key_save",
            "provider_key_remove",
            "provider_consent_status",
            "provider_consent_grant",
            "provider_consent_revoke",
            "provider_chat_status",
            "provider_chat_send",
            "provider_chat_cancel",
            "file_changes_list",
            "file_change_propose",
            "file_change_approve",
            "file_change_reject",
            "file_change_undo"
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

/// The updater's commands are not reachable from the window: no permission of the updater plugin is granted.
#[test]
fn the_window_is_granted_no_updater_permission() {
    let capability: Value = serde_json::from_str(MAIN_CAPABILITY).expect("the capability is JSON");
    let granted = capability["permissions"]
        .as_array()
        .expect("the permissions are a list");

    assert!(
        granted.iter().all(|permission| !permission
            .as_str()
            .unwrap_or_default()
            .starts_with("updater")),
        "{granted:?}"
    );
}
