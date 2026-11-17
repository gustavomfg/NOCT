//! The request of `brain_create`, checked against `brainMemoryCreateSchema` in `shared/ipc/schemas.ts`. Every refusal
//! here is one the TypeScript parse refuses too, and every accepted value is one it accepts.

use noct_core::brain_memory::{BrainMemoryKind, BrainMemoryScope};
use noct_core::ipc::brain::BrainMemoryCreateRequest;
use serde_json::{Map, Value, json};

const CONVERSATION: &str = "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d";

fn with(content: Value) -> Value {
    json!({
        "conversationId": CONVERSATION,
        "kind": "fact",
        "scope": "workspace",
        "content": content,
    })
}

fn parse(value: Value) -> Result<BrainMemoryCreateRequest, serde_json::Error> {
    serde_json::from_value(value)
}

fn refused(value: Value) -> bool {
    parse(value).is_err()
}

#[test]
fn a_valid_request_is_read_with_its_content_trimmed() {
    let request = parse(with(json!("  Use vitest for the tests  "))).expect("the request is valid");
    assert_eq!(request.conversation_id, CONVERSATION);
    assert_eq!(request.kind, BrainMemoryKind::Fact);
    assert_eq!(request.scope, BrainMemoryScope::Workspace);
    assert_eq!(request.content, "Use vitest for the tests");
}

#[test]
fn every_kind_and_scope_of_the_schema_is_accepted() {
    for kind in ["fact", "decision", "preference", "constraint", "learning"] {
        for scope in ["workspace", "conversation"] {
            let value = json!({
                "conversationId": CONVERSATION,
                "kind": kind,
                "scope": scope,
                "content": "A remembered note",
            });
            assert!(parse(value).is_ok(), "{kind} / {scope} is accepted");
        }
    }
}

#[test]
fn a_kind_or_scope_outside_the_schema_is_refused() {
    let mut value = with(json!("A note"));
    value["kind"] = json!("opinion");
    assert!(refused(value));
    let mut value = with(json!("A note"));
    value["scope"] = json!("global");
    assert!(refused(value));
}

#[test]
fn a_conversation_id_that_is_not_a_uuid_is_refused() {
    for id in [
        "not-a-uuid",
        "3f0c7a52-8d1e-0b6a-9c2f-5e7d8a9b0c1d",
        "3f0c7a52-8d1e-4b6a-1c2f-5e7d8a9b0c1d",
    ] {
        let mut value = with(json!("A note"));
        value["conversationId"] = json!(id);
        assert!(refused(value), "{id} is refused");
    }
}

#[test]
fn content_that_is_empty_after_trimming_is_refused() {
    assert!(refused(with(json!(""))));
    assert!(refused(with(json!("   \t\n  "))));
}

#[test]
fn content_is_measured_in_utf16_units_up_to_8000() {
    assert!(parse(with(json!("a".repeat(8_000)))).is_ok());
    assert!(refused(with(json!("a".repeat(8_001)))));
    // An emoji is two UTF-16 units, so 4,000 of them fill the limit and one more does not fit.
    assert!(parse(with(json!("😀".repeat(4_000)))).is_ok());
    assert!(refused(with(json!("😀".repeat(4_001)))));
}

#[test]
fn content_that_looks_like_a_credential_is_refused() {
    for content in [
        "token=abcdefghijklmnop",
        "password: correct-horse-battery",
        "api_key = 0123456789abcdef",
        "sk-abcdefghijklmnop",
    ] {
        assert!(refused(with(json!(content))), "{content} is refused");
    }
    assert!(
        parse(with(json!("The token is kept in the OS vault"))).is_ok(),
        "a sentence that names a credential without a value is accepted"
    );
}

#[test]
fn an_unknown_missing_or_mistyped_field_is_refused() {
    let mut extra = with(json!("A note"));
    extra["theme"] = json!("dark");
    assert!(refused(extra), "an unknown field");

    let mut missing = with(json!("A note"));
    missing.as_object_mut().expect("an object").remove("scope");
    assert!(refused(missing), "a missing field");

    assert!(refused(with(json!(42))), "content that is not a string");
    assert!(refused(json!("A note")), "a request that is not an object");
    assert!(refused(Value::Object(Map::new())), "an empty object");
}
