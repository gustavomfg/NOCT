//! The request of `suggestions_status`, checked against `suggestionStatusSchema` in `shared/ipc/schemas.ts`.

use noct_core::ipc::suggestions::SuggestionStatusRequest;
use noct_core::suggestions::SuggestionStatus;
use serde_json::json;

const CONVERSATION: &str = "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d";
const SUGGESTION: &str = "7a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";

fn request(status: &str) -> serde_json::Value {
    json!({ "conversationId": CONVERSATION, "suggestionId": SUGGESTION, "status": status })
}

#[test]
fn a_status_is_read_in_the_typescript_spelling_and_the_result_is_kept_as_it_is() {
    let with = serde_json::from_value::<SuggestionStatusRequest>(json!({
        "conversationId": CONVERSATION,
        "suggestionId": SUGGESTION,
        "status": "in-analysis",
        "result": "  kept  ",
    }))
    .expect("the request is valid");
    assert_eq!(with.status, SuggestionStatus::InAnalysis);
    assert_eq!(with.result.as_deref(), Some("  kept  "));

    let without = serde_json::from_value::<SuggestionStatusRequest>(request("accepted"))
        .expect("the request is valid");
    assert_eq!(without.status, SuggestionStatus::Accepted);
    assert_eq!(without.result, None);
}

#[test]
fn a_status_outside_the_list_is_refused() {
    for status in ["done", "In-Analysis", "in_analysis", "", " new"] {
        assert!(
            serde_json::from_value::<SuggestionStatusRequest>(request(status)).is_err(),
            "{status:?} is refused"
        );
    }
}

#[test]
fn a_null_result_is_refused_and_an_absent_one_is_none() {
    let refused = serde_json::from_value::<SuggestionStatusRequest>(json!({
        "conversationId": CONVERSATION,
        "suggestionId": SUGGESTION,
        "status": "accepted",
        "result": null,
    }));
    assert!(refused.is_err());
}

#[test]
fn an_unknown_or_missing_field_is_refused() {
    let mut extra = request("accepted");
    extra["priority"] = json!(1);
    assert!(serde_json::from_value::<SuggestionStatusRequest>(extra).is_err());

    for missing in ["conversationId", "suggestionId", "status"] {
        let mut value = request("accepted");
        value.as_object_mut().expect("an object").remove(missing);
        assert!(
            serde_json::from_value::<SuggestionStatusRequest>(value).is_err(),
            "without {missing} the request is refused"
        );
    }
}

#[test]
fn the_result_has_at_most_twenty_thousand_utf16_units() {
    let result_of = |text: String| {
        serde_json::from_value::<SuggestionStatusRequest>(json!({
            "conversationId": CONVERSATION,
            "suggestionId": SUGGESTION,
            "status": "resolved",
            "result": text,
        }))
    };
    assert!(
        result_of("a".repeat(20_000)).is_ok(),
        "the edge is accepted"
    );
    assert!(
        result_of("a".repeat(20_001)).is_err(),
        "one unit over is refused"
    );
    // An astral character counts as two UTF-16 units, as zod counts them.
    assert!(result_of("😀".repeat(10_000)).is_ok());
    assert!(result_of("😀".repeat(10_001)).is_err());
}

#[test]
fn the_ids_must_be_uuids() {
    let mut value = request("accepted");
    value["suggestionId"] = json!("not-a-uuid");
    assert!(serde_json::from_value::<SuggestionStatusRequest>(value).is_err());
}
