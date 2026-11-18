//! The updater of the shell, `tauri-plugin-updater`. The plugin is registered only when `plugins.updater` in
//! `tauri.conf.json` is complete and safe: a public key, at least one https endpoint, the signed-version check on, and
//! none of the flags that accept what an updater must refuse. The shipped section is complete (the release's public key
//! and its endpoint), so the plugin is registered at start. Registering it makes no request, and nothing in the shell
//! calls its check, download or install commands: the window is granted no updater permission. The private signing key
//! is not in this repository, and nothing in this module creates or reads one.

use serde_json::Value;
use tauri::{Builder, Runtime};

/// The section of `plugins` in `tauri.conf.json` that the updater reads.
pub const SECTION: &str = "updater";

/// The flags that accept what an updater must refuse. Each one must be off.
const UNSAFE_FLAGS: [&str; 4] = [
    "dangerousInsecureTransportProtocol",
    "dangerousAcceptInvalidCerts",
    "dangerousAcceptInvalidHostnames",
    "allowDowngrades",
];

/// Why the updater section cannot be used. Each one is a defect of the configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The section is not an object, or a field has the wrong type.
    Malformed,
    /// A public key without an endpoint, or an endpoint without a public key.
    Incomplete,
    /// An endpoint that is not https.
    InsecureEndpoint,
    /// A flag that must stay off is on. The flag is named.
    UnsafeFlag(&'static str),
    /// The signed-version check is not on. It is required once the updater is configured, because the endpoint
    /// response is fetched over TLS but is not itself signed.
    UnsignedVersion,
}

/// Whether the updater is registered for a section: `Ok(false)` for an absent section or one with neither a key nor an
/// endpoint, `Ok(true)` for a complete and safe section, and a refusal for anything else.
pub fn decide(section: Option<&Value>) -> Result<bool, Refusal> {
    let Some(section) = section else {
        return Ok(false);
    };
    let Value::Object(fields) = section else {
        return Err(Refusal::Malformed);
    };
    for flag in UNSAFE_FLAGS {
        match fields.get(flag) {
            None | Some(Value::Null) | Some(Value::Bool(false)) => {}
            Some(Value::Bool(true)) => return Err(Refusal::UnsafeFlag(flag)),
            Some(_) => return Err(Refusal::Malformed),
        }
    }
    let key = match fields.get("pubkey") {
        None | Some(Value::Null) => "",
        Some(Value::String(key)) => key.trim(),
        Some(_) => return Err(Refusal::Malformed),
    };
    let endpoints = match fields.get("endpoints") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) => list
            .iter()
            .map(|endpoint| endpoint.as_str().ok_or(Refusal::Malformed))
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(Refusal::Malformed),
    };
    match (key.is_empty(), endpoints.is_empty()) {
        (true, true) => return Ok(false),
        (false, false) => {}
        _ => return Err(Refusal::Incomplete),
    }
    if endpoints
        .iter()
        .any(|endpoint| !endpoint.to_ascii_lowercase().starts_with("https://"))
    {
        return Err(Refusal::InsecureEndpoint);
    }
    match fields.get("requireSignedVersion") {
        Some(Value::Bool(true)) => Ok(true),
        _ => Err(Refusal::UnsignedVersion),
    }
}

/// Adds the updater plugin to the builder when the configuration asks for it. `config` is the configuration the
/// context was generated from, so the registration and the shipped section are read from the same place.
pub fn with_plugin<R: Runtime>(
    builder: Builder<R>,
    config: &tauri::Config,
) -> Result<Builder<R>, Refusal> {
    if decide(config.plugins.0.get(SECTION))? {
        Ok(builder.plugin(tauri_plugin_updater::Builder::new().build()))
    } else {
        Ok(builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The section the shell ships with, read from `tauri.conf.json`.
    fn shipped() -> Value {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("the configuration is JSON");
        config["plugins"][SECTION].clone()
    }

    /// A complete and safe section. The key is a fixture, not a signing key.
    fn complete() -> Value {
        json!({
            "pubkey": "test-public-key-not-a-real-key",
            "endpoints": ["https://updates.example.invalid/latest.json"],
            "requireSignedVersion": true,
        })
    }

    #[test]
    fn the_shipped_section_is_complete_and_registers_the_updater() {
        assert_eq!(decide(Some(&shipped())), Ok(true));
    }

    #[test]
    fn a_section_with_neither_key_nor_endpoint_registers_nothing() {
        let empty = json!({ "pubkey": "", "endpoints": [], "requireSignedVersion": true });
        assert_eq!(decide(Some(&empty)), Ok(false));
    }

    #[test]
    fn an_absent_section_registers_nothing() {
        assert_eq!(decide(None), Ok(false));
    }

    #[test]
    fn a_complete_and_safe_section_registers_the_updater() {
        assert_eq!(decide(Some(&complete())), Ok(true));
    }

    #[test]
    fn a_key_without_an_endpoint_or_an_endpoint_without_a_key_is_incomplete() {
        let mut no_endpoint = complete();
        no_endpoint["endpoints"] = json!([]);
        assert_eq!(decide(Some(&no_endpoint)), Err(Refusal::Incomplete));

        let mut no_key = complete();
        no_key["pubkey"] = json!("   ");
        assert_eq!(decide(Some(&no_key)), Err(Refusal::Incomplete));
    }

    #[test]
    fn an_endpoint_that_is_not_https_is_refused() {
        let mut plain = complete();
        plain["endpoints"] = json!(["http://updates.example.invalid/latest.json"]);
        assert_eq!(decide(Some(&plain)), Err(Refusal::InsecureEndpoint));

        let mut mixed = complete();
        mixed["endpoints"] = json!([
            "https://updates.example.invalid/latest.json",
            "HTTP://updates.example.invalid/latest.json",
        ]);
        assert_eq!(decide(Some(&mixed)), Err(Refusal::InsecureEndpoint));
    }

    #[test]
    fn an_upper_case_https_endpoint_is_accepted() {
        let mut upper = complete();
        upper["endpoints"] = json!(["HTTPS://updates.example.invalid/latest.json"]);
        assert_eq!(decide(Some(&upper)), Ok(true));
    }

    #[test]
    fn each_flag_that_accepts_unsafe_updates_is_refused_by_name() {
        // The four names are written out here, not read from the list the check uses, so that removing one from the
        // list fails this test.
        for flag in [
            "dangerousInsecureTransportProtocol",
            "dangerousAcceptInvalidCerts",
            "dangerousAcceptInvalidHostnames",
            "allowDowngrades",
        ] {
            let mut section = complete();
            section[flag] = json!(true);
            assert_eq!(
                decide(Some(&section)),
                Err(Refusal::UnsafeFlag(flag)),
                "{flag}"
            );
        }
    }

    #[test]
    fn the_flags_may_be_off_or_absent() {
        let mut section = complete();
        for flag in UNSAFE_FLAGS {
            section[flag] = json!(false);
        }
        assert_eq!(decide(Some(&section)), Ok(true));
    }

    #[test]
    fn the_signed_version_check_is_required_once_configured() {
        let mut off = complete();
        off["requireSignedVersion"] = json!(false);
        assert_eq!(decide(Some(&off)), Err(Refusal::UnsignedVersion));

        let mut missing = complete();
        missing
            .as_object_mut()
            .expect("an object")
            .remove("requireSignedVersion");
        assert_eq!(decide(Some(&missing)), Err(Refusal::UnsignedVersion));
    }

    #[test]
    fn a_value_of_the_wrong_type_is_malformed() {
        assert_eq!(
            decide(Some(&json!("not an object"))),
            Err(Refusal::Malformed)
        );

        let mut key = complete();
        key["pubkey"] = json!(42);
        assert_eq!(decide(Some(&key)), Err(Refusal::Malformed));

        let mut endpoints = complete();
        endpoints["endpoints"] = json!([42]);
        assert_eq!(decide(Some(&endpoints)), Err(Refusal::Malformed));

        let mut flag = complete();
        flag["allowDowngrades"] = json!("yes");
        assert_eq!(decide(Some(&flag)), Err(Refusal::Malformed));
    }
}
