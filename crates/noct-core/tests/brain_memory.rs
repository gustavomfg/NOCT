//! Brain memory contract, checked against `shared/brainMemory.ts`. The row gate compares the
//! same credential answers with the TypeScript filter on a larger set of inputs.

use noct_core::brain_memory::{fts_query, is_safe_brain_memory_content};

#[test]
fn credentials_are_detected_at_the_typescript_thresholds() {
    let cases = [
        ("Usar SQLite local para persistência.", true),
        ("", true),
        ("Authorization: Bearer abcdefgh", false),
        ("authorization:abcdefgh", false),
        ("authorization: short", true),
        ("api_key = 123456789012", false),
        ("api-key=12345678901", true),
        ("senha=abcdefghijkl", false),
        ("senha=abcdefghijk", true),
        ("sk-abcdefghijklmnop", false),
        ("sk-abcdefghijklmno", true),
        ("GHP_abcdefghijklmnopqrst", false),
        ("ghp_abcdefghijklmnopqrs", true),
        ("bearer token only", true),
    ];
    for (input, safe) in cases {
        assert_eq!(is_safe_brain_memory_content(input), safe, "{input:?}");
    }
}

#[test]
fn credential_lengths_count_utf16_units_and_bmp_characters() {
    // Six emoji are twelve UTF-16 units, the threshold of `\S{12,}`. Five are ten.
    assert!(!is_safe_brain_memory_content("token=😀😀😀😀😀😀"));
    assert!(is_safe_brain_memory_content("token=😀😀😀😀😀"));
    // Twelve BMP characters pass the same threshold.
    assert!(!is_safe_brain_memory_content(
        "password: 漢漢漢漢漢漢漢漢漢漢漢漢"
    ));
    assert!(is_safe_brain_memory_content(
        "password: 漢漢漢漢漢漢漢漢漢漢漢"
    ));
}

#[test]
fn whitespace_is_javascript_whitespace() {
    // NBSP and U+FEFF are `\s` in JavaScript, U+0085 and U+180E are not.
    assert!(!is_safe_brain_memory_content(
        "token\u{00A0}=\u{00A0}aaaaaaaaaaaa"
    ));
    assert!(!is_safe_brain_memory_content(
        "token\u{FEFF}=\u{FEFF}aaaaaaaaaaaa"
    ));
    assert!(!is_safe_brain_memory_content(
        "token\u{2028}=\u{2028}aaaaaaaaaaaa"
    ));
    assert!(is_safe_brain_memory_content(
        "token\u{0085}=aaaaaaaaaaaaaaaa"
    ));
    assert!(is_safe_brain_memory_content("token\u{180E}=aaaaaaaaaaaa"));
}

#[test]
fn case_folding_is_ascii_only() {
    // The Kelvin sign and the long s fold onto k and s only under Unicode case folding.
    assert!(is_safe_brain_memory_content("api\u{212A}ey=123456789012"));
    assert!(is_safe_brain_memory_content("ſk-abcdefghijklmnop"));
    assert!(!is_safe_brain_memory_content("SK-abcdefghijklmnop"));
}

#[test]
fn search_expression_quotes_normalized_tokens() {
    assert_eq!(
        fts_query("SQLite persistência"),
        "\"SQLite\"* OR \"persistência\"*"
    );
    // NFKC folds the fi ligature before the tokens are found.
    assert_eq!(fts_query("\u{FB01}le"), "\"file\"*");
    assert_eq!(fts_query("foo-bar_baz"), "\"foo-bar_baz\"*");
    assert_eq!(fts_query("say \"hello\""), "\"say\"* OR \"hello\"*");
}

#[test]
fn search_expression_needs_two_character_tokens_and_keeps_twelve() {
    assert_eq!(fts_query("a b c"), "");
    assert_eq!(fts_query(""), "");
    let thirteen = (0..13)
        .map(|index| format!("t{index:02}"))
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(fts_query(&thirteen).matches(" OR ").count(), 11);
}
