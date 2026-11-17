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

/// A request from the page of `window`, so its origin is the one the access control list names for the
/// application on this platform: `tauri://localhost`, or `http://tauri.localhost` on Windows, where the webview
/// serves custom protocols over http. A fixed origin would be refused on Windows.
fn request(window: &WebviewWindow<MockRuntime>, command: &str, body: Value) -> InvokeRequest {
    InvokeRequest {
        cmd: command.to_string(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: window.url().expect("the window has the URL of its page"),
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
        .build(noct_app::context())
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

    let settings = ok_json(get_ipc_response(
        &main,
        request(&main, "settings_get", json!({})),
    ));
    assert_eq!(settings["approvalPolicy"], "on-request");
    assert_eq!(settings["theme"], "dark");
    assert_eq!(settings["language"], "pt-BR");

    let updated = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "settings_set",
            json!({ "input": { "theme": "light" } }),
        ),
    ));
    assert_eq!(updated["theme"], "light");

    assert_eq!(
        ok_json(get_ipc_response(
            &main,
            request(
                &main,
                "workspaces_list",
                json!({ "input": { "limit": 500 } })
            )
        )),
        json!({ "items": [], "hasMore": false, "nextCursor": null })
    );
    assert_eq!(
        ok_json(get_ipc_response(
            &main,
            request(
                &main,
                "conversations_list",
                json!({ "input": { "limit": 10 } })
            )
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
            request(&main, "conversations_list", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }
}

/// The project page is checked before the store is read, like the conversation page. The limit is required, the
/// offset is bounded, and an unknown field or a null offset is refused.
#[test]
fn workspaces_list_refuses_page_requests_outside_the_contract() {
    let scratch = Scratch::new("workspace-page");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");

    for input in [
        json!({ "input": {} }),
        json!({ "input": { "limit": 0 } }),
        json!({ "input": { "limit": 501 } }),
        json!({ "input": { "limit": "10" } }),
        json!({ "input": { "limit": 10, "offset": 0 } }),
        json!({ "input": { "limit": 10, "cursor": null } }),
        json!({ "input": { "limit": 10, "cursor": {} } }),
        json!({ "input": { "limit": 10, "cursor": { "favorite": false, "lastOpenedAt": "2026-01-02T00:00:00.000Z" } } }),
        json!({ "input": null }),
    ] {
        let error = error_of(get_ipc_response(
            &main,
            request(&main, "workspaces_list", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }
}

/// The favorite command is granted to the main window. It refuses a request outside the contract before the store
/// is read, and a project that is not saved with its own code; a saved project is marked, and the mark is stored.
#[test]
fn workspace_set_favorite_is_granted_and_checks_its_request() {
    let scratch = Scratch::new("favorite-acl");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");
    let saved = "/synthetic/project";
    app.state::<Persistence>()
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspaces()
        .touch(saved)
        .expect("the project is saved");

    assert_eq!(
        ok_json(get_ipc_response(
            &main,
            request(
                &main,
                "workspace_set_favorite",
                json!({ "input": { "workspace": saved, "favorite": true } }),
            ),
        )),
        Value::Null,
        "a saved project is marked and the command returns nothing"
    );
    let stored = app
        .state::<Persistence>()
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspaces()
        .list()
        .expect("the rows are read");
    assert!(
        stored.iter().any(|row| row.path == saved && row.favorite),
        "the mark is stored"
    );

    let unknown = error_of(get_ipc_response(
        &main,
        request(
            &main,
            "workspace_set_favorite",
            json!({ "input": { "workspace": "/synthetic/never-saved", "favorite": true } }),
        ),
    ));
    assert_eq!(unknown["code"], "unknown-workspace");
    assert_eq!(unknown["message"], "Pasta do projeto não encontrada.");

    for input in [
        json!({ "input": { "workspace": "", "favorite": true } }),
        json!({ "input": { "workspace": saved, "favorite": "yes" } }),
        json!({ "input": { "workspace": saved } }),
        json!({ "input": { "workspace": saved, "favorite": true, "theme": "dark" } }),
        json!({ "input": null }),
    ] {
        let error = error_of(get_ipc_response(
            &main,
            request(&main, "workspace_set_favorite", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }
}

/// The conversation command is granted to the main window. It takes the project path as a string, refuses any other
/// shape before the store is read, and refuses a project that is not saved or not available with its own code. Only
/// the accepted request writes a row.
#[test]
fn conversations_create_is_granted_and_checks_its_request() {
    let scratch = Scratch::new("conversation-create-acl");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let saved = project.to_string_lossy().into_owned();
    app.state::<Persistence>()
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspaces()
        .touch(&saved)
        .expect("the project is saved");
    let conversations = || -> i64 {
        app.state::<Persistence>()
            .store
            .lock()
            .expect("the store is not poisoned")
            .connection()
            .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
            .expect("the count is read")
    };

    let created = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "conversations_create",
            json!({ "input": saved.clone() }),
        ),
    ));
    assert_eq!(created["title"], "Nova conversa");
    assert_eq!(created["workspace"], saved.as_str());
    assert!(created["id"].as_str().is_some_and(|id| id.len() == 36));
    assert_eq!(conversations(), 1, "the accepted request writes one row");

    let unknown = error_of(get_ipc_response(
        &main,
        request(
            &main,
            "conversations_create",
            json!({ "input": "/synthetic/never-saved" }),
        ),
    ));
    assert_eq!(unknown["code"], "unknown-workspace");
    assert_eq!(unknown["message"], "Pasta do projeto não encontrada.");

    for input in [
        json!({ "input": "" }),
        json!({ "input": 1 }),
        json!({ "input": null }),
        json!({ "input": { "workspace": saved.clone() } }),
    ] {
        let error = error_of(get_ipc_response(
            &main,
            request(&main, "conversations_create", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }
    assert_eq!(conversations(), 1, "the refused requests write nothing");
}

/// The brain command is granted to the main window. It refuses a request outside the contract before the store is read,
/// with the code of the contract; a conversation that does not exist has its own code; a saved one takes a memory.
#[test]
fn brain_create_is_granted_and_checks_its_request() {
    let scratch = Scratch::new("brain-create-acl");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let saved = project.to_string_lossy().into_owned();
    let conversation = ok_json(get_ipc_response(&main, {
        app.state::<Persistence>()
            .store
            .lock()
            .expect("the store is not poisoned")
            .workspaces()
            .touch(&saved)
            .expect("the project is saved");
        request(
            &main,
            "conversations_create",
            json!({ "input": saved.clone() }),
        )
    }));
    let conversation_id = conversation["id"].as_str().expect("an id").to_string();

    let memory = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "brain_create",
            json!({ "input": {
                "conversationId": conversation_id.clone(),
                "kind": "constraint",
                "scope": "conversation",
                "content": "  Keep the review read-only  ",
            } }),
        ),
    ));
    assert_eq!(memory["content"], "Keep the review read-only");
    assert_eq!(memory["status"], "candidate");

    let unknown = error_of(get_ipc_response(
        &main,
        request(
            &main,
            "brain_create",
            json!({ "input": {
                "conversationId": "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d",
                "kind": "fact",
                "scope": "workspace",
                "content": "A note",
            } }),
        ),
    ));
    assert_eq!(unknown["code"], "unknown-conversation");
    assert_eq!(unknown["message"], "Conversa não encontrada.");

    for input in [
        json!({ "input": { "conversationId": conversation_id.clone(), "kind": "opinion", "scope": "workspace", "content": "A note" } }),
        json!({ "input": { "conversationId": conversation_id.clone(), "kind": "fact", "scope": "workspace", "content": "token=abcdefghijklmnop" } }),
        json!({ "input": { "conversationId": conversation_id.clone(), "kind": "fact", "scope": "workspace", "content": "A note", "theme": "dark" } }),
        json!({ "input": "not an object" }),
    ] {
        let error = error_of(get_ipc_response(
            &main,
            request(&main, "brain_create", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }
}

/// The brain update command is granted to the main window. A change that the lifecycle refuses, a memory that does not
/// exist and a request that changes nothing each have their own code or are invalid input; a valid change is stored.
#[test]
fn brain_update_is_granted_and_checks_its_request() {
    let scratch = Scratch::new("brain-update-acl");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let saved = project.to_string_lossy().into_owned();
    app.state::<Persistence>()
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspaces()
        .touch(&saved)
        .expect("the project is saved");
    let conversation = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "conversations_create",
            json!({ "input": saved.clone() }),
        ),
    ));
    let conversation_id = conversation["id"].as_str().expect("an id").to_string();
    let memory = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "brain_create",
            json!({ "input": {
                "conversationId": conversation_id.clone(),
                "kind": "fact",
                "scope": "workspace",
                "content": "A note",
            } }),
        ),
    ));
    let memory_id = memory["id"].as_str().expect("an id").to_string();

    let updated = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "brain_update",
            json!({ "input": {
                "conversationId": conversation_id.clone(),
                "memoryId": memory_id.clone(),
                "content": "  A changed note  ",
                "status": "active",
            } }),
        ),
    ));
    assert_eq!(updated["content"], "A changed note");
    assert_eq!(updated["status"], "active");

    let refused = error_of(get_ipc_response(
        &main,
        request(
            &main,
            "brain_update",
            json!({ "input": {
                "conversationId": conversation_id.clone(),
                "memoryId": memory_id.clone(),
                "status": "candidate",
            } }),
        ),
    ));
    assert_eq!(refused["code"], "invalid-transition");
    assert_eq!(refused["message"], "Transição de memória inválida.");

    let missing = error_of(get_ipc_response(
        &main,
        request(
            &main,
            "brain_update",
            json!({ "input": {
                "conversationId": conversation_id.clone(),
                "memoryId": "7a1d2c3e-4f50-4a61-8b72-9c83d4e5f607",
                "content": "A note",
            } }),
        ),
    ));
    assert_eq!(missing["code"], "unknown-memory");

    for input in [
        json!({ "input": { "conversationId": conversation_id.clone(), "memoryId": memory_id.clone() } }),
        json!({ "input": { "conversationId": conversation_id.clone(), "memoryId": memory_id.clone(), "confidence": 101 } }),
        json!({ "input": { "conversationId": conversation_id.clone(), "memoryId": memory_id.clone(), "theme": "dark" } }),
    ] {
        let error = error_of(get_ipc_response(
            &main,
            request(&main, "brain_update", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }
}

/// The extract command is granted to the main window. It reads a reply's block and stores the candidates; a conversation
/// that does not exist and a request outside the contract are refused with their codes.
#[test]
fn brain_extract_is_granted_and_checks_its_request() {
    let scratch = Scratch::new("brain-extract-acl");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let saved = project.to_string_lossy().into_owned();
    app.state::<Persistence>()
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspaces()
        .touch(&saved)
        .expect("the project is saved");
    let conversation = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "conversations_create",
            json!({ "input": saved.clone() }),
        ),
    ));
    let conversation_id = conversation["id"].as_str().expect("an id").to_string();

    let reply = "Texto.\n```nocturne-memories\n[{\"kind\":\"learning\",\"scope\":\"workspace\",\"content\":\"Run the row gate\"}]\n```";
    let extraction = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "brain_extract",
            json!({ "input": { "conversationId": conversation_id.clone(), "content": reply } }),
        ),
    ));
    assert_eq!(extraction["content"], "Texto.");
    assert_eq!(extraction["memories"][0]["content"], "Run the row gate");
    assert!(
        extraction.get("warning").is_none(),
        "no warning when the candidates are stored"
    );

    let unknown = error_of(get_ipc_response(
        &main,
        request(
            &main,
            "brain_extract",
            json!({ "input": { "conversationId": "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d", "content": "x" } }),
        ),
    ));
    assert_eq!(unknown["code"], "unknown-conversation");

    for input in [
        json!({ "input": { "conversationId": conversation_id.clone(), "content": 5 } }),
        json!({ "input": { "conversationId": conversation_id.clone(), "content": "x", "mode": "build" } }),
    ] {
        let error = error_of(get_ipc_response(
            &main,
            request(&main, "brain_extract", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }
}

/// The suggestions command is granted to the main window. It reads a review snapshot and reconciles it; a conversation
/// that does not exist and a request outside the contract are refused with their codes.
#[test]
fn suggestions_create_is_granted_and_checks_its_request() {
    let scratch = Scratch::new("suggestions-create-acl");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let saved = project.to_string_lossy().into_owned();
    app.state::<Persistence>()
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspaces()
        .touch(&saved)
        .expect("the project is saved");
    let conversation = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "conversations_create",
            json!({ "input": saved.clone() }),
        ),
    ));
    let conversation_id = conversation["id"].as_str().expect("an id").to_string();

    let reply = "Revisão.\n```nocturne-suggestions\n[{\"title\":\"Pin the toolchain\",\"description\":\"d\",\"reasoning\":\"r\",\"category\":\"dependency\",\"severity\":\"low\"}]\n```";
    let creation = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "suggestions_create",
            json!({ "input": { "conversationId": conversation_id.clone(), "content": reply } }),
        ),
    ));
    assert_eq!(creation["suggestions"][0]["title"], "Pin the toolchain");
    assert_eq!(creation["suggestions"][0]["status"], "new");
    assert_eq!(
        creation["comparison"]["newSuggestions"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert!(creation.get("warning").is_none());

    let unknown = error_of(get_ipc_response(
        &main,
        request(
            &main,
            "suggestions_create",
            json!({ "input": { "conversationId": "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d", "content": "x" } }),
        ),
    ));
    assert_eq!(unknown["code"], "unknown-conversation");

    for input in [
        json!({ "input": { "conversationId": conversation_id.clone(), "content": null } }),
        json!({ "input": { "conversationId": conversation_id.clone(), "content": "x", "mode": "review" } }),
    ] {
        let error = error_of(get_ipc_response(
            &main,
            request(&main, "suggestions_create", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }
}

/// The status command is granted to the main window. It moves a suggestion of a conversation; a status outside the list,
/// a missing field and a conversation that does not exist are refused with their codes.
#[test]
fn suggestions_status_is_granted_and_checks_its_request() {
    let scratch = Scratch::new("suggestions-status-acl");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let saved = project.to_string_lossy().into_owned();
    app.state::<Persistence>()
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspaces()
        .touch(&saved)
        .expect("the project is saved");
    let conversation = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "conversations_create",
            json!({ "input": saved.clone() }),
        ),
    ));
    let conversation_id = conversation["id"].as_str().expect("an id").to_string();
    let reply = "Revisão.\n```nocturne-suggestions\n[{\"title\":\"Pin the toolchain\",\"description\":\"d\",\"reasoning\":\"r\",\"category\":\"dependency\",\"severity\":\"low\"}]\n```";
    let creation = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "suggestions_create",
            json!({ "input": { "conversationId": conversation_id.clone(), "content": reply } }),
        ),
    ));
    let suggestion_id = creation["suggestions"][0]["id"]
        .as_str()
        .expect("an id")
        .to_string();

    let moved = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "suggestions_status",
            json!({ "input": {
                "conversationId": conversation_id.clone(),
                "suggestionId": suggestion_id.clone(),
                "status": "in-analysis",
                "result": "Em análise",
            } }),
        ),
    ));
    assert_eq!(moved["status"], "in-analysis");
    assert!(
        moved["history"]
            .as_array()
            .expect("the history is a list")
            .iter()
            .any(|entry| entry["status"] == "in-analysis" && entry["result"] == "Em análise"),
        "the decision keeps its result in the history: {moved}"
    );

    let unknown = error_of(get_ipc_response(
        &main,
        request(
            &main,
            "suggestions_status",
            json!({ "input": {
                "conversationId": "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d",
                "suggestionId": suggestion_id.clone(),
                "status": "accepted",
            } }),
        ),
    ));
    assert_eq!(unknown["code"], "unknown-conversation");

    for input in [
        json!({ "input": { "conversationId": conversation_id.clone(), "suggestionId": suggestion_id.clone(), "status": "done" } }),
        json!({ "input": { "conversationId": conversation_id.clone(), "suggestionId": suggestion_id.clone() } }),
    ] {
        let error = error_of(get_ipc_response(
            &main,
            request(&main, "suggestions_status", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }
}

/// The bindings command is granted to the main window. It stores the bindings of a saved project and refuses an unknown
/// field, an explicit null and a project that is not saved.
#[test]
fn models_set_bindings_is_granted_and_checks_its_request() {
    let scratch = Scratch::new("models-set-bindings-acl");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let saved = project.to_string_lossy().into_owned();
    app.state::<Persistence>()
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspaces()
        .touch(&saved)
        .expect("the project is saved");

    let stored = ok_json(get_ipc_response(
        &main,
        request(
            &main,
            "models_set_bindings",
            json!({ "input": {
                "workspaceId": saved.clone(),
                "defaultBinding": { "providerId": "openai", "modelId": "gpt-model" },
                "remoteEmbeddingAllowed": false,
            } }),
        ),
    ));
    assert_eq!(stored["workspaceId"], saved);
    assert_eq!(stored["defaultBinding"]["modelId"], "gpt-model");
    assert!(
        stored.get("embeddingBinding").is_none(),
        "an absent binding is not written as null"
    );

    for input in [
        json!({ "input": { "workspaceId": saved.clone(), "priority": 1 } }),
        json!({ "input": { "workspaceId": saved.clone(), "embeddingBinding": null } }),
        json!({ "input": { "workspaceId": saved.clone(), "defaultBinding": { "providerId": "openai", "modelId": "m", "extra": 1 } } }),
    ] {
        let error = error_of(get_ipc_response(
            &main,
            request(&main, "models_set_bindings", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }

    let unsaved = error_of(get_ipc_response(
        &main,
        request(
            &main,
            "models_set_bindings",
            json!({ "input": { "workspaceId": scratch.path().join("elsewhere").to_string_lossy() } }),
        ),
    ));
    assert_eq!(unsaved["code"], "unknown-workspace");
}

#[test]
fn settings_set_changes_the_diagnostic_gate_the_shell_holds() {
    let scratch = Scratch::new("gate");
    let app = app_with_commands(&scratch);
    let main = window(&app, "main");

    let _ = get_ipc_response(
        &main,
        request(
            &main,
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
            request(&main, "settings_set", input.clone()),
        ));
        assert_eq!(error["code"], "invalid-input", "{input} is refused");
    }

    let untouched = ok_json(get_ipc_response(
        &main,
        request(&main, "settings_get", json!({})),
    ));
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

    assert!(get_ipc_response(&main, request(&main, "settings_set", json!({}))).is_err());
}

#[test]
fn a_command_that_no_capability_grants_is_refused() {
    let scratch = Scratch::new("unlisted");
    let persistence = bootstrap::open(scratch.path()).expect("the shell opens");
    let app = mock_builder()
        .manage(persistence)
        .invoke_handler(tauri::generate_handler![probe])
        .build(noct_app::context())
        .expect("the mock application builds");
    let main = window(&app, "main");

    let response = get_ipc_response(&main, request(&main, "probe", json!({})));

    assert_refused_by_the_acl(response);
}

#[test]
fn a_window_outside_the_capability_is_refused_even_for_a_granted_command() {
    let scratch = Scratch::new("other-window");
    let app = app_with_commands(&scratch);
    let other = window(&app, "secondary");

    let response = get_ipc_response(&other, request(&other, "settings_get", json!({})));

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
