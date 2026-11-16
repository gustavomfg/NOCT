//! The mapping between the saved settings and the `settings` contract, checked against the behavior of
//! `electron/ipc/registerSettingsIpc.ts` and `electron/database/SettingsRepository.ts`.

use std::collections::BTreeMap;

use noct_core::ipc::settings::{
    AppLanguage, AppSettings, AppTheme, ApprovalPolicy, SettingsUpdate, from_saved, to_saved,
};

fn saved(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

#[test]
fn an_empty_store_reads_the_defaults_of_the_settings_repository() {
    let output = from_saved(&BTreeMap::new());

    assert_eq!(
        output,
        AppSettings {
            approval_policy: ApprovalPolicy::OnRequest,
            diagnostic_mode: Some(false),
            theme: Some(AppTheme::Dark),
            language: Some(AppLanguage::PtBr),
            pandoc_version: None,
        }
    );
}

#[test]
fn saved_values_are_read_as_their_contract_values() {
    let output = from_saved(&saved(&[
        ("approvalPolicy", "untrusted"),
        ("diagnosticMode", "true"),
        ("theme", "light"),
        ("language", "en"),
        ("pandocVersion", "3.1.2"),
    ]));

    assert_eq!(output.approval_policy, ApprovalPolicy::Untrusted);
    assert_eq!(output.diagnostic_mode, Some(true));
    assert_eq!(output.theme, Some(AppTheme::Light));
    assert_eq!(output.language, Some(AppLanguage::En));
    assert_eq!(output.pandoc_version.as_deref(), Some("3.1.2"));
}

/// A stored value that is not the allowed one reads as the fallback, as `constrain` and
/// `SettingsRepository.get` do. `diagnosticMode` is true only for the exact string `"true"`.
#[test]
fn values_that_are_not_allowed_read_as_their_fallbacks() {
    let output = from_saved(&saved(&[
        ("approvalPolicy", "always"),
        ("diagnosticMode", "1"),
        ("theme", "blue"),
        ("language", "fr"),
    ]));

    assert_eq!(output.approval_policy, ApprovalPolicy::OnRequest);
    assert_eq!(output.diagnostic_mode, Some(false));
    assert_eq!(output.theme, Some(AppTheme::Dark));
    assert_eq!(output.language, Some(AppLanguage::PtBr));
}

#[test]
fn an_update_is_written_as_the_strings_the_typescript_application_stores() {
    let update = SettingsUpdate {
        approval_policy: Some(ApprovalPolicy::OnRequest),
        diagnostic_mode: Some(false),
        theme: Some(AppTheme::Light),
        language: Some(AppLanguage::PtBr),
    };

    assert_eq!(
        to_saved(&update),
        saved(&[
            ("approvalPolicy", "on-request"),
            ("diagnosticMode", "false"),
            ("theme", "light"),
            ("language", "pt-BR"),
        ])
    );
}

#[test]
fn an_update_writes_only_the_fields_it_carries() {
    assert!(to_saved(&SettingsUpdate::default()).is_empty());
    assert_eq!(
        to_saved(&SettingsUpdate {
            theme: Some(AppTheme::Dark),
            ..SettingsUpdate::default()
        }),
        saved(&[("theme", "dark")])
    );
}

#[test]
fn what_is_written_reads_back_as_the_same_settings() {
    let update = SettingsUpdate {
        approval_policy: Some(ApprovalPolicy::Untrusted),
        diagnostic_mode: Some(true),
        theme: Some(AppTheme::Light),
        language: Some(AppLanguage::En),
    };

    let output = from_saved(&to_saved(&update));

    assert_eq!(output.approval_policy, ApprovalPolicy::Untrusted);
    assert_eq!(output.diagnostic_mode, Some(true));
    assert_eq!(output.theme, Some(AppTheme::Light));
    assert_eq!(output.language, Some(AppLanguage::En));
}
