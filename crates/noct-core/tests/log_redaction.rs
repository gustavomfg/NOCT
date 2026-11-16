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
        // A recorded divergence states what the port must give. Every other case is TypeScript's output.
        let expected = case
            .get("rustOutput")
            .unwrap_or(&case["output"])
            .as_str()
            .expect("output");
        assert_eq!(redact_text(input), expected, "input {input:?}");
    }
}

/// A divergence differs from TypeScript and says why. A case without one has neither field.
#[test]
fn every_intentional_divergence_is_recorded_with_its_reason() {
    let fixture = fixture();
    let mut recorded = 0;
    for case in fixture["text"].as_array().expect("text cases") {
        match case.get("rustOutput") {
            None => assert!(case.get("divergence").is_none(), "{}", case["input"]),
            Some(rust_output) => {
                recorded += 1;
                assert_ne!(rust_output, &case["output"], "{}", case["input"]);
                assert!(
                    case["divergence"]
                        .as_str()
                        .is_some_and(|reason| !reason.is_empty()),
                    "{}",
                    case["input"]
                );
            }
        }
    }
    assert_eq!(recorded, 4, "the private key divergences");
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

/// TypeScript redacts a private key header only when `KEY` follows the algorithm directly, so
/// `BEGIN RSA PRIVATE KEY` is not redacted there. The port redacts every algorithm. The divergence is
/// recorded in the fixture (`rustOutput`); this test pins the port's values.
#[test]
fn every_private_key_algorithm_header_is_redacted() {
    for header in [
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN DSA PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
        "-----BEGIN RSA KEY-----",
    ] {
        assert_eq!(redact_text(header), "[REDACTED-TOKEN]", "{header}");
    }
}

/// The header pattern is not wider than the private key headers.
#[test]
fn the_header_pattern_does_not_match_other_pem_labels() {
    for text in [
        "-----BEGIN KEY-----",
        "-----BEGIN PUBLIC KEY-----",
        "-----BEGIN CERTIFICATE-----",
    ] {
        assert_eq!(redact_text(text), text);
    }
}
