//! The log redaction, checked against the TypeScript logger. `fixtures/log-redaction.json` holds the
//! outputs of `redactLogText` and `redactLogValue` for fixed inputs, written by
//! `scripts/generate-log-redaction-fixture.mjs`; the port must reproduce every one of them.

use noct_core::log_redaction::{MAX_TEXT_UNITS, redact_text, redact_value};
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("fixtures/log-redaction.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("the fixture is JSON")
}

#[test]
fn the_text_redaction_reproduces_the_typescript_outputs() {
    let fixture = fixture();
    let cases = fixture["text"].as_array().expect("text cases");
    assert!(!cases.is_empty());
    for case in cases {
        let input = case["input"].as_str().expect("input");
        let expected = case["output"].as_str().expect("output");
        assert_eq!(redact_text(input), expected, "input {input:?}");
    }
}

#[test]
fn the_value_redaction_reproduces_the_typescript_outputs() {
    let fixture = fixture();
    let cases = fixture["value"].as_array().expect("value cases");
    assert!(!cases.is_empty());
    for case in cases {
        assert_eq!(
            redact_value(&case["input"]),
            case["output"],
            "input {}",
            case["input"]
        );
    }
}

#[test]
fn the_fixture_keeps_the_vectors_of_the_security_tests() {
    // tests/security.test.ts: the text vectors and the structured value.
    let fixture = fixture();
    let inputs: Vec<&str> = fixture["text"]
        .as_array()
        .expect("text cases")
        .iter()
        .map(|case| case["input"].as_str().expect("input"))
        .collect();
    for vector in [
        "Authorization: Bearer segredo-super-secreto",
        "authorization=Basic YWxhZGRpbjpvcGVuc2VzYW1l",
        "token=\"valor com espaços\"",
        "{\"api_key\":\"chave-json\",\"ok\":true}",
    ] {
        assert!(inputs.contains(&vector), "missing vector {vector:?}");
    }
    let value = json!({
        "authorization": "Bearer segredo",
        "prompt": "pedido privado",
        "content": "arquivo completo",
        "error": "falha com dados privados",
        "nested": { "password": "senha", "safe": "ok" }
    });
    assert_eq!(redact_value(&value), json!({ "nested": { "safe": "ok" } }));
}

#[test]
fn the_text_limit_counts_utf16_units() {
    let emoji = "😀".repeat(4_500);
    let redacted = redact_text(&emoji);
    assert_eq!(redacted.encode_utf16().count(), MAX_TEXT_UNITS);
    assert_eq!(redacted.chars().count(), MAX_TEXT_UNITS / 2);
}

/// Known difference. TypeScript cuts `"a" + 5000 emoji` at 8000 units and keeps the lone high
/// surrogate of the pair that straddles the limit. A Rust string cannot hold that, so the port cuts
/// before the pair and keeps 7999 units. This test pins the port's behavior, not the TypeScript one.
#[test]
fn a_pair_straddling_the_limit_is_dropped_whole() {
    let text = format!("a{}", "😀".repeat(5_000));
    let redacted = redact_text(&text);
    assert_eq!(redacted.encode_utf16().count(), MAX_TEXT_UNITS - 1);
    assert!(
        redacted
            .chars()
            .all(|character| character == 'a' || character == '😀')
    );
}

/// Known finding, reproduced from the TypeScript pattern: `SENSITIVE_KEYS` needs `KEY` right after the
/// algorithm, so an RSA or DSA private key header is not redacted, while a bare `PRIVATE KEY` header is.
/// The port keeps the TypeScript behavior until the owner decides the fix.
#[test]
fn the_private_key_header_follows_the_typescript_pattern() {
    assert_eq!(
        redact_text("-----BEGIN PRIVATE KEY-----"),
        "[REDACTED-TOKEN]"
    );
    assert_eq!(
        redact_text("-----BEGIN RSA PRIVATE KEY-----"),
        "-----BEGIN RSA PRIVATE KEY-----"
    );
}
