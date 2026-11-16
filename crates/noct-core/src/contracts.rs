//! Wire-compatible domain contracts, ported from `shared/types.ts`.
//!
//! The JSON shape must match the TypeScript types exactly: camelCase keys, kebab-case
//! enum values where the TypeScript union uses them, and `null` or omitted keys as
//! the TypeScript optionality dictates. Backups and renderer payloads depend on it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceAvailability {
    Available,
    Missing,
    PermissionDenied,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub workspace: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub path: String,
    pub name: String,
    pub favorite: bool,
    pub authorized: bool,
    pub availability: WorkspaceAvailability,
    /// Optional in TypeScript (`availabilityMessage?: string`): omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_message: Option<String>,
    pub created_at: String,
    pub last_opened_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: Role,
    pub content: String,
    /// `metadata: string | null` in TypeScript: serialized as `null`, never omitted.
    pub metadata: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    /// Round-trips a value and compares it to the JSON the TypeScript contract produces.
    fn assert_wire<T>(value: &T, expected: Value)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        assert_eq!(serde_json::to_value(value).unwrap(), expected);
        let back: T = serde_json::from_value(expected).unwrap();
        assert_eq!(&back, value);
    }

    #[test]
    fn roles_use_lowercase_wire_values() {
        assert_wire(&Role::Assistant, json!("assistant"));
        assert_wire(&Role::User, json!("user"));
        assert_wire(&Role::System, json!("system"));
    }

    #[test]
    fn availability_uses_kebab_case_wire_values() {
        assert_wire(
            &WorkspaceAvailability::PermissionDenied,
            json!("permission-denied"),
        );
        assert_wire(&WorkspaceAvailability::Available, json!("available"));
    }

    #[test]
    fn conversation_uses_camel_case_keys() {
        let value = Conversation {
            id: "c1".into(),
            title: "t".into(),
            workspace: "/w".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            updated_at: "2026-01-02T00:00:00.000Z".into(),
        };
        assert_wire(
            &value,
            json!({
                "id": "c1", "title": "t", "workspace": "/w",
                "createdAt": "2026-01-01T00:00:00.000Z", "updatedAt": "2026-01-02T00:00:00.000Z"
            }),
        );
    }

    #[test]
    fn workspace_omits_absent_optional_message() {
        let value = Workspace {
            path: "/w".into(),
            name: "w".into(),
            favorite: false,
            authorized: true,
            availability: WorkspaceAvailability::Missing,
            availability_message: None,
            created_at: "a".into(),
            last_opened_at: "b".into(),
        };
        assert_wire(
            &value,
            json!({
                "path": "/w", "name": "w", "favorite": false, "authorized": true,
                "availability": "missing", "createdAt": "a", "lastOpenedAt": "b"
            }),
        );
    }

    #[test]
    fn workspace_keeps_present_optional_message() {
        let value = Workspace {
            path: "/w".into(),
            name: "w".into(),
            favorite: true,
            authorized: false,
            availability: WorkspaceAvailability::Invalid,
            availability_message: Some("gone".into()),
            created_at: "a".into(),
            last_opened_at: "b".into(),
        };
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["availabilityMessage"], json!("gone"));
    }

    #[test]
    fn message_serializes_null_metadata_explicitly() {
        let value = Message {
            id: "m1".into(),
            conversation_id: "c1".into(),
            role: Role::User,
            content: "oi".into(),
            metadata: None,
            created_at: "a".into(),
        };
        assert_wire(
            &value,
            json!({
                "id": "m1", "conversationId": "c1", "role": "user", "content": "oi",
                "metadata": null, "createdAt": "a"
            }),
        );
    }
}
