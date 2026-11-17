//! The request and the page of `workspaces_list`. The limit is required and bounded, the offset is optional and
//! bounded, and anything else is refused before the store is read. An explicit `null` is refused, as the other
//! optional fields are.

use noct_core::ipc::pages::{OFFSET_MAX, PAGE_LIMIT_MAX, PAGE_LIMIT_MIN};
use noct_core::ipc::workspaces::{WorkspacePage, WorkspacePageRequest};
use serde_json::{Value, json};

fn request(value: Value) -> Result<WorkspacePageRequest, serde_json::Error> {
    serde_json::from_value(value)
}

#[test]
fn the_first_page_takes_a_limit_and_starts_at_zero() {
    let parsed = request(json!({ "limit": 50 })).expect("a limit alone is a request");

    assert_eq!(parsed.limit, 50);
    assert_eq!(parsed.offset, 0);
}

#[test]
fn a_later_page_takes_an_offset() {
    let parsed = request(json!({ "limit": 20, "offset": 40 })).expect("a limit and an offset");

    assert_eq!(parsed.offset, 40);
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
        request(json!({ "offset": 0 })).is_err(),
        "an offset alone is refused"
    );
}

#[test]
fn the_offset_is_bounded_and_null_is_refused() {
    assert_eq!(
        request(json!({ "limit": 5, "offset": OFFSET_MAX }))
            .unwrap()
            .offset,
        1_000_000
    );
    assert!(request(json!({ "limit": 5, "offset": OFFSET_MAX + 1 })).is_err());
    assert!(request(json!({ "limit": 5, "offset": -1 })).is_err());
    assert!(request(json!({ "limit": 5, "offset": null })).is_err());
}

#[test]
fn unknown_fields_are_refused() {
    assert!(
        request(json!({ "limit": 5, "cursor": {} })).is_err(),
        "cursors belong to conversations"
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
        has_more: true,
    };
    let value = serde_json::to_value(&page).expect("the page serializes");

    assert_eq!(value, json!({ "items": [], "hasMore": true }));
}
