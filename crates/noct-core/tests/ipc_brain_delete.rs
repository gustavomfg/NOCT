//! The request of `brain_delete`, checked against `brainMemoryDeleteSchema` in `shared/ipc/schemas.ts`.

use noct_core::ipc::brain::BrainMemoryDeleteRequest;
use serde_json::json;

const CONVERSATION: &str = "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d";
const MEMORY: &str = "7a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";

#[test]
fn both_uuids_are_read_and_nothing_else_is_accepted() {
    let parsed = serde_json::from_value::<BrainMemoryDeleteRequest>(json!({
        "conversationId": CONVERSATION,
        "memoryId": MEMORY,
    }))
    .expect("the request is valid");
    assert_eq!(parsed.conversation_id, CONVERSATION);
    assert_eq!(parsed.memory_id, MEMORY);

    let extra = json!({ "conversationId": CONVERSATION, "memoryId": MEMORY, "force": true });
    assert!(
        serde_json::from_value::<BrainMemoryDeleteRequest>(extra).is_err(),
        "strict"
    );

    for missing in ["conversationId", "memoryId"] {
        let mut value = json!({ "conversationId": CONVERSATION, "memoryId": MEMORY });
        value.as_object_mut().expect("an object").remove(missing);
        assert!(
            serde_json::from_value::<BrainMemoryDeleteRequest>(value).is_err(),
            "without {missing}"
        );
    }

    let not_a_uuid = json!({ "conversationId": CONVERSATION, "memoryId": "not-a-uuid" });
    assert!(serde_json::from_value::<BrainMemoryDeleteRequest>(not_a_uuid).is_err());
}
