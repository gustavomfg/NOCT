//! The consent request and reply of a project. The request carries a path and nothing else; the reply carries no content
//! of the project.

use noct_core::ai::consent::{CONSENT_VERSION, ProjectConsentRequest, ProjectConsentStatus};
use noct_core::ai::provider_key::ProviderKeyProvider;
use noct_core::ipc::workspaces::PATH_CHARACTERS_MAX;

fn request(workspace: &str) -> ProjectConsentRequest {
    ProjectConsentRequest {
        workspace: workspace.to_string(),
    }
}

#[test]
fn the_consent_version_names_the_beta_one_scope() {
    assert_eq!(CONSENT_VERSION, "openrouter-v1");
}

#[test]
fn a_path_of_one_to_the_maximum_characters_is_accepted() {
    assert!(request("/projects/app").validated().is_ok());
    assert!(
        request(&"p".repeat(PATH_CHARACTERS_MAX))
            .validated()
            .is_ok()
    );
    assert!(request("").validated().is_err(), "an empty path is refused");
    assert!(
        request(&"p".repeat(PATH_CHARACTERS_MAX + 1))
            .validated()
            .is_err(),
        "a path over the maximum is refused"
    );
}

#[test]
fn a_path_is_counted_in_characters_not_bytes() {
    let accented = "é".repeat(PATH_CHARACTERS_MAX);
    assert!(
        request(&accented).validated().is_ok(),
        "{PATH_CHARACTERS_MAX} accented characters fit"
    );
}

#[test]
fn the_request_refuses_unknown_fields_and_other_shapes() {
    assert!(
        serde_json::from_str::<ProjectConsentRequest>(r#"{"workspace":"/p","granted":true}"#)
            .is_err()
    );
    assert!(serde_json::from_str::<ProjectConsentRequest>(r#"{"workspace":7}"#).is_err());
    assert!(serde_json::from_str::<ProjectConsentRequest>(r#"{}"#).is_err());
    let parsed: ProjectConsentRequest =
        serde_json::from_str(r#"{"workspace":"/p"}"#).expect("the request parses");
    assert_eq!(parsed, request("/p"));
}

#[test]
fn the_reply_carries_the_path_the_provider_and_the_flag_only() {
    let status = ProjectConsentStatus {
        workspace: "/projects/app".to_string(),
        provider: ProviderKeyProvider::OpenRouter,
        granted: true,
    };
    let json = serde_json::to_string(&status).expect("the reply serializes");
    assert_eq!(
        json,
        r#"{"workspace":"/projects/app","provider":"openrouter","granted":true}"#
    );
}
