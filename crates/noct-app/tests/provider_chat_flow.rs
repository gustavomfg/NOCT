//! The provider chat: its checks in order, the project context it sends, the streamed answer, and the cancel. The calls run
//! against the fake provider on loopback through the real policy and transport. The switch is passed as an argument, so the
//! tests reach the checks after it without changing the constant that ships.

mod common;

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use common::Scratch;
use common::fake_provider::{
    FakeServer, KEY, Resolve, events, head, limits, local_approval, local_url, serve,
};
use noct_app::bootstrap::{self, Persistence};
use noct_app::credentials::OPENROUTER_REFERENCE;
use noct_app::error::CommandError;
use noct_app::provider_chat::{
    ChatControl, ChatEvent, ChatRequest, ChatRun, PROVIDER_CHAT_DEFAULT, body, is_enabled, prepare,
    project_context, run_chat, set_enabled, status,
};
use noct_app::{consent, services};
use noct_core::ai::ProviderSource;
use noct_core::ai::bindings::WorkspaceModelBindings;
use noct_core::ai::consent::ProjectConsentRequest;
use noct_core::ai::model::ModelReference;
use noct_core::ai::network::{Approval, EndpointError};
use noct_core::ai::openrouter::ChatFailure;
use noct_secrets::SecretString;

const MODEL: &str = "openai/gpt-4o:free";
const KEY_REFERENCE: &str = "00000000-0000-4000-8000-000000000001";

// The switch ships on, after the owner review with a real key. Ajustes can turn it off.
const _: () = assert!(PROVIDER_CHAT_DEFAULT, "the chat ships switched on");

fn fixture(name: &str) -> (Scratch, Persistence) {
    let scratch = Scratch::new(name);
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    (scratch, persistence)
}

/// A real folder in the scratch directory, saved as a project.
fn project(scratch: &Scratch, persistence: &Persistence, name: &str) -> String {
    let folder = scratch.path().join(name);
    std::fs::create_dir(&folder).expect("the folder is created");
    let path = folder.to_string_lossy().into_owned();
    persistence
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspaces()
        .touch(&path)
        .expect("the project is saved");
    path
}

fn grant(persistence: &Persistence, workspace: &str) {
    consent::grant(
        &persistence.store,
        ProjectConsentRequest {
            workspace: workspace.to_string(),
        },
    )
    .expect("the consent is granted");
}

fn set_model(persistence: &Persistence, workspace: &str, model: &str) {
    persistence
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspace_model_bindings()
        .set_default_model(
            workspace,
            Some(ModelReference {
                provider_id: "openrouter".to_string(),
                model_id: model.to_string(),
            }),
        )
        .expect("the default model is saved");
}

fn save_key_reference(persistence: &Persistence) {
    let values = BTreeMap::from([(OPENROUTER_REFERENCE.to_string(), KEY_REFERENCE.to_string())]);
    persistence
        .store
        .lock()
        .expect("the store is not poisoned")
        .settings()
        .set(&values)
        .expect("the reference is saved");
}

fn request(workspace: &str, question: &str) -> ChatRequest {
    ChatRequest {
        workspace: workspace.to_string(),
        question: question.to_string(),
    }
}

/// Runs the checks with the switch passed in, against the store of the fixture.
fn check(
    persistence: &Persistence,
    workspace: &str,
    question: &str,
    enabled: bool,
) -> Result<noct_app::provider_chat::Prepared, CommandError> {
    let guard = persistence.store.lock().expect("the store is not poisoned");
    prepare(&guard, &[], request(workspace, question), enabled)
}

/// A project with every check passed except the one a test removes.
fn ready(scratch: &Scratch, persistence: &Persistence) -> String {
    let workspace = project(scratch, persistence, "ready");
    grant(persistence, &workspace);
    set_model(persistence, &workspace, MODEL);
    save_key_reference(persistence);
    workspace
}

#[test]
fn the_switch_is_on_by_default_keeps_the_choice_and_refuses_before_any_check_when_off() {
    let (scratch, persistence) = fixture("chat-switch");
    {
        let guard = persistence.store.lock().expect("the store is not poisoned");
        assert!(
            status(&guard).expect("read").enabled,
            "a new store has the chat on"
        );
        assert!(!set_enabled(&guard, false).expect("written").enabled);
        assert!(!is_enabled(&guard).expect("read"), "the choice is kept");
        assert!(set_enabled(&guard, true).expect("written").enabled);
    }
    let workspace = ready(&scratch, &persistence);

    assert_eq!(
        check(&persistence, &workspace, "pergunta", false),
        Err(CommandError::ChatDisabled),
        "with the switch off, a fully ready project is still refused"
    );
    assert_eq!(
        check(&persistence, "/synthetic/never-saved", "", false),
        Err(CommandError::ChatDisabled),
        "the switch is checked before the question and the project"
    );
}

#[test]
fn a_ready_question_passes_with_its_model_and_key_reference_and_its_text_trimmed() {
    let (scratch, persistence) = fixture("chat-ready");
    let workspace = ready(&scratch, &persistence);

    let prepared = check(&persistence, &workspace, "  o que mudou?  ", true).expect("ready");
    assert_eq!(prepared.question, "o que mudou?");
    assert_eq!(
        prepared.model_id, MODEL,
        "the default model is named, free variant included"
    );
    assert_eq!(prepared.key_reference, KEY_REFERENCE);
    assert_eq!(prepared.workspace, workspace);
}

#[test]
fn the_checks_run_in_order_consent_then_model_then_key() {
    let (scratch, persistence) = fixture("chat-order");
    let workspace = project(&scratch, &persistence, "order");
    assert_eq!(
        check(&persistence, &workspace, "pergunta", true),
        Err(CommandError::ConsentRequired),
        "no consent is the first refusal"
    );

    grant(&persistence, &workspace);
    assert_eq!(
        check(&persistence, &workspace, "pergunta", true),
        Err(CommandError::ModelNotSet),
        "without a default model, the consent is not enough"
    );

    set_model(&persistence, &workspace, MODEL);
    assert_eq!(
        check(&persistence, &workspace, "pergunta", true),
        Err(CommandError::KeyNotConfigured),
        "without a saved key, the model is not enough"
    );

    save_key_reference(&persistence);
    assert!(check(&persistence, &workspace, "pergunta", true).is_ok());
}

#[test]
fn an_empty_or_too_long_question_is_refused_before_the_project_is_looked_at() {
    let (_scratch, persistence) = fixture("chat-question");
    for question in ["", "   \n\t  ", &"x".repeat(8_001)] {
        assert_eq!(
            check(&persistence, "/synthetic/never-saved", question, true),
            Err(CommandError::InvalidInput),
            "a question of {} characters is refused",
            question.chars().count()
        );
    }
    let at_limit = "x".repeat(8_000);
    assert_eq!(
        check(&persistence, "/synthetic/never-saved", &at_limit, true),
        Err(CommandError::UnknownWorkspace),
        "a question at the limit passes the question check, and the project is then refused"
    );
}

#[test]
fn a_project_that_is_not_saved_authorized_or_available_is_refused() {
    let (scratch, persistence) = fixture("chat-project");
    assert_eq!(
        check(&persistence, "/synthetic/never-saved", "pergunta", true),
        Err(CommandError::UnknownWorkspace)
    );

    let workspace = ready(&scratch, &persistence);
    persistence
        .store
        .lock()
        .expect("the store is not poisoned")
        .connection()
        .execute(
            "UPDATE workspaces SET authorized = 0 WHERE path = ?1",
            [&workspace],
        )
        .expect("the project is marked as restored");
    assert_eq!(
        check(&persistence, &workspace, "pergunta", true),
        Err(CommandError::UnknownWorkspace),
        "a restored project is not authorized, and nothing is read"
    );
    persistence
        .store
        .lock()
        .expect("the store is not poisoned")
        .connection()
        .execute(
            "UPDATE workspaces SET authorized = 1 WHERE path = ?1",
            [&workspace],
        )
        .expect("the project is authorized again");

    std::fs::remove_dir_all(&workspace).expect("the folder is removed");
    assert_eq!(
        check(&persistence, &workspace, "pergunta", true),
        Err(CommandError::UnknownWorkspace),
        "a project whose folder is gone is not available"
    );
    assert!(
        services::authorized_workspace(
            &persistence.store.lock().expect("the store is not poisoned"),
            &[],
            &workspace
        )
        .is_err()
    );
}

#[test]
fn the_context_is_the_top_level_text_the_rules_allow_and_nothing_else() {
    let (scratch, _persistence) = fixture("chat-context");
    let folder = scratch.path().join("context");
    std::fs::create_dir(&folder).expect("the folder is created");
    std::fs::write(folder.join("README.md"), "# Projeto\nTexto do leitor\n").unwrap();
    std::fs::write(folder.join("notes.txt"), "anotações\n").unwrap();
    std::fs::write(folder.join(".env"), "SEGREDO=valor-que-nao-sai\n").unwrap();
    std::fs::write(folder.join("big.md"), "x".repeat(40 * 1024)).unwrap();
    std::fs::write(folder.join("data.bin"), [0xff, 0xfe, 0x00]).unwrap();
    std::fs::create_dir(folder.join("subdir")).unwrap();
    std::fs::write(folder.join("subdir").join("inner.md"), "dentro\n").unwrap();

    let files = project_context(&folder).expect("the context is read");
    let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, vec!["README.md", "notes.txt"]);
    assert!(files[0].1.contains("Texto do leitor"));
    assert!(
        !files
            .iter()
            .any(|(_, text)| text.contains("valor-que-nao-sai")),
        "a sensitive file is never read into the context"
    );
}

#[test]
fn the_request_names_the_model_and_carries_the_question_and_files_but_no_key() {
    let files = vec![("README.md".to_string(), "# Projeto\n".to_string())];
    let text = body(MODEL, "o que é isto?", &files);
    let value: serde_json::Value = serde_json::from_str(&text).expect("the body is JSON");
    assert_eq!(value["model"], MODEL);
    assert_eq!(value["stream"], true);
    assert_eq!(value["messages"][1]["content"], "o que é isto?");
    assert!(
        value["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("# Projeto")
    );
    assert!(!text.contains(KEY), "the key is never in the body");
}

fn run_for(server: &FakeServer, request_body: String) -> ChatRun {
    ChatRun {
        approvals: local_approval(server.port),
        resolver: Arc::new(Resolve(|| -> std::io::Result<Vec<IpAddr>> {
            Err(std::io::Error::other(
                "no lookup is expected for an IP literal",
            ))
        })),
        source: ProviderSource::Local,
        url: local_url(server.port),
        key: SecretString::from(KEY.to_string()),
        body: request_body,
        limits: limits(),
    }
}

fn collect(run: ChatRun) -> Vec<ChatEvent> {
    let mut events = Vec::new();
    tauri::async_runtime::block_on(run_chat(run, &mut |event| events.push(event)));
    events
}

fn delta(text: &str) -> ChatEvent {
    ChatEvent::Delta {
        text: text.to_string(),
    }
}

fn failed(code: &str) -> ChatEvent {
    ChatEvent::Failed {
        code: code.to_string(),
    }
}

#[test]
fn a_streamed_answer_arrives_in_order_and_ends_with_done() {
    let server = serve(|_, stream| {
        use std::io::Write;
        let body = format!("{}data: [DONE]\n\n", events(&["ol", "á"]));
        let _ = stream.write_all(format!("{}{body}", head("200 OK", "")).as_bytes());
    });
    assert_eq!(
        collect(run_for(&server, "{}".to_string())),
        vec![delta("ol"), delta("á"), ChatEvent::Done]
    );
}

#[test]
fn a_reply_cut_before_done_reports_what_came_before_and_then_a_truncated_failure() {
    let server = serve(|_, stream| {
        use std::io::Write;
        let _ =
            stream.write_all(format!("{}{}", head("200 OK", ""), events(&["parcial"])).as_bytes());
    });
    assert_eq!(
        collect(run_for(&server, "{}".to_string())),
        vec![delta("parcial"), failed(ChatFailure::Truncated.code())]
    );
}

#[test]
fn a_malformed_event_is_a_malformed_failure() {
    let server = serve(|_, stream| {
        use std::io::Write;
        let _ = stream.write_all(format!("{}data: {{ruim\n\n", head("200 OK", "")).as_bytes());
    });
    assert_eq!(
        collect(run_for(&server, "{}".to_string())),
        vec![failed(ChatFailure::Malformed.code())]
    );
}

#[test]
fn a_redirect_is_reported_with_the_policy_code_and_its_location_is_never_requested() {
    let server = serve(|_, stream| {
        use std::io::Write;
        let _ = stream.write_all(
            "HTTP/1.1 302 Found\r\nLocation: http://10.0.0.1:9/\r\nConnection: close\r\n\r\n"
                .as_bytes(),
        );
    });
    assert_eq!(
        collect(run_for(&server, "{}".to_string())),
        vec![failed(EndpointError::RedirectRefused.code())]
    );
    assert_eq!(server.connections(), 1, "the location is not followed");
}

#[test]
fn a_name_that_resolves_to_a_private_address_is_refused_before_any_connection() {
    let server = serve(|_, _| {});
    let mut run = run_for(&server, "{}".to_string());
    run.approvals = vec![Approval {
        host: "api.test.invalid",
        port: server.port,
        source: ProviderSource::Remote,
    }];
    run.source = ProviderSource::Remote;
    run.url = format!(
        "https://api.test.invalid:{}/api/v1/chat/completions",
        server.port
    );
    run.resolver = Arc::new(Resolve(|| -> std::io::Result<Vec<IpAddr>> {
        Ok(vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5))])
    }));
    assert_eq!(
        collect(run),
        vec![failed(EndpointError::AddressNotAllowed.code())]
    );
    assert_eq!(server.connections(), 0, "no connection was made");
}

#[test]
fn an_error_reply_reports_its_fixed_code_and_the_key_is_in_no_event() {
    let server = serve(|_, stream| {
        use std::io::Write;
        let text = format!("{{\"error\":\"rejeitou Bearer {KEY}\"}}");
        let _ = stream.write_all(
            format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            )
            .as_bytes(),
        );
    });
    let events = collect(run_for(&server, "{}".to_string()));
    assert_eq!(events, vec![failed(ChatFailure::Unauthorized.code())]);
    assert!(
        !format!("{events:?}").contains(KEY),
        "no event holds the key"
    );
}

#[test]
fn a_cancelled_call_sends_no_more_events_and_clears_the_control() {
    let server = serve(|_, stream| {
        use std::io::Write;
        let _ = stream.write_all(format!("{}{}", head("200 OK", ""), events(&["a"])).as_bytes());
        let _ = stream.flush();
        thread::sleep(Duration::from_secs(5));
    });
    let control = Arc::new(ChatControl::default());
    let sink: Arc<Mutex<Vec<ChatEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let run = run_for(&server, "{}".to_string());
    let shared = Arc::clone(&sink);
    control
        .start(async move {
            run_chat(run, &mut |event| shared.lock().unwrap().push(event)).await;
        })
        .expect("the call starts");
    assert!(control.is_running());

    let deadline = Instant::now() + Duration::from_secs(3);
    while sink.lock().unwrap().is_empty() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let started = Instant::now();
    assert!(control.cancel(), "the running call is stopped");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the cancel does not wait for the reply"
    );
    thread::sleep(Duration::from_millis(200));
    assert_eq!(
        *sink.lock().unwrap(),
        vec![delta("a")],
        "nothing comes after the cancel"
    );
    assert!(!control.is_running());
    assert!(!control.cancel(), "a second cancel finds nothing");
}

#[test]
fn a_second_question_is_refused_while_one_runs_and_the_next_one_starts_after_it_ends() {
    let control = Arc::new(ChatControl::default());
    control
        .start(std::future::pending::<()>())
        .expect("the first call starts");
    assert_eq!(
        control.start(async {}),
        Err(CommandError::ChatBusy),
        "a second question is refused while the first runs"
    );
    assert!(control.cancel());

    control
        .start(async {})
        .expect("a question starts once the call is stopped");
    let deadline = Instant::now() + Duration::from_secs(2);
    while control.is_running() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !control.is_running(),
        "a call that ends clears its own entry"
    );
    control.start(async {}).expect("and the next one starts");
}

#[test]
fn the_default_model_of_another_provider_is_not_the_chat_model() {
    let (scratch, persistence) = fixture("chat-provider");
    let workspace = project(&scratch, &persistence, "other-provider");
    grant(&persistence, &workspace);
    save_key_reference(&persistence);
    persistence
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspace_model_bindings()
        .set(WorkspaceModelBindings {
            workspace_id: workspace.clone(),
            default_binding: Some(ModelReference {
                provider_id: "local-embeddings".to_string(),
                model_id: "nomic-embed".to_string(),
            }),
            embedding_binding: None,
            remote_embedding_allowed: None,
        })
        .expect("the binding is saved");
    assert_eq!(
        check(&persistence, &workspace, "pergunta", true),
        Err(CommandError::ModelNotSet),
        "a model of another provider does not answer the OpenRouter chat"
    );
}
