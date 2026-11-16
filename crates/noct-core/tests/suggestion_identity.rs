//! The suggestion identity and title sanitizing, checked against `shared/suggestions.ts`.
//! The row gate compares the same values with the TypeScript functions.

use noct_core::suggestions::{SuggestionCategory, sanitize_suggestion_title, suggestion_identity};

#[test]
fn identity_ignores_case_accents_punctuation_and_spacing() {
    let identity = suggestion_identity(SuggestionCategory::Bug, "Ação: revisar  ‘Config’");

    assert_eq!(identity, "bug:acao revisar config");
    assert_eq!(
        suggestion_identity(SuggestionCategory::Bug, "acao revisar config"),
        identity
    );
    assert_eq!(
        suggestion_identity(SuggestionCategory::Bug, "  AÇÃO — revisar\tconfig!  "),
        identity
    );
}

#[test]
fn compatibility_forms_decompose_before_comparison() {
    assert_eq!(
        suggestion_identity(SuggestionCategory::Bug, "ﬁle"),
        "bug:file"
    );
}

#[test]
fn numbers_are_kept_and_other_symbols_separate_words() {
    assert_eq!(
        suggestion_identity(SuggestionCategory::Testing, "v2 é/ok"),
        "testing:v2 e ok"
    );
}

#[test]
fn the_category_is_part_of_the_identity() {
    assert_ne!(
        suggestion_identity(SuggestionCategory::Bug, "Mesmo título"),
        suggestion_identity(SuggestionCategory::Security, "Mesmo título")
    );
}

#[test]
fn a_title_without_letters_or_numbers_has_an_empty_identity() {
    assert_eq!(
        suggestion_identity(SuggestionCategory::Cleanup, "!!! ---"),
        "cleanup:"
    );
}

#[test]
fn sanitizing_turns_controls_and_runs_of_whitespace_into_single_spaces() {
    assert_eq!(
        sanitize_suggestion_title("a\u{0007}\tb\u{2028}c  "),
        "a b c"
    );
    assert_eq!(sanitize_suggestion_title("\u{feff}  "), "");
}
