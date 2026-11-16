//! The settings the interface shows, read with `settings_get`. A reply is checked against the contract of
//! `noct-core` before it is shown. A reply that does not match is an error, not a default: a wrong value
//! shown as the saved one would mislead the person.

use noct_core::ipc::settings::{AppLanguage, AppSettings, AppTheme, ApprovalPolicy};
use serde_json::Value;

use crate::ipc::Outcome;

/// The command the interface reads its settings with.
pub const COMMAND: &str = "settings_get";

/// What the settings area shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsView {
    Loading,
    Ready(AppSettings),
    Failed(Failure),
}

/// Why the settings could not be read: a code and the message to show. The message comes from the shell
/// when the shell answered with an error, and from this module otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub code: String,
    pub message: String,
}

const UNAVAILABLE_MESSAGE: &str =
    "Não foi possível falar com o NOCT. Abra o aplicativo pela janela do desktop.";
const UNEXPECTED_MESSAGE: &str = "O NOCT respondeu de forma inesperada.";
const UNKNOWN_MESSAGE: &str = "Não foi possível ler as configurações.";
const NOT_INFORMED: &str = "Não informado";

/// Turns the outcome of a call into what the settings area shows.
pub fn settle(outcome: Outcome) -> SettingsView {
    match outcome {
        Outcome::Resolved(text) => match serde_json::from_str::<AppSettings>(&text) {
            Ok(settings) => SettingsView::Ready(settings),
            Err(_) => SettingsView::Failed(Failure {
                code: "invalid-response".to_string(),
                message: UNEXPECTED_MESSAGE.to_string(),
            }),
        },
        Outcome::Rejected(text) => SettingsView::Failed(failure_of(&text)),
        Outcome::Unavailable => SettingsView::Failed(Failure {
            code: "unavailable".to_string(),
            message: UNAVAILABLE_MESSAGE.to_string(),
        }),
    }
}

/// The code and message of a command error, which the shell sends as `{ "code": ..., "message": ... }`.
fn failure_of(text: &str) -> Failure {
    let parsed: Option<Value> = serde_json::from_str(text).ok();
    let field = |name: &str| {
        parsed
            .as_ref()
            .and_then(|value| value.get(name))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    match (field("code"), field("message")) {
        (Some(code), Some(message)) => Failure { code, message },
        _ => Failure {
            code: "unknown".to_string(),
            message: UNKNOWN_MESSAGE.to_string(),
        },
    }
}

/// The rows the settings area shows, in order: the label and the value as the person reads them.
pub fn rows(settings: &AppSettings) -> Vec<(&'static str, String)> {
    vec![
        ("Tema", theme_label(settings.theme).to_string()),
        ("Idioma", language_label(settings.language).to_string()),
        (
            "Segurança do agente",
            approval_label(settings.approval_policy).to_string(),
        ),
        (
            "Diagnóstico",
            diagnostic_label(settings.diagnostic_mode).to_string(),
        ),
        (
            "Pandoc",
            settings
                .pandoc_version
                .clone()
                .unwrap_or_else(|| NOT_INFORMED.to_string()),
        ),
    ]
}

fn theme_label(theme: Option<AppTheme>) -> &'static str {
    match theme {
        Some(AppTheme::Dark) => "NOCT escuro",
        Some(AppTheme::Light) => "NOCT claro",
        None => NOT_INFORMED,
    }
}

fn language_label(language: Option<AppLanguage>) -> &'static str {
    match language {
        Some(AppLanguage::PtBr) => "Português (Brasil)",
        Some(AppLanguage::En) => "English",
        None => NOT_INFORMED,
    }
}

/// The wording of the agent safety option in the React settings (`settings.approveFileChanges` and
/// `settings.approveFileChangesHint`).
fn approval_label(policy: ApprovalPolicy) -> &'static str {
    match policy {
        ApprovalPolicy::Untrusted => "Pedir aprovação antes de cada alteração de arquivo",
        ApprovalPolicy::OnRequest => "Comandos sempre exigem aprovação",
    }
}

fn diagnostic_label(enabled: Option<bool>) -> &'static str {
    match enabled {
        Some(true) => "Ativo",
        Some(false) => "Inativo",
        None => NOT_INFORMED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared contract of the settings reply: the cases `noct-core` accepts and refuses.
    const CONTRACT: &str = include_str!("../../noct-core/tests/fixtures/ipc/settings-get.json");

    fn contract() -> Value {
        serde_json::from_str(CONTRACT).expect("the contract fixture is JSON")
    }

    #[test]
    fn every_valid_contract_reply_is_shown() {
        for entry in contract()["valid"].as_array().expect("valid cases") {
            let view = settle(Outcome::Resolved(entry.to_string()));
            assert!(matches!(view, SettingsView::Ready(_)), "{entry} is shown");
        }
    }

    #[test]
    fn every_invalid_contract_reply_is_an_error_not_a_default() {
        for entry in contract()["invalid"].as_array().expect("invalid cases") {
            let view = settle(Outcome::Resolved(entry.to_string()));
            match view {
                SettingsView::Failed(failure) => {
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
            SettingsView::Failed(Failure {
                code: "persistence".to_string(),
                message: "Não foi possível acessar os dados locais.".to_string(),
            })
        );
    }

    #[test]
    fn an_error_that_is_not_the_shell_shape_gets_the_generic_message() {
        for text in ["", "not json", r#"{"code":"persistence"}"#, "[]"] {
            let view = settle(Outcome::Rejected(text.to_string()));
            assert_eq!(
                view,
                SettingsView::Failed(Failure {
                    code: "unknown".to_string(),
                    message: UNKNOWN_MESSAGE.to_string(),
                }),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_page_outside_the_shell_is_reported_as_unavailable() {
        let view = settle(Outcome::Unavailable);

        assert_eq!(
            view,
            SettingsView::Failed(Failure {
                code: "unavailable".to_string(),
                message: UNAVAILABLE_MESSAGE.to_string(),
            })
        );
    }

    #[test]
    fn the_rows_read_the_saved_values_as_the_person_reads_them() {
        let settings = AppSettings {
            approval_policy: ApprovalPolicy::Untrusted,
            diagnostic_mode: Some(true),
            theme: Some(AppTheme::Light),
            language: Some(AppLanguage::En),
            pandoc_version: None,
        };

        assert_eq!(
            rows(&settings),
            vec![
                ("Tema", "NOCT claro".to_string()),
                ("Idioma", "English".to_string()),
                (
                    "Segurança do agente",
                    "Pedir aprovação antes de cada alteração de arquivo".to_string()
                ),
                ("Diagnóstico", "Ativo".to_string()),
                ("Pandoc", NOT_INFORMED.to_string()),
            ]
        );
    }

    #[test]
    fn an_absent_optional_setting_is_not_shown_as_a_value() {
        let settings: AppSettings = serde_json::from_str(r#"{"approvalPolicy":"on-request"}"#)
            .expect("the contract accepts it");

        let shown = rows(&settings);

        assert_eq!(shown[0], ("Tema", NOT_INFORMED.to_string()));
        assert_eq!(shown[1], ("Idioma", NOT_INFORMED.to_string()));
        assert_eq!(shown[3], ("Diagnóstico", NOT_INFORMED.to_string()));
    }
}
