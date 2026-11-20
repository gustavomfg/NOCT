//! The OpenRouter model list against a fake server on loopback: the real client and the real policy, with the approvals
//! injected. No test reaches OpenRouter, and no test uses the operating system resolver.

mod common;

use std::future::Future;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::fake_provider::{KEY, Resolve, limits, local_target, serve};
use noct_app::error::CommandError;
use noct_app::model_catalog::{self, ModelCatalog, fetch_from, openrouter_models_url};
use noct_app::provider_transport::verify_target;
use noct_core::ai::ProviderSource;
use noct_core::ai::catalog::CatalogModel;
use noct_core::ai::network::{Approval, BUILTIN_APPROVALS, EndpointError};
use noct_core::ai::openrouter::ChatFailure;

const LIST: &str = r#"{"data":[
  {"id":"deepseek/deepseek-chat-v3:free","name":"DeepSeek V3","context_length":131072,"pricing":{"prompt":"0","completion":"0"},"description":"<img src=x onerror=alert(1)>"},
  {"id":"openai/gpt-4o","name":"GPT-4o","context_length":128000,"pricing":{"prompt":"0.0000025","completion":"0.00001"}},
  {"id":"../../etc/passwd","name":"bad"},
  {"id":"vendor/<b>","name":"bad"}
]}"#;

fn json_reply(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn read(port: u16) -> Result<Vec<CatalogModel>, CommandError> {
    tauri::async_runtime::block_on(fetch_from(&local_target(port), &limits()))
}

#[test]
fn a_valid_list_is_read_into_its_shown_fields_and_bad_ids_are_dropped() {
    let server = serve(|_, stream| {
        let _ = stream.write_all(json_reply(LIST).as_bytes());
    });
    let models = read(server.port).expect("the list is read");
    let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
    assert_eq!(ids, ["deepseek/deepseek-chat-v3:free", "openai/gpt-4o"]);
    assert!(models[0].free && !models[1].free);
    assert_eq!(models[0].context_length, Some(131_072));
    let shown = serde_json::to_string(&models).expect("serializes");
    assert!(
        !shown.contains("onerror") && !shown.contains("description"),
        "no other field reaches the window"
    );
}

#[test]
fn the_request_is_a_get_that_carries_no_key_and_no_authorization() {
    let server = serve(|_, stream| {
        let _ = stream.write_all(json_reply(LIST).as_bytes());
    });
    read(server.port).expect("the list is read");
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let request = requests[0].to_ascii_lowercase();
    assert!(
        request.starts_with("get "),
        "the list is read with GET: {request}"
    );
    assert!(
        !request.contains("authorization"),
        "no Authorization header"
    );
    assert!(!request.contains(&KEY.to_ascii_lowercase()), "no key");
    assert!(!request.contains("cookie"), "no cookie");
}

#[test]
fn a_reply_over_the_byte_cap_is_refused_with_or_without_a_length() {
    let announced = serve(|_, stream| {
        let _ = stream.write_all(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 99999999\r\nConnection: close\r\n\r\n{"
                .as_bytes(),
        );
    });
    let mut small = limits();
    small.reply_bytes = 64 * 1024;
    assert_eq!(
        tauri::async_runtime::block_on(fetch_from(&local_target(announced.port), &small)),
        Err(CommandError::ModelsTooLarge)
    );

    let streamed = serve(|_, stream| {
        let _ = stream.write_all(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"data\":[".as_bytes(),
        );
        let entry = format!("{{\"id\":\"v/m\",\"name\":\"{}\"}},", "x".repeat(900));
        for _ in 0..4_000 {
            if stream.write_all(entry.as_bytes()).is_err() {
                break;
            }
        }
    });
    assert_eq!(
        tauri::async_runtime::block_on(fetch_from(&local_target(streamed.port), &small)),
        Err(CommandError::ModelsTooLarge)
    );
}

#[test]
fn a_cut_reply_is_never_read_as_a_shorter_list() {
    // With no length, the connection closes in the middle of the JSON: the list is invalid, not cut.
    let unannounced = serve(|_, stream| {
        let _ = stream.write_all(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"data\":[{\"id\":\"v/a\"},"
                .as_bytes(),
        );
    });
    assert_eq!(read(unannounced.port), Err(CommandError::ModelsInvalid));
    // With a length, a body shorter than it fails in the transport.
    let announced = serve(|_, stream| {
        let _ = stream.write_all(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 500\r\nConnection: close\r\n\r\n{\"data\":["
                .as_bytes(),
        );
    });
    assert_eq!(read(announced.port), Err(CommandError::ModelsNetwork));
}

#[test]
fn a_redirect_is_refused_and_its_location_is_never_requested() {
    let server = serve(|_, stream| {
        let _ = stream.write_all(
            "HTTP/1.1 302 Found\r\nLocation: http://10.0.0.1:9/models\r\nConnection: close\r\n\r\n"
                .as_bytes(),
        );
    });
    assert_eq!(read(server.port), Err(CommandError::ModelsNetwork));
    assert_eq!(server.connections(), 1, "the location is not followed");
}

#[test]
fn a_reply_that_is_not_a_list_is_invalid() {
    let server = serve(|_, stream| {
        let _ = stream.write_all(json_reply("<html><body>login</body></html>").as_bytes());
    });
    assert_eq!(read(server.port), Err(CommandError::ModelsInvalid));
}

#[test]
fn the_list_url_is_the_builtin_endpoint_and_a_private_answer_is_refused_before_any_connection() {
    assert_eq!(
        openrouter_models_url(),
        "https://openrouter.ai/api/v1/models"
    );
    let public = verify_target(
        BUILTIN_APPROVALS,
        &Resolve(|| Ok(vec![IpAddr::V4(Ipv4Addr::new(104, 18, 2, 3))])),
        &openrouter_models_url(),
        ProviderSource::Remote,
    );
    assert!(public.is_ok(), "a public answer passes");

    let server = serve(|_, _| {});
    let approvals = [Approval {
        host: "models.test.invalid",
        port: server.port,
        source: ProviderSource::Remote,
    }];
    let refused = verify_target(
        &approvals,
        &Resolve(|| Ok(vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))])),
        &format!("https://models.test.invalid:{}/api/v1/models", server.port),
        ProviderSource::Remote,
    );
    let failure = refused.expect_err("refused");
    assert_eq!(
        failure,
        ChatFailure::Endpoint(EndpointError::AddressNotAllowed)
    );
    assert_eq!(
        model_catalog::failure_of(failure),
        CommandError::ModelsNetwork
    );
    assert_eq!(server.connections(), 0, "no connection was made");

    let other_host = verify_target(
        BUILTIN_APPROVALS,
        &Resolve(|| Ok(vec![IpAddr::V4(Ipv4Addr::new(104, 18, 2, 3))])),
        "https://example.com/api/v1/models",
        ProviderSource::Remote,
    );
    assert!(other_host.is_err(), "only openrouter.ai is approved");
}

fn one_model() -> Vec<CatalogModel> {
    vec![CatalogModel {
        id: "v/a".into(),
        name: "A".into(),
        context_length: None,
        free: true,
    }]
}

#[test]
fn a_list_is_kept_for_an_hour_and_a_failure_is_not_kept() {
    let catalog = ModelCatalog::default();
    let reads = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();
    let fetch = |reads: Arc<AtomicUsize>, result: Result<Vec<CatalogModel>, CommandError>| {
        move || async move {
            reads.fetch_add(1, Ordering::SeqCst);
            result
        }
    };

    let failed = tauri::async_runtime::block_on(catalog.list(
        start,
        fetch(Arc::clone(&reads), Err(CommandError::ModelsNetwork)),
    ));
    assert_eq!(failed, Err(CommandError::ModelsNetwork));
    let first = tauri::async_runtime::block_on(
        catalog.list(start, fetch(Arc::clone(&reads), Ok(one_model()))),
    );
    assert_eq!(first.expect("read").as_ref(), &one_model());
    assert_eq!(reads.load(Ordering::SeqCst), 2, "a failed read is not kept");

    let later = start + Duration::from_secs(3_599);
    tauri::async_runtime::block_on(catalog.list(later, fetch(Arc::clone(&reads), Ok(Vec::new()))))
        .expect("kept");
    assert_eq!(
        reads.load(Ordering::SeqCst),
        2,
        "within the hour the kept list is used"
    );

    let expired = start + Duration::from_secs(3_600);
    let again = tauri::async_runtime::block_on(
        catalog.list(expired, fetch(Arc::clone(&reads), Ok(Vec::new()))),
    );
    assert!(again.expect("read").is_empty());
    assert_eq!(
        reads.load(Ordering::SeqCst),
        3,
        "after an hour the list is read again"
    );
}

#[test]
fn a_running_read_can_be_cancelled_and_is_not_kept() {
    let catalog = Arc::new(ModelCatalog::default());
    let running = Arc::clone(&catalog);
    let task = tauri::async_runtime::spawn(async move {
        running
            .list(Instant::now(), || async {
                loop {
                    tauri::async_runtime::spawn_blocking(|| {
                        std::thread::sleep(Duration::from_millis(20))
                    })
                    .await
                    .ok();
                }
            })
            .await
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !catalog.cancel() {
        assert!(Instant::now() < deadline, "the read started");
        std::thread::sleep(Duration::from_millis(10));
    }
    let outcome = tauri::async_runtime::block_on(task).expect("the task ends");
    assert_eq!(outcome, Err(CommandError::ModelsCancelled));
    assert!(!catalog.cancel(), "nothing runs after the cancel");
    assert!(
        catalog.fresh(Instant::now()).is_none(),
        "a cancelled read is not kept"
    );
}

#[test]
fn a_second_read_while_one_runs_is_refused() {
    let catalog = Arc::new(ModelCatalog::default());
    let running = Arc::clone(&catalog);
    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let release_rx = Arc::new(std::sync::Mutex::new(release_rx));
    let task = tauri::async_runtime::spawn(async move {
        running
            .list(Instant::now(), move || async move {
                let _ = started_tx.send(());
                let wait = Arc::clone(&release_rx);
                tauri::async_runtime::spawn_blocking(move || wait.lock().unwrap().recv())
                    .await
                    .ok();
                Ok(one_model())
            })
            .await
    });
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the first read started");
    let second =
        tauri::async_runtime::block_on(catalog.list(Instant::now(), || async { Ok(Vec::new()) }));
    assert_eq!(second, Err(CommandError::ModelsBusy));
    release_tx.send(()).expect("released");
    let first = tauri::async_runtime::block_on(task).expect("the task ends");
    assert_eq!(first.expect("read").as_ref(), &one_model());
}

#[test]
fn a_read_whose_caller_is_dropped_does_not_leave_the_list_busy() {
    let catalog = ModelCatalog::default();
    {
        let read = catalog.list(Instant::now(), || async { Ok(one_model()) });
        let mut read = Box::pin(read);
        // Poll the call once, so the read is marked as running, then drop it before it ends.
        let waker = std::task::Waker::noop();
        let mut context = std::task::Context::from_waker(waker);
        let _ = read.as_mut().poll(&mut context);
    }
    let again =
        tauri::async_runtime::block_on(catalog.list(Instant::now(), || async { Ok(Vec::new()) }));
    assert!(
        again.is_ok(),
        "a later read is not refused as busy: {again:?}"
    );
}
