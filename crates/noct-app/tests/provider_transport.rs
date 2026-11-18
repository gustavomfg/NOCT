//! The provider transport against a fake server on loopback. The approvals are injected, so the real client and the real
//! policy run against a local listener; no test reaches a provider, and no test uses the operating system resolver.

mod common;

use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::thread;
use std::time::{Duration, Instant};

use common::fake_provider::*;
use noct_app::provider_transport::{stream_chat, verify_target};
use noct_core::ai::ProviderSource;
use noct_core::ai::network::{Approval, BUILTIN_APPROVALS, EndpointError};
use noct_core::ai::openrouter::ChatFailure;

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
fn a_connection_closed_before_any_reply_is_a_transport_failure_that_holds_no_key() {
    // The server owns its port for the whole test and closes each connection without a reply. A port that was freed
    // on purpose is not used: another test of this binary can bind it in the meantime, and then the connection is
    // answered by that test, not refused. The failure asserted is the same one: a transport failure, with no key.
    let server = serve(|_, _| {});
    let target = verify_target(
        &local_approval(server.port),
        &Resolve(|| panic!("an IP literal is not resolved")),
        &local_url(server.port),
        ProviderSource::Local,
    )
    .unwrap();
    let (_, outcome) = chat(&target, &limits());
    let failure = outcome.expect_err("the server closes without a reply");
    assert_eq!(failure, ChatFailure::Transport);
    assert!(!format!("{failure:?}").contains(KEY));
    assert_eq!(
        server.connections(),
        1,
        "the request reached the server once"
    );
}
