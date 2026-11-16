//! The commands through Tauri's mock runtime, with the capabilities of the real application. These tests
//! check what the window may call, not what the commands return; `services.rs` checks the results.

mod common;

use common::Scratch;
use noct_app::bootstrap::{self, Persistence};
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// A command that exists in the application but is not in the first slice, so no capability grants it.
#[tauri::command]
fn probe() -> &'static str {
    "probe ran"
}

fn request(command: &str, body: Value) -> InvokeRequest {
    InvokeRequest {
        cmd: command.to_string(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().expect("the URL is valid"),
        body: InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}

fn app_with_commands(scratch: &Scratch) -> App<MockRuntime> {
    let persistence: Persistence =
        bootstrap::open(scratch.path()).expect("the shell opens in its scratch directory");
    mock_builder()
        .manage(persistence)
        .invoke_handler(noct_app::command_handler())
        .build(tauri::generate_context!())
        .expect("the mock application builds")
}

fn window(app: &App<MockRuntime>, label: &str) -> WebviewWindow<MockRuntime> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .build()
        .expect("the window is created")
}

fn ok_json(response: Result<InvokeResponseBody, Value>) -> Value {
    response
        .expect("the command is allowed")
        .deserialize::<Value>()
        .expect("the body is JSON")
}

fn error_of(response: Result<InvokeResponseBody, Value>) -> Value {
    response.expect_err("the command is refused")
}

#[test]
fn the_main_window_runs_the_granted_commands() {
    let scratch = Scratch::new("granted");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");

    let settings = ok_json(get_ipc_response(&main, request("settings_get", json!({}))));
    assert_eq!(settings["approvalPolicy"], "on-request");
    assert_eq!(settings["theme"], "dark");
    assert_eq!(settings["language"], "pt-BR");

    let updated = ok_json(get_ipc_response(
        &main,
        request("settings_set", json!({ "input": { "theme": "light" } })),
    ));
    assert_eq!(updated["theme"], "light");

    assert_eq!(
        ok_json(get_ipc_response(
            &main,
            request("workspaces_list", json!({}))
        )),
        json!([])
    );
    assert_eq!(
        ok_json(get_ipc_response(
            &main,
            request("conversations_list", json!({ "input": { "limit": 10 } }))
        )),
        json!({ "items": [], "hasMore": false, "nextCursor": null })
    );
}

/// A page request is checked before the store is read. A missing, out-of-range or non-integer limit, an
/// unknown field, an explicit null cursor, a cursor without its key, and a missing input are refused as
/// invalid input.
#[test]
fn conversations_list_refuses_page_requests_outside_the_contract() {
    let scratch = Scratch::new("conversation-page");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");

    for input in [
        json!({ "input": {} }),
        json!({ "input": { "limit": 0 } }),
        json!({ "input": { "limit": 501 } }),
        json!({ "input": { "limit": "10" } }),
        json!({ "input": { "limit": 10, "offset": 5 } }),
        json!({ "input": { "limit": 10, "cursor": null } }),
        json!({ "input": { "limit": 10, "cursor": { "updatedAt": "2026-01-01T00:00:00.000Z" } } }),
        json!({ "input": null }),
    ] {
        let error = error_of(get_ipc_response(
            &main,
            request("conversations_list", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }
}

#[test]
fn settings_set_changes_the_diagnostic_gate_the_shell_holds() {
    let scratch = Scratch::new("gate");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");

    let _ = get_ipc_response(
        &main,
        request(
            "settings_set",
            json!({ "input": { "diagnosticMode": true } }),
        ),
    );

    assert!(app.state::<Persistence>().diagnostics.is_enabled());
}

/// The arguments are checked with the contract before anything is written: an unknown key, a value the
/// contract does not allow, a list, or an explicit null are all refused as invalid input.
#[test]
fn settings_set_refuses_arguments_outside_the_contract() {
    let scratch = Scratch::new("invalid");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");

    for input in [
        json!({ "input": { "theme": "blue" } }),
        json!({ "input": { "unknown": 1 } }),
        json!({ "input": [] }),
        json!({ "input": null }),
    ] {
        let error = error_of(get_ipc_response(
            &main,
            request("settings_set", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }

    let untouched = ok_json(get_ipc_response(&main, request("settings_get", json!({}))));
    assert_eq!(
        untouched["theme"], "dark",
        "a refused update writes nothing"
    );
}

#[test]
fn a_missing_argument_is_refused() {
    let scratch = Scratch::new("missing-argument");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");

    assert!(get_ipc_response(&main, request("settings_set", json!({}))).is_err());
}

#[test]
fn a_command_that_no_capability_grants_is_refused() {
    let scratch = Scratch::new("unlisted");
    let persistence = bootstrap::open(scratch.path()).expect("the shell opens");
    let app = mock_builder()
        .manage(persistence)
        .invoke_handler(tauri::generate_handler![probe])
        .build(tauri::generate_context!())
        .expect("the mock application builds");
    let main = window(&app, "main");

    let response = get_ipc_response(&main, request("probe", json!({})));

    assert_refused_by_the_acl(response);
}

#[test]
fn a_window_outside_the_capability_is_refused_even_for_a_granted_command() {
    let scratch = Scratch::new("other-window");
    let app = app_with_commands(&scratch);
    let other = window(&app, "secondary");

    let response = get_ipc_response(&other, request("settings_get", json!({})));

    assert_refused_by_the_acl(response);
}

/// The refusal must come from the access control list, not from a failure of the command itself. Tauri
/// words it as "not allowed" for a command the window may not call, and as "not found" for a command the
/// application does not know.
fn assert_refused_by_the_acl(response: Result<InvokeResponseBody, Value>) {
    let text = response.expect_err("the command is refused").to_string();
    assert!(
        text.contains("not allowed") || text.contains("not found"),
        "refused by the access control list, got {text}"
    );
}
