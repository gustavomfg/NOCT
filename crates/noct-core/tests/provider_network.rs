//! The network policy of the provider adapters, without a network. The resolver is a fake that counts its calls, so the
//! tests can show that a request resolves once and that the connection would use the first answer only.

use std::cell::{Cell, RefCell};
use std::io;
use std::net::IpAddr;

use noct_core::ai::ProviderSource;
use noct_core::ai::network::{
    AddressClass, AddressResolver, BUILTIN_APPROVALS, EndpointError, OPENROUTER_CHAT_PATH,
    OPENROUTER_HOST, OPENROUTER_PORT, Scheme, check_addresses, check_endpoint, check_status,
    classify, parse_url, verify_addresses,
};
use noct_core::limits::provider_network as limits;

fn ip(text: &str) -> IpAddr {
    text.parse().expect("the test address is valid")
}

/// Answers each lookup with the next list of addresses, and counts the lookups.
struct Scripted {
    answers: RefCell<Vec<io::Result<Vec<IpAddr>>>>,
    calls: Cell<u32>,
}

impl Scripted {
    fn new(answers: Vec<io::Result<Vec<IpAddr>>>) -> Self {
        Self {
            answers: RefCell::new(answers),
            calls: Cell::new(0),
        }
    }
}

impl AddressResolver for Scripted {
    fn resolve(&self, _host: &str, _port: u16) -> io::Result<Vec<IpAddr>> {
        self.calls.set(self.calls.get() + 1);
        let mut answers = self.answers.borrow_mut();
        if answers.is_empty() {
            return Err(io::Error::other("no more answers"));
        }
        answers.remove(0)
    }
}

#[test]
fn the_builtin_endpoint_is_openrouter_over_https_on_443() {
    assert_eq!(OPENROUTER_HOST, "openrouter.ai");
    assert_eq!(OPENROUTER_PORT, 443);
    assert_eq!(OPENROUTER_CHAT_PATH, "/api/v1/chat/completions");
    assert_eq!(BUILTIN_APPROVALS.len(), 1, "beta 1 approves one host");
    let endpoint =
        parse_url("https://openrouter.ai/api/v1/chat/completions").expect("the endpoint parses");
    assert_eq!(endpoint.scheme, Scheme::Https);
    assert_eq!(endpoint.host, "openrouter.ai");
    assert_eq!(endpoint.port, 443);
    assert_eq!(endpoint.path, "/api/v1/chat/completions");
    assert!(check_endpoint(&endpoint, ProviderSource::Remote).is_ok());
}

#[test]
fn a_url_parses_with_its_default_port_and_a_lowercase_host() {
    let endpoint = parse_url("https://OpenRouter.AI").expect("the host is valid");
    assert_eq!(endpoint.host, "openrouter.ai");
    assert_eq!(endpoint.port, 443);
    assert_eq!(endpoint.path, "/");

    let local = parse_url("http://127.0.0.1:1234/v1").expect("the local URL parses");
    assert_eq!(local.scheme, Scheme::Http);
    assert_eq!(local.port, 1234);
    assert_eq!(parse_url("http://localhost").unwrap().port, 80);
}

#[test]
fn an_ipv6_literal_parses_without_its_brackets() {
    let endpoint = parse_url("http://[::1]:8080/v1").expect("the literal parses");
    assert_eq!(endpoint.host, "::1");
    assert_eq!(endpoint.port, 8080);
}

#[test]
fn a_url_with_credentials_a_query_a_fragment_or_a_bad_host_or_port_is_refused() {
    let refused = [
        "ftp://openrouter.ai",
        "openrouter.ai/api",
        "https://user:secret@openrouter.ai",
        "https://openrouter.ai?key=1",
        "https://openrouter.ai#frag",
        "https://openrouter.ai\\evil",
        "https://open router.ai",
        "https://openrouter.ai.",
        "https://-openrouter.ai",
        "https://openrouter..ai",
        "https://openrouter.ai:0",
        "https://openrouter.ai:65536",
        "https://openrouter.ai:",
        "https://openrouter.ai:abc",
        "https://ex\u{00e4}mple.com",
        "https://",
        "https://[::1",
        "https://::1",
    ];
    for url in refused {
        assert_eq!(
            parse_url(url),
            Err(EndpointError::InvalidEndpoint),
            "{url} is refused"
        );
    }
}

#[test]
fn only_the_approved_host_and_port_are_called() {
    let other_port = parse_url("https://openrouter.ai:8443").unwrap();
    assert_eq!(
        check_endpoint(&other_port, ProviderSource::Remote),
        Err(EndpointError::HostNotApproved),
        "a remote source on another port is refused"
    );
    let other_host = parse_url("https://api.example.com").unwrap();
    assert_eq!(
        check_endpoint(&other_host, ProviderSource::Remote),
        Err(EndpointError::HostNotApproved)
    );
    let regional = parse_url("https://eu.openrouter.ai").unwrap();
    assert_eq!(
        check_endpoint(&regional, ProviderSource::Remote),
        Err(EndpointError::HostNotApproved),
        "a regional host is not in the list"
    );
    let wrong_source = parse_url("https://openrouter.ai").unwrap();
    assert_eq!(
        check_endpoint(&wrong_source, ProviderSource::Local),
        Err(EndpointError::HostNotApproved),
        "the approval names its source"
    );
}

#[test]
fn a_remote_source_is_https_only() {
    let plain = parse_url("http://openrouter.ai").unwrap();
    assert_eq!(
        check_endpoint(&plain, ProviderSource::Remote),
        Err(EndpointError::HostNotApproved),
        "port 80 is not approved for the host, so the scheme is not reached"
    );
    let mut downgraded = parse_url("https://openrouter.ai").unwrap();
    downgraded.scheme = Scheme::Http;
    assert_eq!(
        check_endpoint(&downgraded, ProviderSource::Remote),
        Err(EndpointError::InvalidEndpoint)
    );
}

#[test]
fn the_address_classes_of_ipv4_follow_the_routable_ranges() {
    let cases = [
        ("127.0.0.1", AddressClass::Loopback),
        ("8.8.8.8", AddressClass::Public),
        ("1.1.1.1", AddressClass::Public),
        ("0.0.0.0", AddressClass::Refused),
        ("10.0.0.5", AddressClass::Refused),
        ("172.16.0.1", AddressClass::Refused),
        ("172.31.255.255", AddressClass::Refused),
        ("172.32.0.1", AddressClass::Public),
        ("192.168.1.1", AddressClass::Refused),
        ("169.254.169.254", AddressClass::Refused),
        ("100.64.0.1", AddressClass::Refused),
        ("100.127.255.255", AddressClass::Refused),
        ("192.0.0.8", AddressClass::Refused),
        ("192.0.2.1", AddressClass::Refused),
        ("198.18.0.1", AddressClass::Refused),
        ("198.51.100.7", AddressClass::Refused),
        ("203.0.113.7", AddressClass::Refused),
        ("224.0.0.1", AddressClass::Refused),
        ("240.0.0.1", AddressClass::Refused),
        ("255.255.255.255", AddressClass::Refused),
    ];
    for (text, expected) in cases {
        assert_eq!(classify(ip(text)), expected, "{text}");
    }
}

#[test]
fn the_address_classes_of_ipv6_follow_the_routable_ranges() {
    let cases = [
        ("::1", AddressClass::Loopback),
        ("::", AddressClass::Refused),
        ("2606:4700:4700::1111", AddressClass::Public),
        ("2001:4860:4860::8888", AddressClass::Public),
        ("fe80::1", AddressClass::Refused),
        ("fc00::1", AddressClass::Refused),
        ("fd12:3456::1", AddressClass::Refused),
        ("ff02::1", AddressClass::Refused),
        ("2001:db8::1", AddressClass::Refused),
        ("2001::1", AddressClass::Refused),
        ("2002:c0a8:0101::1", AddressClass::Refused),
        ("64:ff9b::8.8.8.8", AddressClass::Refused),
        ("100::1", AddressClass::Refused),
        ("::ffff:10.0.0.1", AddressClass::Refused),
        ("::ffff:8.8.8.8", AddressClass::Public),
        ("::ffff:127.0.0.1", AddressClass::Loopback),
    ];
    for (text, expected) in cases {
        assert_eq!(classify(ip(text)), expected, "{text}");
    }
}

#[test]
fn a_remote_source_accepts_public_addresses_only_and_one_bad_address_refuses_the_set() {
    assert_eq!(
        check_addresses(&[ip("8.8.8.8"), ip("1.1.1.1")], ProviderSource::Remote),
        Ok(())
    );
    assert_eq!(
        check_addresses(&[ip("8.8.8.8"), ip("10.0.0.5")], ProviderSource::Remote),
        Err(EndpointError::AddressNotAllowed),
        "a mixed answer is refused as a whole"
    );
    assert_eq!(
        check_addresses(&[ip("127.0.0.1")], ProviderSource::Remote),
        Err(EndpointError::AddressNotAllowed)
    );
    assert_eq!(
        check_addresses(&[], ProviderSource::Remote),
        Err(EndpointError::NoAddress)
    );
}

#[test]
fn a_local_source_reaches_loopback_only_which_closes_gap_g1() {
    assert_eq!(
        check_addresses(&[ip("127.0.0.1")], ProviderSource::Local),
        Ok(())
    );
    assert_eq!(check_addresses(&[ip("::1")], ProviderSource::Local), Ok(()));
    assert_eq!(
        check_addresses(&[ip("8.8.8.8")], ProviderSource::Local),
        Err(EndpointError::AddressNotAllowed),
        "a local endpoint that resolves to a public address is refused"
    );
    assert_eq!(
        check_addresses(&[ip("10.0.0.5")], ProviderSource::Local),
        Err(EndpointError::AddressNotAllowed),
        "a private address is refused for a local source"
    );
    assert_eq!(
        check_addresses(&[ip("169.254.169.254")], ProviderSource::Local),
        Err(EndpointError::AddressNotAllowed),
        "the metadata address is refused for a local source"
    );
}

#[test]
fn a_name_is_resolved_once_and_the_first_answer_is_the_snapshot() {
    let endpoint = parse_url("https://openrouter.ai").unwrap();
    // The second answer is private. A second lookup would find it; the policy never asks for one.
    let resolver = Scripted::new(vec![Ok(vec![ip("8.8.8.8")]), Ok(vec![ip("10.0.0.5")])]);

    let snapshot = verify_addresses(&resolver, &endpoint, ProviderSource::Remote)
        .expect("the answer is public");

    assert_eq!(snapshot, vec![ip("8.8.8.8")]);
    assert_eq!(resolver.calls.get(), 1, "one lookup per request");
}

#[test]
fn a_mixed_answer_is_refused_after_a_single_lookup() {
    let endpoint = parse_url("https://openrouter.ai").unwrap();
    let resolver = Scripted::new(vec![Ok(vec![ip("8.8.8.8"), ip("192.168.0.9")])]);

    assert_eq!(
        verify_addresses(&resolver, &endpoint, ProviderSource::Remote),
        Err(EndpointError::AddressNotAllowed)
    );
    assert_eq!(resolver.calls.get(), 1);
}

#[test]
fn an_ip_literal_is_its_own_answer_and_the_resolver_is_not_called() {
    let endpoint = parse_url("http://127.0.0.1:1234").unwrap();
    let resolver = Scripted::new(vec![]);

    let snapshot =
        verify_addresses(&resolver, &endpoint, ProviderSource::Local).expect("loopback is allowed");

    assert_eq!(snapshot, vec![ip("127.0.0.1")]);
    assert_eq!(resolver.calls.get(), 0);
}

#[test]
fn a_failed_lookup_is_reported_without_detail() {
    let endpoint = parse_url("https://openrouter.ai").unwrap();
    let resolver = Scripted::new(vec![Err(io::Error::other("dns refused at 10.0.0.1"))]);

    assert_eq!(
        verify_addresses(&resolver, &endpoint, ProviderSource::Remote),
        Err(EndpointError::ResolutionFailed)
    );
}

#[test]
fn an_empty_answer_is_refused() {
    let endpoint = parse_url("https://openrouter.ai").unwrap();
    let resolver = Scripted::new(vec![Ok(vec![])]);

    assert_eq!(
        verify_addresses(&resolver, &endpoint, ProviderSource::Remote),
        Err(EndpointError::NoAddress)
    );
}

#[test]
fn every_redirect_is_refused_and_the_other_statuses_reach_the_adapter() {
    for status in [300, 301, 302, 303, 304, 307, 308, 399] {
        assert_eq!(
            check_status(status),
            Err(EndpointError::RedirectRefused),
            "{status}"
        );
    }
    for status in [200, 204, 400, 401, 429, 500] {
        assert_eq!(check_status(status), Ok(()), "{status}");
    }
}

#[test]
fn the_refusal_codes_are_stable_and_name_no_address() {
    let all = [
        EndpointError::InvalidEndpoint,
        EndpointError::HostNotApproved,
        EndpointError::ResolutionFailed,
        EndpointError::NoAddress,
        EndpointError::AddressNotAllowed,
        EndpointError::RedirectRefused,
    ];
    let codes: Vec<&str> = all.iter().map(|error| error.code()).collect();
    assert_eq!(
        codes,
        [
            "invalid-endpoint",
            "host-not-approved",
            "resolution-failed",
            "no-address",
            "address-not-allowed",
            "redirect-refused",
        ]
    );
    for error in all {
        assert!(
            !error
                .message()
                .chars()
                .any(|character| character.is_ascii_digit()),
            "{} names no number, so no address: {}",
            error.code(),
            error.message()
        );
    }
}

#[test]
fn the_limits_bound_connection_time_reply_size() {
    assert_eq!(limits::CONNECT_TIMEOUT_MS, 10_000);
    assert_eq!(limits::REQUEST_TIMEOUT_MS, 120_000);
    assert_eq!(limits::MAXIMUM_RESPONSE_BYTES, 2 * 1024 * 1024);
}
