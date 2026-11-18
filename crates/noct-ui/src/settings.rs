//! The settings the interface shows and edits, with `settings_get` and `settings_set`. A reply is checked against the
//! contract of `noct-core` before it is shown. A reply that does not match is an error, not a default: a wrong value
//! shown as the saved one would mislead the person.

use noct_core::ipc::settings::{AppLanguage, AppSettings, AppTheme, ApprovalPolicy};
use serde_json::{Map, Value, json};

use crate::ipc::{self, Failure, Outcome};

/// The command the interface reads its settings with.
pub const COMMAND: &str = "settings_get";

/// The command that saves a setting. The theme button and the settings area save through it, one setting per call.
pub const SET_COMMAND: &str = "settings_set";

/// The text of a setting the shell has no value for.
pub const NOT_INFORMED: &str = "Não informado";

/// The choices of the language select: the value the contract uses, the value the select carries, and the words the
/// person reads. A choice is read back from the value the select carries, never from its words.
pub const LANGUAGE_CHOICES: [(AppLanguage, &str, &str); 2] = [
    (AppLanguage::PtBr, "pt-BR", "Português (Brasil)"),
    (AppLanguage::En, "en", "English"),
];

/// The choices of the agent safety select, in words short enough for the narrowest window.
pub const APPROVAL_CHOICES: [(ApprovalPolicy, &str, &str); 2] = [
    (
        ApprovalPolicy::Untrusted,
        "untrusted",
        "Aprovar cada alteração de arquivo",
    ),
    (
        ApprovalPolicy::OnRequest,
        "on-request",
        "Comandos sempre exigem aprovação",
    ),
];

/// The choices of the diagnostic select.
pub const DIAGNOSTIC_CHOICES: [(bool, &str, &str); 2] =
    [(true, "on", "Ativo"), (false, "off", "Inativo")];

/// The value of the select that shows `value`.
pub fn choice_value<T: Copy + PartialEq>(
    choices: &[(T, &'static str, &'static str)],
    value: T,
) -> &'static str {
    choices
        .iter()
        .find(|&&(choice, _, _)| choice == value)
        .map_or("", |&(_, select, _)| select)
}

/// The choice that the select's value names, or none for a value the select does not carry.
pub fn choice_of<T: Copy>(choices: &[(T, &'static str, &'static str)], select: &str) -> Option<T> {
    choices
        .iter()
        .find(|&&(_, value, _)| value == select)
        .map(|&(choice, _, _)| choice)
}

/// The argument of a save: one setting and nothing else, so a change saves only what changed.
pub fn save_request(field: &str, value: Value) -> String {
    let mut input = Map::new();
    input.insert(field.to_string(), value);
    json!({ "input": input }).to_string()
}

/// The argument of the theme save.
pub fn theme_request(theme: AppTheme) -> String {
    save_request("theme", json!(theme))
}

/// The argument of the language save.
pub fn language_request(language: AppLanguage) -> String {
    save_request("language", json!(language))
}

/// The argument of the agent safety save.
pub fn approval_request(policy: ApprovalPolicy) -> String {
    save_request("approvalPolicy", json!(policy))
}

/// The argument of the diagnostic save.
pub fn diagnostic_request(enabled: bool) -> String {
    save_request("diagnosticMode", json!(enabled))
}

/// The settings the shell holds after a save, or the shell's error to show beside the control that saved.
pub fn saved(outcome: Outcome) -> Result<SettingsView, Failure> {
    match settle(outcome) {
        SettingsView::Failed(failure) => Err(failure),
        view => Ok(view),
    }
}

/// What the settings area shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsView {
    Loading,
    Ready(AppSettings),
    Failed(Failure),
}

/// Turns the outcome of a call into what the settings area shows.
pub fn settle(outcome: Outcome) -> SettingsView {
    match ipc::reply(outcome) {
        Ok(text) => match serde_json::from_str::<AppSettings>(&text) {
            Ok(settings) => SettingsView::Ready(settings),
            Err(_) => SettingsView::Failed(ipc::invalid_response()),
        },
        Err(failure) => SettingsView::Failed(failure),
    }
}

/// The version of Pandoc the shell found, or the words for none.
pub fn pandoc_label(settings: &AppSettings) -> String {
    settings
        .pandoc_version
        .clone()
        .unwrap_or_else(|| NOT_INFORMED.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{UNAVAILABLE_MESSAGE, UNEXPECTED_MESSAGE, UNKNOWN_MESSAGE};

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
    fn the_theme_is_saved_alone() {
        assert_eq!(
            theme_request(AppTheme::Dark),
            r#"{"input":{"theme":"dark"}}"#
        );
        assert_eq!(
            theme_request(AppTheme::Light),
            r#"{"input":{"theme":"light"}}"#
        );
    }

    #[test]
    fn a_saved_reply_is_the_settings_the_shell_holds() {
        match saved(Outcome::Resolved(
            r#"{"approvalPolicy":"on-request","theme":"light"}"#.to_string(),
        )) {
            Ok(SettingsView::Ready(settings)) => assert_eq!(settings.theme, Some(AppTheme::Light)),
            other => panic!("the saved settings are shown, got {other:?}"),
        }
    }

    #[test]
    fn a_refused_save_is_the_shell_error_and_an_unexpected_reply_is_an_error() {
        assert_eq!(
            saved(Outcome::Rejected(
                r#"{"code":"persistence","message":"Não foi possível acessar os dados locais."}"#
                    .to_string(),
            )),
            Err(Failure {
                code: "persistence".to_string(),
                message: "Não foi possível acessar os dados locais.".to_string(),
            })
        );
        assert_eq!(
            saved(Outcome::Resolved("[]".to_string())),
            Err(ipc::invalid_response())
        );
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
    fn each_setting_is_saved_alone() {
        assert_eq!(
            language_request(AppLanguage::En),
            r#"{"input":{"language":"en"}}"#
        );
        assert_eq!(
            approval_request(ApprovalPolicy::OnRequest),
            r#"{"input":{"approvalPolicy":"on-request"}}"#
        );
        assert_eq!(
            diagnostic_request(false),
            r#"{"input":{"diagnosticMode":false}}"#
        );
    }

    #[test]
    fn the_choices_carry_the_values_the_contract_uses() {
        for (language, select, _) in LANGUAGE_CHOICES {
            assert_eq!(
                serde_json::to_value(language).expect("serializes"),
                json!(select)
            );
        }
        for (policy, select, _) in APPROVAL_CHOICES {
            assert_eq!(
                serde_json::to_value(policy).expect("serializes"),
                json!(select)
            );
        }
    }

    #[test]
    fn a_choice_is_read_back_from_the_value_the_select_carries() {
        assert_eq!(choice_of(&LANGUAGE_CHOICES, "en"), Some(AppLanguage::En));
        assert_eq!(choice_of(&DIAGNOSTIC_CHOICES, "off"), Some(false));
        assert_eq!(
            choice_of(&LANGUAGE_CHOICES, "English"),
            None,
            "the words are not a value"
        );
        assert_eq!(choice_value(&LANGUAGE_CHOICES, AppLanguage::PtBr), "pt-BR");
        assert_eq!(choice_value(&DIAGNOSTIC_CHOICES, true), "on");
    }

    #[test]
    fn the_theme_and_pandoc_read_as_the_person_reads_them() {
        let settings = AppSettings {
            approval_policy: ApprovalPolicy::Untrusted,
            diagnostic_mode: Some(true),
            theme: Some(AppTheme::Light),
            language: Some(AppLanguage::En),
            pandoc_version: Some("3.1.2".to_string()),
        };

        assert_eq!(pandoc_label(&settings), "3.1.2");
    }

    #[test]
    fn an_absent_optional_setting_is_not_shown_as_a_value() {
        let settings: AppSettings = serde_json::from_str(r#"{"approvalPolicy":"on-request"}"#)
            .expect("the contract accepts it");

        assert_eq!(pandoc_label(&settings), NOT_INFORMED);
        assert_eq!(
            settings
                .language
                .map_or("", |language| choice_value(&LANGUAGE_CHOICES, language)),
            ""
        );
    }
}
