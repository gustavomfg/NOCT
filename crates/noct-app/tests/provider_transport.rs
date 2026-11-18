//! The provider transport against a fake server on loopback. The approvals are injected, so the real client and the real
//! policy run against a local listener; no test reaches a provider, and no test uses the operating system resolver.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use noct_app::provider_transport::{Limits, Target, stream_chat, verify_target};
use noct_core::ai::ProviderSource;
use noct_core::ai::network::{AddressResolver, Approval, BUILTIN_APPROVALS, EndpointError};
use noct_core::ai::openrouter::ChatFailure;
use noct_secrets::SecretString;

const KEY: &str = "sk-or-v1-transport-test-0123456789";
const SSE: &str = "text/event-stream";

/// A resolver that answers with a fixed result, for the policy checks that do not need a real name.
struct Resolve<F>(F);

impl<F: Fn() -> std::io::Result<Vec<IpAddr>> + Send + Sync> AddressResolver for Resolve<F> {
    fn resolve(&self, _host: &str, _port: u16) -> std::io::Result<Vec<IpAddr>> {
        (self.0)()
    }
}

/// Reads one request: its head, and the body its `Content-Length` announces.
fn read_request(stream: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            break;
        }
        head.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&head).to_string();
    let length = text
        .lines()
        .find_map(|line| {
            let lower = line.to_ascii_lowercase();
            lower
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse::<usize>().unwrap_or(0))
        })
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    let _ = stream.read_exact(&mut body);
    format!("{text}{}", String::from_utf8_lossy(&body))
}

/// A fake provider. Each connection is read, then `reply` writes whatever it wants, including a stall. The requests are
/// kept, so a test can count the connections that were made.
struct FakeServer {
    port: u16,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FakeServer {
    fn connections(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

fn serve(reply: impl Fn(&str, &mut TcpStream) + Send + Sync + 'static) -> FakeServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&requests);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let request = read_request(&mut stream);
            log.lock().unwrap().push(request.clone());
            reply(&request, &mut stream);
            let _ = stream.flush();
        }
    });
    FakeServer { port, requests }
}

/// A reply head with no length: the body ends when the connection closes.
fn head(status: &str, extra: &str) -> String {
    format!("HTTP/1.1 {status}\r\nContent-Type: {SSE}\r\nConnection: close\r\n{extra}\r\n")
}

fn events(deltas: &[&str]) -> String {
    let mut body = String::new();
    for delta in deltas {
        body.push_str(&format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{delta}\"}}}}]}}\n\n"
        ));
    }
    body
}

fn local_approval(port: u16) -> Vec<Approval> {
    vec![Approval {
        host: "127.0.0.1",
        port,
        source: ProviderSource::Local,
    }]
}

fn local_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/api/v1/chat/completions")
}

fn limits() -> Limits {
    Limits {
        connect: Duration::from_secs(2),
        stall: Duration::from_millis(1_500),
        total: Duration::from_secs(10),
        reply_bytes: 2 * 1024 * 1024,
        answer_characters: 64 * 1024,
    }
}

/// The local target of a fake server. The IP literal is its own answer, so the resolver is not used.
fn local_target(port: u16) -> Target {
    verify_target(
        &local_approval(port),
        &Resolve(|| panic!("an IP literal is not resolved")),
        &local_url(port),
        ProviderSource::Local,
    )
    .expect("the loopback target passes the policy")
}

fn key() -> SecretString {
    SecretString::from(KEY.to_string())
}

/// Streams a chat to the target and returns the answer and the outcome.
fn chat(target: &Target, limits: &Limits) -> (String, Result<(), ChatFailure>) {
    let mut answer = String::new();
    let mut sink = |delta: &str| answer.push_str(delta);
    let outcome = tauri::async_runtime::block_on(stream_chat(
        target,
        &key(),
        "{\"model\":\"a/b\",\"messages\":[],\"stream\":true}".to_string(),
        limits,
        &mut sink,
    ));
    (answer, outcome)
}

#[test]
fn a_streamed_answer_is_read_in_order_until_done() {
    let server = serve(|_, stream| {
        let body = format!("{}data: [DONE]\n\n", events(&["ol", "á"]));
        let _ = stream.write_all(format!("{}{body}", head("200 OK", "")).as_bytes());
    });
    let (answer, outcome) = chat(&local_target(server.port), &limits());
    assert_eq!(outcome, Ok(()));
    assert_eq!(answer, "olá");
}

#[test]
fn the_key_is_sent_in_one_header_and_the_body_holds_no_key() {
    let server = serve(|_, stream| {
        let _ = stream.write_all(format!("{}data: [DONE]\n\n", head("200 OK", "")).as_bytes());
    });
    let (_, outcome) = chat(&local_target(server.port), &limits());
    assert_eq!(outcome, Ok(()));
    let requests = server.requests.lock().unwrap();
    let request = &requests[0];
    assert!(
        request
            .to_ascii_lowercase()
            .contains(&format!("authorization: bearer {KEY}")),
        "the key travels in the authorization header"
    );
    let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
    assert!(!body.contains(KEY), "the body holds no key");
}

#[test]
fn a_reply_cut_before_done_is_truncated() {
    let server = serve(|_, stream| {
        let _ =
            stream.write_all(format!("{}{}", head("200 OK", ""), events(&["partial"])).as_bytes());
    });
    let (answer, outcome) = chat(&local_target(server.port), &limits());
    assert_eq!(outcome, Err(ChatFailure::Truncated));
    assert_eq!(
        answer, "partial",
        "the text before the cut is what was received"
    );
}

#[test]
fn an_event_that_is_not_json_is_malformed() {
    let server = serve(|_, stream| {
        let _ = stream.write_all(format!("{}data: {{bad\n\n", head("200 OK", "")).as_bytes());
    });
    let (_, outcome) = chat(&local_target(server.port), &limits());
    assert_eq!(outcome, Err(ChatFailure::Malformed));
}

#[test]
fn a_reply_with_less_body_than_its_length_is_a_transport_failure() {
    let server = serve(|_, stream| {
        let _ = stream.write_all(
            format!(
                "{}{}",
                head("200 OK", "Content-Length: 1000\r\n"),
                events(&["a"])
            )
            .as_bytes(),
        );
    });
    let (_, outcome) = chat(&local_target(server.port), &limits());
    assert_eq!(outcome, Err(ChatFailure::Transport));
}

#[test]
fn a_redirect_is_refused_and_its_location_is_never_requested() {
    let server = serve(|_, stream| {
        let _ = stream.write_all(
            "HTTP/1.1 302 Found\r\nLocation: http://10.0.0.1:9/\r\nConnection: close\r\n\r\n"
                .as_bytes(),
        );
    });
    let (_, outcome) = chat(&local_target(server.port), &limits());
    assert_eq!(
        outcome,
        Err(ChatFailure::Endpoint(EndpointError::RedirectRefused))
    );
    assert_eq!(server.connections(), 1, "the location is not followed");
}

#[test]
fn a_reply_over_the_size_cap_is_refused_before_it_is_read_to_the_end() {
    let server = serve(|_, stream| {
        let _ = stream.write_all(head("200 OK", "").as_bytes());
        let chunk = format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n\n",
            "x".repeat(900)
        );
        for _ in 0..4_000 {
            if stream.write_all(chunk.as_bytes()).is_err() {
                break;
            }
        }
    });
    let mut small = limits();
    small.reply_bytes = 64 * 1024;
    let (_, outcome) = chat(&local_target(server.port), &small);
    assert_eq!(outcome, Err(ChatFailure::TooLarge));
}

#[test]
fn an_answer_over_the_character_cap_is_refused() {
    let server = serve(|_, stream| {
        let _ = stream.write_all(
            format!(
                "{}{}data: [DONE]\n\n",
                head("200 OK", ""),
                events(&["abcdefghijklmnopqrstuvwxyz"])
            )
            .as_bytes(),
        );
    });
    let mut small = limits();
    small.answer_characters = 10;
    let (_, outcome) = chat(&local_target(server.port), &small);
    assert_eq!(outcome, Err(ChatFailure::TooLarge));
}

#[test]
fn a_stalled_reply_times_out_at_the_stall_limit() {
    let server = serve(|_, stream| {
        let _ = stream.write_all(format!("{}{}", head("200 OK", ""), events(&["a"])).as_bytes());
        let _ = stream.flush();
        thread::sleep(Duration::from_secs(4));
        let _ = stream.write_all(b"data: [DONE]\n\n");
    });
    let mut short = limits();
    short.stall = Duration::from_millis(300);
    let started = Instant::now();
    let (answer, outcome) = chat(&local_target(server.port), &short);
    assert_eq!(outcome, Err(ChatFailure::Timeout));
    assert_eq!(answer, "a");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the stall ends the call early"
    );
}

#[test]
fn a_cancelled_call_stops_without_waiting_for_the_reply() {
    let server = serve(|_, stream| {
        let _ = stream.write_all(format!("{}{}", head("200 OK", ""), events(&["a"])).as_bytes());
        let _ = stream.flush();
        thread::sleep(Duration::from_secs(5));
    });
    let target = local_target(server.port);
    let mut long = limits();
    long.stall = Duration::from_secs(20);
    long.total = Duration::from_secs(30);
    let handle = tauri::async_runtime::spawn(async move {
        let mut sink = |_: &str| {};
        stream_chat(&target, &key(), "{}".to_string(), &long, &mut sink).await
    });
    thread::sleep(Duration::from_millis(400));
    let started = Instant::now();
    handle.abort();
    let joined = tauri::async_runtime::block_on(handle);
    assert!(
        joined.is_err(),
        "the aborted call does not finish with an answer"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the abort does not wait for the reply"
    );
}

#[test]
fn a_host_that_is_not_approved_is_refused_before_any_connection() {
    let outcome = verify_target(
        BUILTIN_APPROVALS,
        &Resolve(|| panic!("the name is not resolved for a host that is not approved")),
        "https://evil.example/api/v1/chat/completions",
        ProviderSource::Remote,
    );
    assert_eq!(
        outcome.err(),
        Some(ChatFailure::Endpoint(EndpointError::HostNotApproved))
    );
}

#[test]
fn a_remote_endpoint_over_plain_http_is_refused() {
    let outcome = verify_target(
        BUILTIN_APPROVALS,
        &Resolve(|| Ok(vec![IpAddr::V4(Ipv4Addr::new(104, 18, 0, 1))])),
        "http://openrouter.ai:443/api/v1/chat/completions",
        ProviderSource::Remote,
    );
    assert_eq!(
        outcome.err(),
        Some(ChatFailure::Endpoint(EndpointError::InvalidEndpoint))
    );
}

#[test]
fn a_name_that_resolves_to_a_private_address_is_refused_before_any_connection() {
    let server = serve(|_, _| {});
    let approvals = [Approval {
        host: "api.test.invalid",
        port: server.port,
        source: ProviderSource::Remote,
    }];
    let outcome = verify_target(
        &approvals,
        &Resolve(|| Ok(vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5))])),
        &format!(
            "https://api.test.invalid:{}/api/v1/chat/completions",
            server.port
        ),
        ProviderSource::Remote,
    );
    assert_eq!(
        outcome.err(),
        Some(ChatFailure::Endpoint(EndpointError::AddressNotAllowed))
    );
    assert_eq!(server.connections(), 0, "no connection was made");
}

#[test]
fn one_private_address_in_a_mixed_answer_refuses_the_whole_answer() {
    let outcome = verify_target(
        BUILTIN_APPROVALS,
        &Resolve(|| {
            Ok(vec![
                IpAddr::V4(Ipv4Addr::new(104, 18, 0, 1)),
                IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
            ])
        }),
        "https://openrouter.ai/api/v1/chat/completions",
        ProviderSource::Remote,
    );
    assert_eq!(
        outcome.err(),
        Some(ChatFailure::Endpoint(EndpointError::AddressNotAllowed))
    );
}

#[test]
fn a_name_with_no_address_or_no_answer_is_refused() {
    let none = verify_target(
        BUILTIN_APPROVALS,
        &Resolve(|| Ok(Vec::new())),
        "https://openrouter.ai/api/v1/chat/completions",
        ProviderSource::Remote,
    );
    assert_eq!(
        none.err(),
        Some(ChatFailure::Endpoint(EndpointError::NoAddress))
    );
    let failed = verify_target(
        BUILTIN_APPROVALS,
        &Resolve(|| Err(std::io::Error::other("no answer"))),
        "https://openrouter.ai/api/v1/chat/completions",
        ProviderSource::Remote,
    );
    assert_eq!(
        failed.err(),
        Some(ChatFailure::Endpoint(EndpointError::ResolutionFailed))
    );
}

#[test]
fn the_builtin_endpoint_accepts_a_public_answer() {
    let target = verify_target(
        BUILTIN_APPROVALS,
        &Resolve(|| {
            Ok(vec![
                IpAddr::V4(Ipv4Addr::new(104, 18, 0, 1)),
                IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0, 0, 0, 0, 0, 1)),
            ])
        }),
        "https://openrouter.ai/api/v1/chat/completions",
        ProviderSource::Remote,
    )
    .expect("a public answer passes the policy");
    assert_eq!(target.addresses().len(), 2);
}

#[test]
fn a_key_echoed_by_the_provider_is_not_in_the_failure() {
    let server = serve(|_, stream| {
        let body = format!("{{\"error\":\"rejected Bearer {KEY}\"}}");
        let _ = stream.write_all(
            format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        );
    });
    let (answer, outcome) = chat(&local_target(server.port), &limits());
    let failure = outcome.expect_err("a 401 is a failure");
    assert_eq!(failure, ChatFailure::Unauthorized);
    assert_eq!(failure.code(), "provider-unauthorized");
    assert!(answer.is_empty());
    assert!(
        !format!("{failure:?}").contains(KEY),
        "the failure holds no key"
    );
}

#[test]
fn a_rate_limit_and_a_server_error_map_to_their_fixed_codes() {
    let limited = serve(|_, stream| {
        let _ = stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nConnection: close\r\n\r\n");
    });
    assert_eq!(
        chat(&local_target(limited.port), &limits()).1,
        Err(ChatFailure::RateLimited)
    );
    let broken = serve(|_, stream| {
        let _ = stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\n\r\n");
    });
    assert_eq!(
        chat(&local_target(broken.port), &limits()).1,
        Err(ChatFailure::Unavailable)
    );
}

#[test]
fn a_connection_that_cannot_be_made_is_a_transport_failure_that_holds_no_key() {
    let port = {
        let probe = TcpListener::bind("127.0.0.1:0").unwrap();
        probe.local_addr().unwrap().port()
    };
    let target = verify_target(
        &local_approval(port),
        &Resolve(|| panic!("an IP literal is not resolved")),
        &local_url(port),
        ProviderSource::Local,
    )
    .unwrap();
    let (_, outcome) = chat(&target, &limits());
    let failure = outcome.expect_err("nothing listens on the port");
    assert_eq!(failure, ChatFailure::Transport);
    assert!(!format!("{failure:?}").contains(KEY));
}
