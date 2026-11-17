//! The request of `artifacts_delete`, checked against `artifacts.delete` in `electron/ipc/registerArtifactIpc.ts`.

use noct_core::ipc::artifacts::ArtifactDeleteRequest;
use serde_json::json;

const CONVERSATION: &str = "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d";
const ARTIFACT: &str = "9b8a7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d";

#[test]
fn both_uuids_are_read_and_nothing_else_is_accepted() {
    let parsed = serde_json::from_value::<ArtifactDeleteRequest>(json!({
        "conversationId": CONVERSATION,
        "artifactId": ARTIFACT,
    }))
    .expect("the request is valid");
    assert_eq!(parsed.conversation_id, CONVERSATION);
    assert_eq!(parsed.artifact_id, ARTIFACT);

    let extra = json!({ "conversationId": CONVERSATION, "artifactId": ARTIFACT, "force": true });
    assert!(
        serde_json::from_value::<ArtifactDeleteRequest>(extra).is_err(),
        "strict"
    );

    for missing in ["conversationId", "artifactId"] {
        let mut value = json!({ "conversationId": CONVERSATION, "artifactId": ARTIFACT });
        value.as_object_mut().expect("an object").remove(missing);
        assert!(
            serde_json::from_value::<ArtifactDeleteRequest>(value).is_err(),
            "without {missing}"
        );
    }

    let not_a_uuid = json!({ "conversationId": CONVERSATION, "artifactId": "not-a-uuid" });
    assert!(serde_json::from_value::<ArtifactDeleteRequest>(not_a_uuid).is_err());
}
