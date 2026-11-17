//! The request of `ai_save_assistant`, checked against `saveAssistantSchema` in `shared/ipc/schemas.ts`.

use noct_core::ipc::ai::SaveAssistantRequest;
use serde_json::{Value, json};

const CONVERSATION: &str = "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d";

fn request(extra: Value) -> Value {
    let mut value = json!({ "conversationId": CONVERSATION, "content": "  Resposta.  " });
    for (key, field) in extra.as_object().expect("an object") {
        value[key] = field.clone();
    }
    value
}

#[test]
fn the_content_is_kept_as_it_is_and_the_metadata_is_optional() {
    let without = serde_json::from_value::<SaveAssistantRequest>(request(json!({})))
        .expect("the request is valid");
    assert_eq!(without.content, "  Resposta.  ", "the reply is not trimmed");
    assert_eq!(without.metadata, None);

    let explicit_null =
        serde_json::from_value::<SaveAssistantRequest>(request(json!({ "metadata": null })))
            .expect("an explicit null is accepted");
    assert_eq!(
        explicit_null.metadata, None,
        "a null is the same as no metadata"
    );

    let with = serde_json::from_value::<SaveAssistantRequest>(request(json!({
        "metadata": { "files": [{ "path": "a.md" }] }
    })))
    .expect("the request is valid");
    assert_eq!(
        with.metadata,
        Some(json!({ "files": [{ "path": "a.md" }] }))
    );
}

#[test]
fn an_unknown_field_is_ignored_because_the_schema_is_not_strict() {
    let parsed = serde_json::from_value::<SaveAssistantRequest>(request(json!({ "extra": true })));
    assert!(
        parsed.is_ok(),
        "zod strips an unknown key, so it is not refused"
    );
}

#[test]
fn metadata_that_is_too_deep_or_too_large_is_refused() {
    let mut deep = json!(1);
    for _ in 0..40 {
        deep = json!([deep]);
    }
    assert!(
        serde_json::from_value::<SaveAssistantRequest>(request(json!({ "metadata": deep })))
            .is_err()
    );

    let large = json!({ "text": "a".repeat(500_000) });
    assert!(
        serde_json::from_value::<SaveAssistantRequest>(request(json!({ "metadata": large })))
            .is_err()
    );
}

#[test]
fn the_content_has_at_most_two_million_utf16_units_and_the_ids_and_fields_are_checked() {
    let at_the_edge = serde_json::from_value::<SaveAssistantRequest>(request(json!({
        "content": "a".repeat(2_000_000)
    })));
    assert!(at_the_edge.is_ok());
    let over = serde_json::from_value::<SaveAssistantRequest>(request(json!({
        "content": "a".repeat(2_000_001)
    })));
    assert!(over.is_err());

    assert!(
        serde_json::from_value::<SaveAssistantRequest>(request(json!({ "content": null })))
            .is_err()
    );
    assert!(
        serde_json::from_value::<SaveAssistantRequest>(request(json!({ "conversationId": "x" })))
            .is_err()
    );
    let mut without_content = request(json!({}));
    without_content
        .as_object_mut()
        .expect("an object")
        .remove("content");
    assert!(serde_json::from_value::<SaveAssistantRequest>(without_content).is_err());
}
