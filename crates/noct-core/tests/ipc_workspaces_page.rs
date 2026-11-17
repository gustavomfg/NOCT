//! The request and the page of `workspaces_list`. The limit is required and bounded. The cursor is the key of the last
//! project of the previous page: it is optional, and when it is present every field is required and bounded. Anything
//! else is refused before the store is read. An explicit `null` is refused, as the other optional fields are, except
//! the reply's `nextCursor`, which is always present and is `null` on the last page.

use noct_core::ipc::pages::{CURSOR_CHARACTERS_MAX, PAGE_LIMIT_MAX, PAGE_LIMIT_MIN};
use noct_core::ipc::workspaces::{
    PATH_CHARACTERS_MAX, WorkspaceCursor, WorkspacePage, WorkspacePageRequest,
};
use serde_json::{Value, json};

fn request(value: Value) -> Result<WorkspacePageRequest, serde_json::Error> {
    serde_json::from_value(value)
}

fn cursor(value: Value) -> Value {
    json!({ "limit": 20, "cursor": value })
}

fn valid_cursor() -> Value {
    json!({
        "favorite": false,
        "lastOpenedAt": "2026-01-02T00:00:00.000Z",
        "path": "/work/alpha",
    })
}

#[test]
fn the_first_page_takes_a_limit_and_no_cursor() {
    let parsed = request(json!({ "limit": 50 })).expect("a limit alone is a request");

    assert_eq!(parsed.limit, 50);
    assert_eq!(parsed.cursor, None);
}

#[test]
fn a_later_page_takes_the_cursor_of_the_last_project() {
    let parsed = request(cursor(json!({
        "favorite": true,
        "lastOpenedAt": "2026-01-02T00:00:00.000Z",
        "path": "/work/alpha",
    })))
    .expect("a limit and a cursor");

    let expected = WorkspaceCursor {
        favorite: true,
        last_opened_at: "2026-01-02T00:00:00.000Z".to_string(),
        path: "/work/alpha".to_string(),
    };
    assert_eq!(parsed.cursor, Some(expected));
}

#[test]
fn the_limit_bounds_are_accepted_and_the_others_refused() {
    assert_eq!(
        request(json!({ "limit": PAGE_LIMIT_MIN })).unwrap().limit,
        1
    );
    assert_eq!(
        request(json!({ "limit": PAGE_LIMIT_MAX })).unwrap().limit,
        500
    );
    for limit in [
        json!(0),
        json!(501),
        json!(2.5),
        json!("10"),
        json!(-1),
        json!(null),
    ] {
        assert!(
            request(json!({ "limit": limit })).is_err(),
            "limit {limit} is refused"
        );
    }
}

#[test]
fn the_limit_is_required() {
    assert!(request(json!({})).is_err(), "an empty request is refused");
    assert!(
        request(json!({ "cursor": valid_cursor() })).is_err(),
        "a cursor alone is refused"
    );
}

#[test]
fn a_cursor_needs_every_field_with_its_type() {
    for field in ["favorite", "lastOpenedAt", "path"] {
        let mut missing = valid_cursor();
        missing.as_object_mut().expect("an object").remove(field);
        assert!(
            request(cursor(missing)).is_err(),
            "a cursor without {field} is refused"
        );
    }
    assert!(
        request(cursor(json!({
            "favorite": "no",
            "lastOpenedAt": "2026-01-02T00:00:00.000Z",
            "path": "/work/alpha",
        })))
        .is_err(),
        "favorite is a boolean"
    );
    assert!(
        request(cursor(json!({
            "favorite": false,
            "lastOpenedAt": 20260102,
            "path": "/work/alpha",
        })))
        .is_err(),
        "lastOpenedAt is a string"
    );
}

#[test]
fn the_cursor_text_is_bounded_and_not_empty() {
    let long_time = "t".repeat(CURSOR_CHARACTERS_MAX);
    let long_path = "p".repeat(PATH_CHARACTERS_MAX);
    let accepted = request(cursor(json!({
        "favorite": false,
        "lastOpenedAt": long_time,
        "path": long_path,
    })));
    assert!(accepted.is_ok(), "the longest fields are accepted");

    let too_long_time = "t".repeat(CURSOR_CHARACTERS_MAX + 1);
    assert!(
        request(cursor(json!({
            "favorite": false,
            "lastOpenedAt": too_long_time,
            "path": "/work/alpha",
        })))
        .is_err(),
        "a time of {} characters is refused",
        CURSOR_CHARACTERS_MAX + 1
    );
    let too_long_path = "p".repeat(PATH_CHARACTERS_MAX + 1);
    assert!(
        request(cursor(json!({
            "favorite": false,
            "lastOpenedAt": "2026-01-02T00:00:00.000Z",
            "path": too_long_path,
        })))
        .is_err(),
        "a path of {} characters is refused",
        PATH_CHARACTERS_MAX + 1
    );
    assert!(
        request(cursor(json!({
            "favorite": false,
            "lastOpenedAt": "",
            "path": "/work/alpha",
        })))
        .is_err(),
        "an empty time is refused"
    );
}

#[test]
fn null_and_unknown_fields_are_refused() {
    assert!(
        request(json!({ "limit": 5, "cursor": null })).is_err(),
        "an explicit null cursor is refused"
    );
    assert!(
        request(cursor(json!({
            "favorite": false,
            "lastOpenedAt": "2026-01-02T00:00:00.000Z",
            "path": "/work/alpha",
            "id": "x",
        })))
        .is_err(),
        "a cursor field the contract does not name is refused"
    );
    assert!(
        request(json!({ "limit": 5, "offset": 40 })).is_err(),
        "the offset is gone: a page is read from its cursor"
    );
    assert!(request(json!({ "limit": 5, "workspace": "/work/alpha" })).is_err());
}

#[test]
fn a_request_must_be_an_object() {
    assert!(request(json!([5])).is_err());
    assert!(request(json!(null)).is_err());
}

#[test]
fn a_page_serializes_with_the_contract_keys() {
    let page = WorkspacePage {
        items: Vec::new(),
        has_more: false,
        next_cursor: None,
    };
    let value = serde_json::to_value(&page).expect("the page serializes");

    assert_eq!(
        value,
        json!({ "items": [], "hasMore": false, "nextCursor": null })
    );
}

#[test]
fn a_page_with_more_serializes_its_cursor_in_camel_case() {
    let page = WorkspacePage {
        items: Vec::new(),
        has_more: true,
        next_cursor: Some(WorkspaceCursor {
            favorite: true,
            last_opened_at: "2026-01-02T00:00:00.000Z".to_string(),
            path: "/work/alpha".to_string(),
        }),
    };
    let value = serde_json::to_value(&page).expect("the page serializes");

    assert_eq!(
        value,
        json!({
            "items": [],
            "hasMore": true,
            "nextCursor": {
                "favorite": true,
                "lastOpenedAt": "2026-01-02T00:00:00.000Z",
                "path": "/work/alpha",
            },
        })
    );
}

#[test]
fn a_reply_must_carry_next_cursor_even_when_it_is_null() {
    assert!(
        serde_json::from_value::<WorkspacePage>(json!({ "items": [], "hasMore": false })).is_err(),
        "a reply without nextCursor is refused, as the TypeScript type refuses it"
    );
    assert!(
        serde_json::from_value::<WorkspacePage>(
            json!({ "items": [], "hasMore": false, "nextCursor": null })
        )
        .is_ok()
    );
    assert!(
        serde_json::from_value::<WorkspacePage>(
            json!({ "items": [], "hasMore": false, "nextCursor": null, "offset": 0 })
        )
        .is_err(),
        "an unknown field of the reply is refused"
    );
}
