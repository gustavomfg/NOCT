//! The network policy of the provider adapters: which endpoints NOCT may call, which addresses an endpoint may resolve to,
//! and which replies are refused before their body is read. Port of `docs/provider-allowlist-design.md`, sections 6 to 8.
//!
//! The answers to the open questions of section 11 are the restrictive ones, and they are recorded in
//! `docs/status.md`: loopback is the only scope for a local source (Q2, which closes G1 in this port), the approval is
//! per host and port (Q3), beta 1 has no "Other" endpoints (Q6), and the list holds OpenRouter alone (Q5).
//!
//! This module does no I/O. The name lookup is a trait the app layer implements, so the rules run in tests without a
//! network, and the connection must use the addresses this module checks, never a second lookup.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::ProviderSource;

/// The host of the built-in OpenRouter endpoint. Its base URL is confirmed in the design document (section 5).
pub const OPENROUTER_HOST: &str = "openrouter.ai";
/// The HTTPS port of the built-in OpenRouter endpoint.
pub const OPENROUTER_PORT: u16 = 443;
/// The Chat Completions path under the OpenRouter base URL. The vendor page that section 5 cites does not show it, so
/// the real chat stays behind a switch that is off by default until the path is checked by hand.
pub const OPENROUTER_CHAT_PATH: &str = "/api/v1/chat/completions";

/// An approval: the host and port a source may call. Beta 1 has no approval store; the approvals are the built-in list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Approval {
    pub host: &'static str,
    pub port: u16,
    pub source: ProviderSource,
}

/// The approvals of beta 1. The list holds OpenRouter alone: the other presets of section 5 are not confirmed, and
/// "Other" endpoints are not part of beta 1.
pub const BUILTIN_APPROVALS: &[Approval] = &[Approval {
    host: OPENROUTER_HOST,
    port: OPENROUTER_PORT,
    source: ProviderSource::Remote,
}];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Https,
    Http,
}

/// A parsed endpoint. The host is lowercase, and an IPv6 literal is stored without its brackets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
    pub path: String,
}

/// Where an address falls in the classes the policy knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressClass {
    Public,
    Loopback,
    Refused,
}

/// Why the policy refuses a request. Each code is the one the interface may show, and none names an address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointError {
    /// The URL is not a plain endpoint: credentials, a query, a fragment, a bad host or port, or a scheme the source
    /// does not allow.
    InvalidEndpoint,
    /// No approval covers this host and port.
    HostNotApproved,
    /// The name could not be resolved.
    ResolutionFailed,
    /// The name resolved to no address.
    NoAddress,
    /// One of the addresses is outside the scope of the source. The whole request is refused.
    AddressNotAllowed,
    /// The reply is a redirect. Redirects are never followed.
    RedirectRefused,
}

impl EndpointError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidEndpoint => "invalid-endpoint",
            Self::HostNotApproved => "host-not-approved",
            Self::ResolutionFailed => "resolution-failed",
            Self::NoAddress => "no-address",
            Self::AddressNotAllowed => "address-not-allowed",
            Self::RedirectRefused => "redirect-refused",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidEndpoint => "O endereço do provedor não é válido.",
            Self::HostNotApproved => "Este host não está aprovado para o provedor.",
            Self::ResolutionFailed => "Não foi possível resolver o endereço do provedor.",
            Self::NoAddress => "O endereço do provedor não resolveu para nenhum IP.",
            Self::AddressNotAllowed => {
                "O endereço do provedor resolveu para um IP que não é permitido."
            }
            Self::RedirectRefused => {
                "O provedor respondeu com um redirecionamento, que o NOCT não segue."
            }
        }
    }
}

/// The lookup the policy needs. The app layer implements it with the operating system resolver; tests implement it
/// with fixed answers. Each call is one lookup, and the policy calls it once per request.
pub trait AddressResolver {
    fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<IpAddr>>;
}

/// Parses an `http` or `https` URL with no credentials, query or fragment. The host is an ASCII name without a trailing
/// dot, or an IP literal. The port defaults to the scheme's port.
pub fn parse_url(url: &str) -> Result<Endpoint, EndpointError> {
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (Scheme::Https, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (Scheme::Http, rest)
    } else {
        return Err(EndpointError::InvalidEndpoint);
    };
    if rest.contains(['?', '#', '@', '\\', ' '])
        || rest.bytes().any(|byte| !(0x21..=0x7e).contains(&byte))
    {
        return Err(EndpointError::InvalidEndpoint);
    }
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    let default_port = match scheme {
        Scheme::Https => 443,
        Scheme::Http => 80,
    };
    let (host, port) = split_authority(authority, default_port)?;
    Ok(Endpoint {
        scheme,
        host,
        port,
        path: path.to_string(),
    })
}

fn split_authority(authority: &str, default_port: u16) -> Result<(String, u16), EndpointError> {
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let end = bracketed.find(']').ok_or(EndpointError::InvalidEndpoint)?;
        let after = &bracketed[end + 1..];
        let port = match after {
            "" => default_port,
            _ => parse_port(
                after
                    .strip_prefix(':')
                    .ok_or(EndpointError::InvalidEndpoint)?,
            )?,
        };
        (&bracketed[..end], port)
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host, parse_port(port)?),
            None => (authority, default_port),
        }
    };
    let host = host.to_ascii_lowercase();
    if host.parse::<IpAddr>().is_ok() {
        return Ok((host, port));
    }
    if !is_host_name(&host) {
        return Err(EndpointError::InvalidEndpoint);
    }
    Ok((host, port))
}

fn parse_port(text: &str) -> Result<u16, EndpointError> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(EndpointError::InvalidEndpoint);
    }
    match text.parse::<u16>() {
        Ok(port) if port != 0 => Ok(port),
        _ => Err(EndpointError::InvalidEndpoint),
    }
}

fn is_host_name(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.starts_with(['.', '-'])
        && !host.ends_with(['.', '-'])
        && !host.contains("..")
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.')
}

/// Checks an endpoint against the approvals. The host and the port must match an approval, and the scheme must be
/// allowed for the approval's source: a remote source is HTTPS only, and a local source may use HTTP as well.
pub fn check_endpoint(
    endpoint: &Endpoint,
    source: ProviderSource,
) -> Result<&'static Approval, EndpointError> {
    let approval = BUILTIN_APPROVALS
        .iter()
        .find(|approval| approval.host == endpoint.host && approval.port == endpoint.port)
        .ok_or(EndpointError::HostNotApproved)?;
    if approval.source != source {
        return Err(EndpointError::HostNotApproved);
    }
    if endpoint.scheme == Scheme::Http && source == ProviderSource::Remote {
        return Err(EndpointError::InvalidEndpoint);
    }
    Ok(approval)
}

/// The scope a source allows. A remote source reaches public addresses only. A local source reaches loopback only,
/// which closes G1 of the design: a local endpoint no longer skips the address check, and no private network is reached.
pub fn allowed_class(source: ProviderSource) -> AddressClass {
    match source {
        ProviderSource::Remote => AddressClass::Public,
        ProviderSource::Local => AddressClass::Loopback,
    }
}

/// Classifies one address. Ranges that are not routable on the public internet are refused, including the ones that
/// reach cloud metadata (link-local) and the networks behind a NAT, and the transition ranges that can carry a private
/// IPv4 address. A mapped IPv6 address is classified by its IPv4 address.
pub fn classify(address: IpAddr) -> AddressClass {
    match address {
        IpAddr::V4(ip) => classify_v4(ip),
        IpAddr::V6(ip) => classify_v6(ip),
    }
}

fn classify_v4(ip: Ipv4Addr) -> AddressClass {
    if ip.is_loopback() {
        return AddressClass::Loopback;
    }
    const REFUSED: &[(Ipv4Addr, u32)] = &[
        (Ipv4Addr::new(0, 0, 0, 0), 8),
        (Ipv4Addr::new(10, 0, 0, 0), 8),
        (Ipv4Addr::new(100, 64, 0, 0), 10),
        (Ipv4Addr::new(169, 254, 0, 0), 16),
        (Ipv4Addr::new(172, 16, 0, 0), 12),
        (Ipv4Addr::new(192, 0, 0, 0), 24),
        (Ipv4Addr::new(192, 0, 2, 0), 24),
        (Ipv4Addr::new(192, 168, 0, 0), 16),
        (Ipv4Addr::new(198, 18, 0, 0), 15),
        (Ipv4Addr::new(198, 51, 100, 0), 24),
        (Ipv4Addr::new(203, 0, 113, 0), 24),
        (Ipv4Addr::new(224, 0, 0, 0), 4),
        (Ipv4Addr::new(240, 0, 0, 0), 4),
    ];
    if REFUSED
        .iter()
        .any(|(network, bits)| in_v4(ip, *network, *bits))
    {
        AddressClass::Refused
    } else {
        AddressClass::Public
    }
}

fn classify_v6(ip: Ipv6Addr) -> AddressClass {
    if ip == Ipv6Addr::LOCALHOST {
        return AddressClass::Loopback;
    }
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return classify_v4(mapped);
    }
    const REFUSED: &[(Ipv6Addr, u32)] = &[
        (Ipv6Addr::UNSPECIFIED, 128),
        (Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0), 96),
        (Ipv6Addr::new(0x64, 0xff9b, 0, 0, 0, 0, 0, 0), 96),
        (Ipv6Addr::new(0x100, 0, 0, 0, 0, 0, 0, 0), 64),
        (Ipv6Addr::new(0x2001, 0, 0, 0, 0, 0, 0, 0), 23),
        (Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32),
        (Ipv6Addr::new(0x2002, 0, 0, 0, 0, 0, 0, 0), 16),
        (Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 0), 7),
        (Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 10),
        (Ipv6Addr::new(0xff00, 0, 0, 0, 0, 0, 0, 0), 8),
    ];
    if REFUSED
        .iter()
        .any(|(network, bits)| in_v6(ip, *network, *bits))
    {
        return AddressClass::Refused;
    }
    if in_v6(ip, Ipv6Addr::new(0x2000, 0, 0, 0, 0, 0, 0, 0), 3) {
        AddressClass::Public
    } else {
        AddressClass::Refused
    }
}

fn in_v4(ip: Ipv4Addr, network: Ipv4Addr, bits: u32) -> bool {
    let mask = u32::MAX.checked_shl(32 - bits).unwrap_or(0);
    (u32::from(ip) & mask) == (u32::from(network) & mask)
}

fn in_v6(ip: Ipv6Addr, network: Ipv6Addr, bits: u32) -> bool {
    let mask = u128::MAX.checked_shl(128 - bits).unwrap_or(0);
    (u128::from(ip) & mask) == (u128::from(network) & mask)
}

/// Checks every address against the scope of the source. One address outside the scope refuses the whole set, and an
/// empty set is refused. The set is returned unchanged, so the connection can pin it.
pub fn check_addresses(addresses: &[IpAddr], source: ProviderSource) -> Result<(), EndpointError> {
    if addresses.is_empty() {
        return Err(EndpointError::NoAddress);
    }
    let allowed = allowed_class(source);
    if addresses
        .iter()
        .all(|address| classify(*address) == allowed)
    {
        Ok(())
    } else {
        Err(EndpointError::AddressNotAllowed)
    }
}

/// Resolves the endpoint once and checks the answer. An IP literal host is its own answer, and the resolver is not
/// called. The returned set is the snapshot the connection must use.
pub fn verify_addresses(
    resolver: &dyn AddressResolver,
    endpoint: &Endpoint,
    source: ProviderSource,
) -> Result<Vec<IpAddr>, EndpointError> {
    let addresses = match endpoint.host.parse::<IpAddr>() {
        Ok(literal) => vec![literal],
        Err(_) => resolver
            .resolve(&endpoint.host, endpoint.port)
            .map_err(|_| EndpointError::ResolutionFailed)?,
    };
    check_addresses(&addresses, source)?;
    Ok(addresses)
}

/// Refuses every redirect. A 3xx reply is a protocol error, and its location is never read or followed.
pub fn check_status(status: u16) -> Result<(), EndpointError> {
    if (300..=399).contains(&status) {
        Err(EndpointError::RedirectRefused)
    } else {
        Ok(())
    }
}
