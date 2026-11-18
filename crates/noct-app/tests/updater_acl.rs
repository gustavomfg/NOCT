//! The updater plugin, registered the way the shell registers it, and the main window's access to its commands. The
//! window's capability grants no updater permission, so the access control list refuses check, download and install
//! before any command runs. No request reaches the update feed from the window.

use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// The commands of the updater plugin that check for, download or install an update, each with the permission the
/// main window would need to call it. The window's capability holds none of them.
const UPDATER_COMMANDS: [(&str, &str); 4] = [
    ("plugin:updater|check", "updater:allow-check"),
    ("plugin:updater|download", "updater:allow-download"),
    ("plugin:updater|install", "updater:allow-install"),
    (
        "plugin:updater|download_and_install",
        "updater:allow-download-and-install",
    ),
];

/// The application with the updater registered from the shipped configuration, as `noct_app::run` does it.
fn app_with_updater() -> App<MockRuntime> {
    let context = noct_app::context();
    let builder = noct_app::updater::with_plugin(mock_builder(), context.config())
        .expect("the shipped updater configuration is safe");
    builder.build(context).expect("the mock application builds")
}

fn window(app: &App<MockRuntime>, label: &str) -> WebviewWindow<MockRuntime> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .build()
        .expect("the window is created")
}

/// A request from the page of `window`, with the origin the access control list names for the application here.
fn request(window: &WebviewWindow<MockRuntime>, command: &str) -> InvokeRequest {
    InvokeRequest {
        cmd: command.to_string(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: window.url().expect("the window has the URL of its page"),
        body: InvokeBody::Json(json!({})),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}

/// The refusal as text, whatever shape Tauri gives it.
fn refusal_text(error: &Value) -> String {
    error
        .as_str()
        .map_or_else(|| error.to_string(), str::to_string)
}

/// With the plugin registered, the main window cannot call check, download, install or download_and_install. Each
/// refusal is the access control list's: it says "not allowed" and names the permission the window lacks. A missing
/// command would give neither. The refusal happens before the plugin runs, so no request reaches the feed.
#[test]
fn the_main_window_cannot_call_the_updater_commands_with_the_plugin_registered() {
    let app = app_with_updater();
    let main = window(&app, "main");

    for (command, permission) in UPDATER_COMMANDS {
        let error = get_ipc_response(&main, request(&main, command))
            .expect_err("the access control list refuses the updater command");
        let text = refusal_text(&error);
        assert!(
            text.contains("not allowed"),
            "{command} is refused as not allowed: {text}"
        );
        assert!(
            text.contains(&format!(
                "Permissions associated with this command: {permission}"
            )),
            "{command} names the permission the window lacks, {permission}: {text}"
        );
    }
}

#[test]
fn the_access_control_list_still_grants_the_main_window_what_its_capability_names() {
    // A control: the same window reaches a command its capability does grant, so the refusals above are about the
    // updater, not about every plugin command.
    let app = app_with_updater();
    let main = window(&app, "main");

    let version = get_ipc_response(&main, request(&main, "plugin:app|version"));
    assert!(
        version.is_ok(),
        "the window may read the application version: {version:?}"
    );
}
