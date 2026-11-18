//! The OpenRouter key and the project consent of the settings area. Requests, replies and the state the area shows.
//!
//! The key is write-only. A save sends the key once, and the draft is emptied before the call is made, whatever the
//! reply. The replies carry no key, and each reply is checked with `deny_unknown_fields`, so a reply that carried one
//! would be refused as an invalid response rather than shown.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::ipc::{self, Failure, Outcome};

/// The commands of the key. The status and removal take no argument.
pub const KEY_STATUS: &str = "provider_key_status";
pub const KEY_SAVE: &str = "provider_key_save";
pub const KEY_REMOVE: &str = "provider_key_remove";

/// The commands of the consent. Each takes the project path.
pub const CONSENT_STATUS: &str = "provider_consent_status";
pub const CONSENT_GRANT: &str = "provider_consent_grant";
pub const CONSENT_REVOKE: &str = "provider_consent_revoke";

/// The provider the area shows. Beta 1 has one.
pub const PROVIDER: &str = "openrouter";

/// What the shell says about the key. It never carries the key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct KeyStatus {
    pub provider: String,
    pub configured: bool,
}

/// What the shell says about the consent of one project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ConsentStatus {
    pub workspace: String,
    pub provider: String,
    pub granted: bool,
}

/// What an area shows for one read: loading, the value, or the shell's error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderView<T> {
    Loading,
    Ready(T),
    Failed(Failure),
}

/// The argument of a key save: the provider and the key, and nothing else.
pub fn key_save_payload(key: &str) -> String {
    json!({ "input": { "provider": PROVIDER, "key": key } }).to_string()
}

/// The argument of a consent call: the project path, and nothing else.
pub fn consent_payload(workspace: &str) -> String {
    json!({ "input": { "workspace": workspace } }).to_string()
}

/// Turns the outcome of a call into what the area shows. A reply that does not match the type is an error, not a
/// default, and a shell error keeps its code and message.
pub fn settle<T: DeserializeOwned>(outcome: Outcome) -> ProviderView<T> {
    match ipc::reply(outcome) {
        Ok(text) => match serde_json::from_str::<T>(&text) {
            Ok(value) => ProviderView::Ready(value),
            Err(_) => ProviderView::Failed(ipc::invalid_response()),
        },
        Err(failure) => ProviderView::Failed(failure),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{UNAVAILABLE_MESSAGE, UNEXPECTED_MESSAGE};

    const KEY: &str = "sk-or-v1-0123456789abcdef";

    #[test]
    fn the_key_is_sent_in_one_argument_with_the_provider() {
        assert_eq!(
            key_save_payload(KEY),
            r#"{"input":{"provider":"openrouter","key":"sk-or-v1-0123456789abcdef"}}"#
        );
    }

    #[test]
    fn a_consent_call_carries_the_project_path_only() {
        assert_eq!(
            consent_payload("/projects/app"),
            r#"{"input":{"workspace":"/projects/app"}}"#
        );
    }

    #[test]
    fn a_valid_key_status_is_shown() {
        assert_eq!(
            settle::<KeyStatus>(Outcome::Resolved(
                r#"{"provider":"openrouter","configured":true}"#.to_string()
            )),
            ProviderView::Ready(KeyStatus {
                provider: "openrouter".to_string(),
                configured: true,
            })
        );
    }

    #[test]
    fn a_key_status_that_carries_a_key_is_refused_and_the_key_is_not_shown() {
        let reply = format!(r#"{{"provider":"openrouter","configured":true,"key":"{KEY}"}}"#);
        let view = settle::<KeyStatus>(Outcome::Resolved(reply));

        assert_eq!(
            view,
            ProviderView::Failed(Failure {
                code: "invalid-response".to_string(),
                message: UNEXPECTED_MESSAGE.to_string(),
            })
        );
        let shown = format!("{view:?}");
        assert!(!shown.contains(KEY), "the refusal holds no key: {shown}");
    }

    #[test]
    fn a_key_status_of_another_shape_is_refused() {
        for text in [
            r#"{"provider":"openrouter"}"#,
            r#"{"provider":"openrouter","configured":"yes"}"#,
            r#"{"provider":"openrouter","configured":true,"extra":1}"#,
            "[]",
            "not json",
        ] {
            assert_eq!(
                settle::<KeyStatus>(Outcome::Resolved(text.to_string())),
                ProviderView::Failed(ipc::invalid_response()),
                "{text}"
            );
        }
    }

    #[test]
    fn a_consent_reply_is_shown_and_an_extra_field_is_refused() {
        assert_eq!(
            settle::<ConsentStatus>(Outcome::Resolved(
                r#"{"workspace":"/p","provider":"openrouter","granted":false}"#.to_string()
            )),
            ProviderView::Ready(ConsentStatus {
                workspace: "/p".to_string(),
                provider: "openrouter".to_string(),
                granted: false,
            })
        );
        assert_eq!(
            settle::<ConsentStatus>(Outcome::Resolved(
                r#"{"workspace":"/p","provider":"openrouter","granted":false,"content":"x"}"#
                    .to_string()
            )),
            ProviderView::Failed(ipc::invalid_response())
        );
    }

    #[test]
    fn a_shell_error_keeps_its_code_and_message() {
        let view: ProviderView<KeyStatus> = settle(Outcome::Rejected(
            r#"{"code":"secure-storage-unavailable","message":"O sistema operacional não oferece armazenamento seguro de credenciais. Nada foi salvo."}"#
                .to_string(),
        ));
        assert_eq!(
            view,
            ProviderView::Failed(Failure {
                code: "secure-storage-unavailable".to_string(),
                message: "O sistema operacional não oferece armazenamento seguro de credenciais. Nada foi salvo."
                    .to_string(),
            })
        );
    }

    #[test]
    fn a_page_outside_the_shell_is_reported_as_unavailable() {
        match settle::<KeyStatus>(Outcome::Unavailable) {
            ProviderView::Failed(failure) => {
                assert_eq!(failure.code, "unavailable");
                assert_eq!(failure.message, UNAVAILABLE_MESSAGE);
            }
            other => panic!("expected a failure, got {other:?}"),
        }
    }
}
