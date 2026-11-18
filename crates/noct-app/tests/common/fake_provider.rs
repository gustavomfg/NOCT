//! The loopback provider used by the transport and chat tests: a fake server that answers as the provider would, and a
//! resolver stand-in. No test reaches a real provider.

#![allow(dead_code)]

use noct_app::provider_transport::{Limits, Target, stream_chat, verify_target};
use noct_core::ai::ProviderSource;
use noct_core::ai::network::{AddressResolver, Approval};
use noct_core::ai::openrouter::ChatFailure;
use noct_secrets::SecretString;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const KEY: &str = "sk-or-v1-transport-test-0123456789";
pub const SSE: &str = "text/event-stream";

/// A resolver that answers with a fixed result, for the policy checks that do not need a real name.
pub struct Resolve<F>(pub F);

impl<F: Fn() -> std::io::Result<Vec<IpAddr>> + Send + Sync> AddressResolver for Resolve<F> {
    fn resolve(&self, _host: &str, _port: u16) -> std::io::Result<Vec<IpAddr>> {
        (self.0)()
    }
}

/// Reads one request: its head, and the body its `Content-Length` announces.
pub fn read_request(stream: &mut TcpStream) -> String {
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
pub struct FakeServer {
    pub port: u16,
    pub requests: Arc<Mutex<Vec<String>>>,
}

impl FakeServer {
    pub fn connections(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

pub fn serve(reply: impl Fn(&str, &mut TcpStream) + Send + Sync + 'static) -> FakeServer {
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
pub fn head(status: &str, extra: &str) -> String {
    format!("HTTP/1.1 {status}\r\nContent-Type: {SSE}\r\nConnection: close\r\n{extra}\r\n")
}

pub fn events(deltas: &[&str]) -> String {
    let mut body = String::new();
    for delta in deltas {
        body.push_str(&format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{delta}\"}}}}]}}\n\n"
        ));
    }
    body
}

pub fn local_approval(port: u16) -> Vec<Approval> {
    vec![Approval {
        host: "127.0.0.1",
        port,
        source: ProviderSource::Local,
    }]
}

pub fn local_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/api/v1/chat/completions")
}

pub fn limits() -> Limits {
    Limits {
        connect: Duration::from_secs(2),
        stall: Duration::from_millis(1_500),
        total: Duration::from_secs(10),
        reply_bytes: 2 * 1024 * 1024,
        answer_characters: 64 * 1024,
    }
}

/// The local target of a fake server. The IP literal is its own answer, so the resolver is not used.
pub fn local_target(port: u16) -> Target {
    verify_target(
        &local_approval(port),
        &Resolve(|| panic!("an IP literal is not resolved")),
        &local_url(port),
        ProviderSource::Local,
    )
    .expect("the loopback target passes the policy")
}

pub fn key() -> SecretString {
    SecretString::from(KEY.to_string())
}

/// Streams a chat to the target and returns the answer and the outcome.
pub fn chat(target: &Target, limits: &Limits) -> (String, Result<(), ChatFailure>) {
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
