//! The request of `workspace_set_favorite`. Both fields are required, the path has from 1 to 4,000 characters, the
//! flag is a boolean, and anything else is refused before the store is read. The TypeScript schema
//! `workspaceFavoriteSchema` refuses the same inputs, so these cases are the ones that schema refuses.

use noct_core::ipc::workspaces::{PATH_CHARACTERS_MAX, WorkspaceFavoriteRequest};
use serde_json::{Value, json};

fn request(value: Value) -> Result<WorkspaceFavoriteRequest, serde_json::Error> {
    serde_json::from_value(value)
}

#[test]
fn a_path_and_a_flag_are_a_request() {
    let parsed = request(json!({ "workspace": "/work/alpha", "favorite": true }))
        .expect("a path and a flag are a request");

    assert_eq!(parsed.workspace, "/work/alpha");
    assert!(parsed.favorite);
}

#[test]
fn the_flag_may_unmark_a_favorite() {
    assert!(
        !request(json!({ "workspace": "/work/alpha", "favorite": false }))
            .unwrap()
            .favorite
    );
}

#[test]
fn both_fields_are_required() {
    assert!(
        request(json!({ "workspace": "/work/alpha" })).is_err(),
        "favorite is required"
    );
    assert!(
        request(json!({ "favorite": true })).is_err(),
        "workspace is required"
    );
    assert!(request(json!({})).is_err());
}

#[test]
fn the_path_is_one_to_the_maximum_characters_long() {
    let longest = "a".repeat(PATH_CHARACTERS_MAX);
    assert_eq!(
        request(json!({ "workspace": longest, "favorite": true }))
            .unwrap()
            .workspace
            .len(),
        PATH_CHARACTERS_MAX
    );
    assert!(
        request(json!({ "workspace": "", "favorite": true })).is_err(),
        "an empty path"
    );
    let too_long = "a".repeat(PATH_CHARACTERS_MAX + 1);
    assert!(request(json!({ "workspace": too_long, "favorite": true })).is_err());
}

#[test]
fn the_fields_have_their_types() {
    for value in [
        json!({ "workspace": null, "favorite": true }),
        json!({ "workspace": 7, "favorite": true }),
        json!({ "workspace": "/work/alpha", "favorite": "yes" }),
        json!({ "workspace": "/work/alpha", "favorite": 1 }),
        json!({ "workspace": "/work/alpha", "favorite": null }),
    ] {
        assert!(request(value.clone()).is_err(), "{value} is refused");
    }
}

#[test]
fn unknown_fields_are_refused() {
    assert!(
        request(json!({ "workspace": "/work/alpha", "favorite": true, "theme": "dark" })).is_err()
    );
}

#[test]
fn a_request_must_be_an_object() {
    assert!(request(json!(["/work/alpha", true])).is_err());
    assert!(request(json!(null)).is_err());
}
