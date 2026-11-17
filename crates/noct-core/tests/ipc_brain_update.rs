//! The request of `brain_update`, checked against `brainMemoryUpdateSchema` in `shared/ipc/schemas.ts`. A request must
//! change something, and each field that is present is checked the way the schema checks it.

use noct_core::brain_memory::{BrainMemoryKind, BrainMemoryScope, BrainMemoryStatus};
use noct_core::ipc::brain::BrainMemoryUpdateRequest;
use serde_json::{Value, json};

const CONVERSATION: &str = "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d";
const MEMORY: &str = "7a1d2c3e-4f50-4a61-8b72-9c83d4e5f607";

fn with(change: Value) -> Value {
    let mut value = json!({ "conversationId": CONVERSATION, "memoryId": MEMORY });
    for (key, field) in change.as_object().expect("a change is an object") {
        value[key] = field.clone();
    }
    value
}

fn parse(value: Value) -> Result<BrainMemoryUpdateRequest, serde_json::Error> {
    serde_json::from_value(value)
}

fn refused(value: Value) -> bool {
    parse(value).is_err()
}

#[test]
fn a_single_change_is_read_and_the_other_fields_stay_absent() {
    let request = parse(with(json!({ "status": "active" }))).expect("the request is valid");
    assert_eq!(request.status, Some(BrainMemoryStatus::Active));
    assert_eq!(request.content, None);
    assert_eq!(request.kind, None);
    assert_eq!(request.scope, None);
    assert_eq!(request.confidence, None);
}

#[test]
fn a_request_that_changes_nothing_is_refused() {
    assert!(refused(
        json!({ "conversationId": CONVERSATION, "memoryId": MEMORY })
    ));
}

#[test]
fn content_follows_the_create_rules() {
    let request = parse(with(json!({ "content": "  A changed note  " }))).expect("valid");
    assert_eq!(request.content.as_deref(), Some("A changed note"));
    assert!(
        refused(with(json!({ "content": "   " }))),
        "empty after trimming"
    );
    assert!(
        refused(with(json!({ "content": "token=abcdefghijklmnop" }))),
        "a credential"
    );
    assert!(
        refused(with(json!({ "content": "a".repeat(8_001) }))),
        "too long"
    );
}

#[test]
fn confidence_is_an_integer_from_0_to_100() {
    assert_eq!(
        parse(with(json!({ "confidence": 0 })))
            .expect("valid")
            .confidence,
        Some(0)
    );
    assert_eq!(
        parse(with(json!({ "confidence": 100 })))
            .expect("valid")
            .confidence,
        Some(100)
    );
    // A whole number written with a decimal point is still an integer in JavaScript, so the schema accepts it.
    assert_eq!(
        parse(with(json!({ "confidence": 80.0 })))
            .expect("valid")
            .confidence,
        Some(80)
    );
    assert!(refused(with(json!({ "confidence": 101 }))));
    assert!(refused(with(json!({ "confidence": -1 }))));
    assert!(refused(with(json!({ "confidence": 50.5 }))));
    assert!(refused(with(json!({ "confidence": "70" }))));
}

#[test]
fn kind_scope_and_status_must_be_in_the_schema() {
    let request = parse(with(
        json!({ "kind": "learning", "scope": "conversation", "status": "archived" }),
    ))
    .expect("valid");
    assert_eq!(request.kind, Some(BrainMemoryKind::Learning));
    assert_eq!(request.scope, Some(BrainMemoryScope::Conversation));
    assert_eq!(request.status, Some(BrainMemoryStatus::Archived));
    assert!(refused(with(json!({ "status": "deleted" }))));
    assert!(refused(with(json!({ "kind": "opinion" }))));
    assert!(refused(with(json!({ "scope": "global" }))));
}

#[test]
fn ids_are_required_uuids_and_unknown_fields_are_refused() {
    assert!(
        refused(json!({ "conversationId": CONVERSATION, "status": "active" })),
        "no memory id"
    );
    assert!(
        refused(json!({ "memoryId": MEMORY, "status": "active" })),
        "no conversation id"
    );
    assert!(refused(with(
        json!({ "memoryId": "not-a-uuid", "status": "active" })
    )));
    assert!(refused(with(
        json!({ "status": "active", "theme": "dark" })
    )));
}
