//! The rules that decide which project files a provider call may carry. The TypeScript rules are the reference for each
//! case; the one difference (a file named exactly after a sensitive extension) is tested on its own.

use noct_core::ai::context::{
    ContextFile, Exclusion, is_ignored_relative_path, is_inside_project,
    is_potentially_sensitive_path, select,
};
use noct_core::limits::provider_context as limits;

fn file(path: &str, bytes: u64) -> ContextFile {
    ContextFile {
        relative_path: path.to_string(),
        bytes,
    }
}

#[test]
fn a_path_through_an_ignored_directory_is_ignored_on_any_segment() {
    for path in [
        ".git/config",
        "node_modules/left-pad/index.js",
        "packages/app/dist/bundle.js",
        "src/build/out.js",
        ".nocturne/state.json",
        ".nocturne-rollback-42/snapshot",
        "crates/app/target/debug/app",
        "web/.next/cache.json",
        "services/.venv/lib/site.py",
        "a\\node_modules\\x.js",
    ] {
        assert!(is_ignored_relative_path(path), "{path} is ignored");
    }
}

#[test]
fn a_name_that_only_contains_an_ignored_word_is_not_ignored() {
    for path in [
        "src/main.rs",
        "docs/build-notes.md",
        "crates/outside/lib.rs",
        "release-notes.md",
    ] {
        assert!(!is_ignored_relative_path(path), "{path} is not ignored");
    }
}

#[test]
fn the_fixed_secret_names_are_sensitive_whatever_their_case_or_folder() {
    for path in [
        ".env",
        "config/.env.local",
        "deploy/.env.production",
        "credentials.json",
        "SECRETS.YAML",
        "ops/secrets.yml",
        "app/secrets.json",
    ] {
        assert!(is_potentially_sensitive_path(path), "{path} is sensitive");
    }
}

#[test]
fn a_name_that_contains_secret_credential_or_token_is_sensitive() {
    for path in [
        "secret-notes.md",
        "api_credentials.txt",
        "auth/token.ts",
        "docs/tokens-guide.md",
        "CredentialStore.rs",
    ] {
        assert!(is_potentially_sensitive_path(path), "{path} is sensitive");
    }
}

#[test]
fn a_sensitive_extension_is_sensitive_including_a_file_named_after_it() {
    for path in [
        "server.key",
        "cert.PEM",
        "a/b.p12",
        "keystore.pfx",
        "root.crt",
        "root.cer",
        "x.pem",
    ] {
        assert!(is_potentially_sensitive_path(path), "{path} is sensitive");
    }
    // The divergence from the TypeScript rule: a file named exactly after an extension is sensitive here.
    assert!(is_potentially_sensitive_path(".pem"));
    assert!(is_potentially_sensitive_path(".key"));
}

#[test]
fn ordinary_source_and_text_files_are_not_sensitive() {
    for path in [
        "src/main.rs",
        "README.md",
        "keyboard.rs",
        "docs/keys-overview.md",
        "package.json",
        ".gitignore",
    ] {
        assert!(
            !is_potentially_sensitive_path(path),
            "{path} is not sensitive"
        );
    }
}

#[test]
fn a_backslash_path_is_read_as_its_slash_form() {
    assert!(is_potentially_sensitive_path("config\\.env"));
    assert!(is_ignored_relative_path("src\\node_modules\\x.js"));
}

#[test]
fn a_path_must_name_a_place_inside_the_project() {
    assert!(is_inside_project("src/main.rs"));
    for path in [
        "",
        "/etc/passwd",
        "../secret.txt",
        "src/../../x",
        "C:/Users/x",
        "src//x",
        "a\\..\\b",
    ] {
        assert!(!is_inside_project(path), "{path:?} is outside the project");
    }
}

#[test]
fn a_selection_keeps_the_order_and_excludes_each_file_with_its_reason() {
    let selection = select(&[
        file("src/main.rs", 1_000),
        file(".env", 10),
        file("node_modules/x.js", 10),
        file("../outside.txt", 10),
        file("docs/readme.md", 500),
    ]);

    assert_eq!(selection.included, vec!["src/main.rs", "docs/readme.md"]);
    assert_eq!(selection.bytes, 1_500);
    assert_eq!(
        selection.excluded,
        vec![
            (".env".to_string(), Exclusion::Sensitive),
            ("node_modules/x.js".to_string(), Exclusion::Ignored),
            ("../outside.txt".to_string(), Exclusion::OutsideProject),
        ]
    );
}

#[test]
fn a_sensitive_file_is_never_included_whatever_its_size() {
    let selection = select(&[file("credentials.json", 1)]);
    assert!(selection.included.is_empty());
    assert_eq!(
        selection.excluded,
        vec![("credentials.json".to_string(), Exclusion::Sensitive)]
    );
}

#[test]
fn a_file_over_the_per_file_cap_is_excluded_and_does_not_use_the_budget() {
    let too_large = limits::MAXIMUM_FILE_BYTES + 1;
    let selection = select(&[file("big.rs", too_large), file("small.rs", 100)]);

    assert_eq!(
        selection.excluded,
        vec![("big.rs".to_string(), Exclusion::TooLarge)]
    );
    assert_eq!(selection.included, vec!["small.rs"]);
    assert_eq!(selection.bytes, 100);
}

#[test]
fn the_selection_never_passes_the_budget_and_stops_at_the_first_file_that_does_not_fit() {
    let each = limits::MAXIMUM_FILE_BYTES;
    let files: Vec<ContextFile> = (0..10)
        .map(|index| file(&format!("f{index}.rs"), each))
        .collect();
    let selection = select(&files);

    assert!(selection.bytes <= limits::MAXIMUM_CONTEXT_BYTES);
    assert_eq!(
        selection.bytes,
        limits::MAXIMUM_CONTEXT_BYTES,
        "four files of the cap fill the budget exactly"
    );
    assert_eq!(selection.included.len(), 4);
    assert!(
        selection
            .excluded
            .iter()
            .all(|(_, reason)| *reason == Exclusion::OverBudget)
    );
    assert_eq!(selection.excluded.len(), 6);
}

#[test]
fn a_smaller_later_file_is_not_sent_once_the_budget_has_stopped_the_selection() {
    // Three files of the per-file cap, then one that leaves ten bytes of budget. Each file is within the cap, so only
    // the budget decides.
    let cap = limits::MAXIMUM_FILE_BYTES;
    let mut files = vec![
        file("a.rs", cap),
        file("b.rs", cap),
        file("c.rs", cap),
        file("d.rs", cap - 10),
    ];
    files.push(file("e.rs", 20));
    files.push(file("f.rs", 5));
    let selection = select(&files);

    assert_eq!(selection.included, vec!["a.rs", "b.rs", "c.rs", "d.rs"]);
    assert_eq!(selection.bytes, limits::MAXIMUM_CONTEXT_BYTES - 10);
    assert_eq!(
        selection.excluded,
        vec![
            ("e.rs".to_string(), Exclusion::OverBudget),
            ("f.rs".to_string(), Exclusion::OverBudget),
        ],
        "f.rs would fit in the ten bytes left, and it is still not sent"
    );
}

#[test]
fn the_exclusion_codes_are_stable() {
    let codes = [
        Exclusion::OutsideProject,
        Exclusion::Ignored,
        Exclusion::Sensitive,
        Exclusion::TooLarge,
        Exclusion::OverBudget,
    ]
    .map(|reason| reason.code());
    assert_eq!(
        codes,
        [
            "outside-project",
            "ignored",
            "sensitive",
            "too-large",
            "over-budget"
        ]
    );
}

#[test]
fn the_caps_are_the_beta_one_values() {
    assert_eq!(limits::MAXIMUM_CONTEXT_BYTES, 128 * 1024);
    assert_eq!(limits::MAXIMUM_FILE_BYTES, 32 * 1024);
}
