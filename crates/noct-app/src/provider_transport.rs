//! The provider transport: the one place where a connection to a provider is opened.
//!
//! A call runs in this order. The endpoint is checked against the approvals. The name is resolved once, and every address
//! of the answer is checked against the scope of the source. The connection is pinned to the addresses that passed, so no
//! second lookup can change the target. Proxies are off, because the updater turns the system-proxy feature on for the
//! whole workspace and a proxy would carry the request past the check. Redirects are refused, not followed. The time to
//! connect, the silence between reads and the whole call are capped. The reply is read under a size cap and an answer cap.
//! The key goes in one header, marked sensitive, and never into a message, a body or a log.

use std::net::{IpAddr, SocketAddr};
use std::sync::Once;
use std::time::Duration;

use noct_core::ai::ProviderSource;
use noct_core::ai::network::{
    AddressResolver, Approval, BUILTIN_APPROVALS, Endpoint, Scheme, check_endpoint_in,
    check_status, parse_url, verify_addresses,
};
use noct_core::ai::openrouter::{ChatFailure, StreamEvent, read_event, status_failure};
use noct_core::ai::sse::SseParser;
use noct_core::limits::provider_chat::MAXIMUM_ANSWER_CHARACTERS;
use noct_core::limits::provider_network::{
    CONNECT_TIMEOUT_MS, MAXIMUM_RESPONSE_BYTES, REQUEST_TIMEOUT_MS, STALL_TIMEOUT_MS,
};
use noct_secrets::{ExposeSecret, SecretString};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue};

/// The time limits and caps of one call. Production uses [`Limits::beta_one`]; the transport tests pass shorter times, so
/// that a stall can be seen in a test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub connect: Duration,
    pub stall: Duration,
    pub total: Duration,
    pub reply_bytes: u64,
    pub answer_characters: usize,
}

impl Limits {
    pub fn beta_one() -> Self {
        Self {
            connect: Duration::from_millis(CONNECT_TIMEOUT_MS),
            stall: Duration::from_millis(STALL_TIMEOUT_MS),
            total: Duration::from_millis(REQUEST_TIMEOUT_MS),
            reply_bytes: MAXIMUM_RESPONSE_BYTES,
            answer_characters: MAXIMUM_ANSWER_CHARACTERS,
        }
    }
}

/// The operating system resolver, behind the policy's trait. A lookup blocks, so the policy runs on a blocking thread.
pub struct SystemResolver;

impl AddressResolver for SystemResolver {
    fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<IpAddr>> {
        use std::net::ToSocketAddrs;
        Ok((host, port)
            .to_socket_addrs()?
            .map(|socket| socket.ip())
            .collect())
    }
}

/// A checked endpoint and the addresses it resolved to. The connection uses these addresses and no other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    endpoint: Endpoint,
    addresses: Vec<IpAddr>,
}

impl Target {
    pub fn addresses(&self) -> &[IpAddr] {
        &self.addresses
    }
}

/// Checks the URL against the approvals, resolves its host once and checks every address. Nothing is connected here.
pub fn verify_target(
    approvals: &[Approval],
    resolver: &dyn AddressResolver,
    url: &str,
    source: ProviderSource,
) -> Result<Target, ChatFailure> {
    let endpoint = parse_url(url).map_err(ChatFailure::Endpoint)?;
    check_endpoint_in(approvals, &endpoint, source).map_err(ChatFailure::Endpoint)?;
    let addresses = verify_addresses(resolver, &endpoint, source).map_err(ChatFailure::Endpoint)?;
    Ok(Target {
        endpoint,
        addresses,
    })
}

/// The production check: the built-in approvals, the operating system resolver, and a remote source. The resolver runs on
/// a blocking thread, so the command that asks is never blocked by a lookup.
pub async fn verify_openrouter(url: String) -> Result<Target, ChatFailure> {
    tauri::async_runtime::spawn_blocking(move || {
        verify_target(
            BUILTIN_APPROVALS,
            &SystemResolver,
            &url,
            ProviderSource::Remote,
        )
    })
    .await
    .map_err(|_| ChatFailure::Transport)?
}

/// Sends one chat request to the verified target and reports each piece of the answer as it arrives. Returns when the
/// answer ends with `[DONE]`. Any other end is a truncated answer.
pub async fn stream_chat(
    target: &Target,
    key: &SecretString,
    body: String,
    limits: &Limits,
    on_delta: &mut (dyn FnMut(&str) + Send),
) -> Result<(), ChatFailure> {
    ensure_tls_provider();
    let endpoint = &target.endpoint;
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(limits.connect)
        .read_timeout(limits.stall)
        .timeout(limits.total);
    if endpoint.host.parse::<IpAddr>().is_err() {
        let sockets: Vec<SocketAddr> = target
            .addresses
            .iter()
            .map(|address| SocketAddr::new(*address, endpoint.port))
            .collect();
        builder = builder.resolve_to_addrs(&endpoint.host, &sockets);
    }
    let client = builder.build().map_err(|_| ChatFailure::Transport)?;

    let mut authorization = HeaderValue::from_str(&format!("Bearer {}", key.expose_secret()))
        .map_err(|_| ChatFailure::Rejected)?;
    authorization.set_sensitive(true);
    let mut response = client
        .post(url_of(endpoint))
        .header(AUTHORIZATION, authorization)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "text/event-stream")
        .body(body)
        .send()
        .await
        .map_err(failure_of)?;

    let status = response.status().as_u16();
    check_status(status).map_err(ChatFailure::Endpoint)?;
    if let Some(failure) = status_failure(status) {
        // The body of an error reply is not read: it can echo the request, and the request carries the key.
        return Err(failure);
    }

    let mut parser = SseParser::new();
    let mut seen: u64 = 0;
    let mut answer: usize = 0;
    while let Some(chunk) = response.chunk().await.map_err(failure_of)? {
        seen += chunk.len() as u64;
        if seen > limits.reply_bytes {
            return Err(ChatFailure::TooLarge);
        }
        for event in parser.push(&chunk)? {
            match read_event(&event.data)? {
                Some(StreamEvent::Delta(text)) => {
                    answer += text.chars().count();
                    if answer > limits.answer_characters {
                        return Err(ChatFailure::TooLarge);
                    }
                    on_delta(&text);
                }
                Some(StreamEvent::Done) => return Ok(()),
                None => {}
            }
        }
    }
    Err(ChatFailure::Truncated)
}

fn url_of(endpoint: &Endpoint) -> String {
    let scheme = match endpoint.scheme {
        Scheme::Https => "https",
        Scheme::Http => "http",
    };
    if endpoint.host.contains(':') {
        format!(
            "{scheme}://[{}]:{}{}",
            endpoint.host, endpoint.port, endpoint.path
        )
    } else {
        format!(
            "{scheme}://{}:{}{}",
            endpoint.host, endpoint.port, endpoint.path
        )
    }
}

fn failure_of(error: reqwest::Error) -> ChatFailure {
    if error.is_timeout() {
        ChatFailure::Timeout
    } else {
        ChatFailure::Transport
    }
}

/// Installs `ring` as the process default crypto provider, once. `rustls-no-provider` panics when a client is built with
/// no provider installed, and `ring` is the only provider in the graph. An error here means one is already installed.
fn ensure_tls_provider() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}
