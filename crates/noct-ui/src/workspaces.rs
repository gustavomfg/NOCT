//! The projects the sidebar lists, read with `workspaces_list`. The reply is a page, checked against the contract
//! of `noct-core` before it is shown. A reply with one entry outside the contract is an error, not a shorter list:
//! a project that is missing from the list would mislead the person about what the shell holds.
//!
//! The interface asks for the first page at the largest limit the shell accepts, and says so when more projects
//! follow. Reading the rest of the list is not done yet.

use noct_core::contracts::{Workspace, WorkspaceAvailability};
use noct_core::ipc::pages::PAGE_LIMIT_MAX;
use noct_core::ipc::workspaces::WorkspacePage;

use crate::ipc::{self, Failure, Outcome};

/// The command the interface reads the projects with.
pub const COMMAND: &str = "workspaces_list";

/// The payload of the first page: the command's argument, a page request at the largest limit.
pub fn first_page_payload() -> String {
    format!(r#"{{"input":{{"limit":{PAGE_LIMIT_MAX}}}}}"#)
}

/// What the sidebar's project list shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspacesView {
    Loading,
    Ready(WorkspacePage),
    Failed(Failure),
}

/// The note under the list when the shell holds more projects than the page shows.
pub const MORE_NOTE: &str = "Há mais projetos do que esta lista mostra.";

/// The note under a project that is not ready to use. A project that the person has not authorized in this
/// session says so; the shell marks a restored project unauthorized until it is chosen again.
const NEEDS_AUTHORIZATION: &str = "Não autorizado. Escolha a pasta de novo para usá-la.";

/// Turns the outcome of a call into what the sidebar shows.
pub fn settle(outcome: Outcome) -> WorkspacesView {
    match ipc::reply(outcome) {
        Ok(text) => match serde_json::from_str::<WorkspacePage>(&text) {
            Ok(page) => WorkspacesView::Ready(page),
            Err(_) => WorkspacesView::Failed(ipc::invalid_response()),
        },
        Err(failure) => WorkspacesView::Failed(failure),
    }
}

/// The note shown under a project's name, or nothing when the project can be used. The shell's own message is
/// shown when it has one, because it names the cause; otherwise the word for the availability is shown.
pub fn note(workspace: &Workspace) -> Option<String> {
    match workspace.availability {
        WorkspaceAvailability::Available if workspace.authorized => None,
        WorkspaceAvailability::Available => Some(NEEDS_AUTHORIZATION.to_string()),
        unavailable => Some(
            workspace
                .availability_message
                .clone()
                .unwrap_or_else(|| availability_label(unavailable).to_string()),
        ),
    }
}

fn availability_label(availability: WorkspaceAvailability) -> &'static str {
    match availability {
        WorkspaceAvailability::Available => "Disponível",
        WorkspaceAvailability::Missing => "A pasta não foi encontrada.",
        WorkspaceAvailability::PermissionDenied => "Sem permissão para ler a pasta.",
        WorkspaceAvailability::Invalid => "O caminho não é uma pasta válida.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{UNAVAILABLE_MESSAGE, UNEXPECTED_MESSAGE, UNKNOWN_MESSAGE};
    use serde_json::Value;

    /// The shared contract of the reply: the lists `noct-core` accepts and refuses.
    const CONTRACT: &str = include_str!("../../noct-core/tests/fixtures/ipc/workspaces-list.json");

    fn contract() -> Value {
        serde_json::from_str(CONTRACT).expect("the contract fixture is JSON")
    }

    fn workspace(availability: &str, authorized: bool, message: Option<&str>) -> Workspace {
        let mut value = serde_json::json!({
            "path": "/work/alpha",
            "name": "alpha",
            "favorite": false,
            "authorized": authorized,
            "availability": availability,
            "createdAt": "2026-01-01T00:00:00.000Z",
            "lastOpenedAt": "2026-01-02T00:00:00.000Z",
        });
        if let Some(message) = message {
            value["availabilityMessage"] = Value::from(message);
        }
        serde_json::from_value(value).expect("the workspace is in the contract")
    }

    #[test]
    fn every_valid_contract_reply_is_listed() {
        for entry in contract()["valid"].as_array().expect("valid cases") {
            let view = settle(Outcome::Resolved(entry.to_string()));
            assert!(
                matches!(view, WorkspacesView::Ready(_)),
                "{entry} is listed"
            );
        }
    }

    #[test]
    fn an_empty_page_is_a_list_and_not_an_error() {
        assert_eq!(
            settle(Outcome::Resolved(
                r#"{"items":[],"hasMore":false,"nextCursor":null}"#.to_string()
            )),
            WorkspacesView::Ready(WorkspacePage {
                items: Vec::new(),
                has_more: false,
                next_cursor: None,
            })
        );
    }

    #[test]
    fn a_page_that_has_more_projects_says_so() {
        let cursor =
            r#"{"favorite":false,"lastOpenedAt":"2026-01-02T00:00:00.000Z","path":"/work/alpha"}"#;
        match settle(Outcome::Resolved(format!(
            r#"{{"items":[],"hasMore":true,"nextCursor":{cursor}}}"#
        ))) {
            WorkspacesView::Ready(page) => assert!(page.has_more),
            other => panic!("the page is listed, got {other:?}"),
        }
    }

    #[test]
    fn the_first_page_asks_for_the_largest_limit() {
        assert_eq!(first_page_payload(), r#"{"input":{"limit":500}}"#);
    }

    #[test]
    fn every_invalid_contract_reply_is_an_error_not_a_shorter_list() {
        for entry in contract()["invalid"].as_array().expect("invalid cases") {
            match settle(Outcome::Resolved(entry.to_string())) {
                WorkspacesView::Failed(failure) => {
                    assert_eq!(failure.code, "invalid-response", "{entry}");
                    assert_eq!(failure.message, UNEXPECTED_MESSAGE);
                }
                other => panic!("{entry} must be refused, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_shell_error_keeps_its_code_and_message() {
        let view = settle(Outcome::Rejected(
            r#"{"code":"persistence","message":"Não foi possível acessar os dados locais."}"#
                .to_string(),
        ));

        assert_eq!(
            view,
            WorkspacesView::Failed(Failure {
                code: "persistence".to_string(),
                message: "Não foi possível acessar os dados locais.".to_string(),
            })
        );
    }

    #[test]
    fn an_error_outside_the_shell_shape_gets_the_generic_message() {
        let view = settle(Outcome::Rejected("not json".to_string()));

        assert_eq!(
            view,
            WorkspacesView::Failed(Failure {
                code: "unknown".to_string(),
                message: UNKNOWN_MESSAGE.to_string(),
            })
        );
    }

    #[test]
    fn a_page_outside_the_shell_is_reported_as_unavailable() {
        assert_eq!(
            settle(Outcome::Unavailable),
            WorkspacesView::Failed(Failure {
                code: "unavailable".to_string(),
                message: UNAVAILABLE_MESSAGE.to_string(),
            })
        );
    }

    #[test]
    fn a_usable_project_has_no_note() {
        assert_eq!(note(&workspace("available", true, None)), None);
    }

    #[test]
    fn a_project_outside_this_session_asks_for_its_folder_again() {
        assert_eq!(
            note(&workspace("available", false, None)),
            Some(NEEDS_AUTHORIZATION.to_string())
        );
    }

    #[test]
    fn the_shells_own_message_names_the_cause_when_there_is_one() {
        assert_eq!(
            note(&workspace("missing", false, Some("A pasta não existe."))),
            Some("A pasta não existe.".to_string())
        );
    }

    #[test]
    fn an_unavailable_project_without_a_message_says_what_is_wrong() {
        assert_eq!(
            note(&workspace("missing", false, None)),
            Some("A pasta não foi encontrada.".to_string())
        );
        assert_eq!(
            note(&workspace("permission-denied", false, None)),
            Some("Sem permissão para ler a pasta.".to_string())
        );
        assert_eq!(
            note(&workspace("invalid", false, None)),
            Some("O caminho não é uma pasta válida.".to_string())
        );
    }
}
