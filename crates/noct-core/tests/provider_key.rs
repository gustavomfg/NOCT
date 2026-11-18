//! The provider key contract: what is accepted, what is refused, and that the key cannot leave through a debug print or
//! through the status that the interface reads.

use noct_core::ai::provider_key::{ProviderKeyInput, ProviderKeyProvider, ProviderKeyStatus};

const KEY: &str = "sk-or-v1-0123456789abcdef";

fn input(key: &str) -> ProviderKeyInput {
    ProviderKeyInput {
        provider: ProviderKeyProvider::OpenRouter,
        key: key.to_string(),
    }
}

#[test]
fn a_visible_key_is_accepted_without_its_surrounding_spaces() {
    let valid = input(&format!("  {KEY}  "))
        .validated()
        .expect("the key is valid");
    assert_eq!(valid.key, KEY);
}

#[test]
fn a_key_outside_the_length_bounds_is_refused() {
    assert!(input("short").validated().is_err(), "under 8 characters");
    assert!(
        input(&"a".repeat(513)).validated().is_err(),
        "over 512 characters"
    );
    assert!(input("        ").validated().is_err(), "only spaces");
    assert!(
        input(&"a".repeat(512)).validated().is_ok(),
        "512 characters is the maximum"
    );
}

#[test]
fn a_key_with_control_characters_or_inner_spaces_is_refused() {
    // A key goes into an HTTP header: a line break or a space inside it would change the header.
    assert!(input("sk-or-v1-abc\r\nX-Injected: 1").validated().is_err());
    assert!(input("sk-or-v1-abc def").validated().is_err());
    assert!(
        input("sk-or-v1-abc\u{00e9}").validated().is_err(),
        "non-ASCII is refused"
    );
}

#[test]
fn the_refusal_names_the_field_and_never_the_value() {
    let error = input("bad key")
        .validated()
        .expect_err("a space is refused")
        .to_string();
    assert!(error.contains("key"), "{error}");
    assert!(!error.contains("bad"), "the value must not appear: {error}");
}

#[test]
fn debug_output_never_contains_the_key() {
    let printed = format!("{:?}", input(KEY));
    assert!(!printed.contains(KEY), "{printed}");
    assert!(printed.contains("<redacted>"), "{printed}");
}

#[test]
fn the_status_serializes_without_any_key_field() {
    let status = ProviderKeyStatus {
        provider: ProviderKeyProvider::OpenRouter,
        configured: true,
    };
    let json = serde_json::to_string(&status).expect("the status serializes");
    assert_eq!(json, r#"{"provider":"openrouter","configured":true}"#);
    assert!(!json.contains("key"));
}

#[test]
fn input_rejects_unknown_fields_and_other_providers() {
    let unknown = r#"{"provider":"openrouter","key":"sk-or-v1-0123456789","extra":1}"#;
    assert!(serde_json::from_str::<ProviderKeyInput>(unknown).is_err());
    let other = r#"{"provider":"groq","key":"sk-or-v1-0123456789"}"#;
    assert!(serde_json::from_str::<ProviderKeyInput>(other).is_err());
}
