//! The request and the page of `conversations_list`. The limit is required and bounded, the cursor is
//! optional and has two fields, and anything else is refused before the store is read.

use noct_core::ipc::conversations::{
    CURSOR_CHARACTERS_MAX, ConversationCursor, ConversationPage, ConversationPageRequest,
    PAGE_LIMIT_MAX,
};
use serde_json::{Value, json};

fn request(value: Value) -> Result<ConversationPageRequest, serde_json::Error> {
    serde_json::from_value(value)
}

#[test]
fn the_first_page_takes_a_limit_and_no_cursor() {
    let parsed = request(json!({ "limit": 50 })).expect("a limit alone is a request");

    assert_eq!(parsed.limit, 50);
    assert_eq!(parsed.cursor, None);
}

#[test]
fn the_limit_bounds_are_accepted() {
    assert_eq!(request(json!({ "limit": 1 })).unwrap().limit, 1);
    assert_eq!(
        request(json!({ "limit": PAGE_LIMIT_MAX })).unwrap().limit,
        500
    );
}

#[test]
fn a_later_page_takes_the_cursor_of_the_previous_one() {
    let parsed = request(json!({
        "limit": 2,
        "cursor": { "updatedAt": "2026-01-02T00:00:00.000Z", "id": "conversation-1" }
    }))
    .expect("a cursor is accepted");

    assert_eq!(
        parsed.cursor,
        Some(ConversationCursor {
            updated_at: "2026-01-02T00:00:00.000Z".to_string(),
            id: "conversation-1".to_string(),
        })
    );
}

/// Every value below is refused, and the refusal names the field that is wrong.
#[test]
fn the_limit_is_required_and_bounded() {
    let refused = [
        json!({}),
        json!({ "limit": 0 }),
        json!({ "limit": 501 }),
        json!({ "limit": 1.5 }),
        json!({ "limit": 2.0 }),
        json!({ "limit": "10" }),
        json!({ "limit": -1 }),
        json!({ "limit": null }),
        json!({ "limit": true }),
    ];
    for value in refused {
        assert!(request(value.clone()).is_err(), "{value} is refused");
    }
}

#[test]
fn unknown_fields_and_bad_cursors_are_refused() {
    let refused = [
        json!({ "limit": 5, "offset": 10 }),
        json!({ "limit": 5, "cursor": null }),
        json!({ "limit": 5, "cursor": "conversation-1" }),
        json!({ "limit": 5, "cursor": { "updatedAt": "2026-01-02T00:00:00.000Z" } }),
        json!({ "limit": 5, "cursor": { "id": "conversation-1" } }),
        json!({ "limit": 5, "cursor": { "updatedAt": "", "id": "conversation-1" } }),
        json!({ "limit": 5, "cursor": { "updatedAt": "2026-01-02T00:00:00.000Z", "id": "x".repeat(CURSOR_CHARACTERS_MAX + 1) } }),
        json!({ "limit": 5, "cursor": { "updatedAt": "2026-01-02T00:00:00.000Z", "id": "conversation-1", "extra": 1 } }),
    ];
    for value in refused {
        assert!(request(value.clone()).is_err(), "{value} is refused");
    }
}

#[test]
fn a_cursor_of_the_longest_allowed_length_is_accepted() {
    let id = "x".repeat(CURSOR_CHARACTERS_MAX);

    assert!(request(json!({ "limit": 5, "cursor": { "updatedAt": "t", "id": id } })).is_ok());
}

#[test]
fn a_request_must_be_an_object() {
    for value in [json!([]), json!("limit"), json!(null), json!(5)] {
        assert!(request(value.clone()).is_err(), "{value} is refused");
    }
}

/// The page has the three keys of the contract, and `nextCursor` is `null` on the last page.
#[test]
fn a_page_serializes_with_the_contract_keys() {
    let page = ConversationPage {
        items: Vec::new(),
        has_more: false,
        next_cursor: None,
    };

    assert_eq!(
        serde_json::to_value(&page).expect("a page serializes"),
        json!({ "items": [], "hasMore": false, "nextCursor": null })
    );
}

#[test]
fn a_cursor_serializes_with_camel_case_keys() {
    let cursor = ConversationCursor {
        updated_at: "2026-01-02T00:00:00.000Z".to_string(),
        id: "conversation-1".to_string(),
    };

    assert_eq!(
        serde_json::to_value(&cursor).expect("a cursor serializes"),
        json!({ "updatedAt": "2026-01-02T00:00:00.000Z", "id": "conversation-1" })
    );
}
