//! The rules of a proposed change that need no disk: the relative path, the sensitive names, the moves of a change, and the
//! journal's limits. The disk checks are in `noct-app/tests/file_changes.rs`.

use noct_core::file_change::{
    FileChange, FileChangeJournal, FileChangeStatus, JournalRefusal, PathRefusal,
    check_relative_path, fits_limit, is_markdown, is_sensitive_name, runs_on_build,
};
use noct_core::limits::file_change::{
    MAXIMUM_CLOSED_CHANGES, MAXIMUM_FILE_BYTES, MAXIMUM_OPEN_CHANGES,
};

fn change(id: &str, status: FileChangeStatus) -> FileChange {
    FileChange {
        id: id.to_string(),
        workspace: "/projects/one".to_string(),
        path: "src/lib.rs".to_string(),
        status,
        base: Some("old\n".to_string()),
        content: "new\n".to_string(),
    }
}

#[test]
fn a_plain_relative_path_is_accepted_with_its_names() {
    assert_eq!(
        check_relative_path("src/lib/mod.rs"),
        Ok(vec![
            "src".to_string(),
            "lib".to_string(),
            "mod.rs".to_string()
        ])
    );
}

#[test]
fn a_path_that_leaves_the_project_is_refused() {
    assert_eq!(
        check_relative_path("../outside.txt"),
        Err(PathRefusal::Traversal)
    );
    assert_eq!(
        check_relative_path("src/../../outside.txt"),
        Err(PathRefusal::Traversal)
    );
    assert_eq!(
        check_relative_path("/etc/passwd"),
        Err(PathRefusal::NotRelative)
    );
    assert_eq!(
        check_relative_path("C:/Windows/win.ini"),
        Err(PathRefusal::Forbidden)
    );
    assert_eq!(
        check_relative_path("src\\..\\x.txt"),
        Err(PathRefusal::Forbidden)
    );
}

#[test]
fn a_path_with_an_empty_dot_or_nul_name_is_refused() {
    assert_eq!(check_relative_path(""), Err(PathRefusal::Empty));
    assert_eq!(
        check_relative_path("src//lib.rs"),
        Err(PathRefusal::Forbidden)
    );
    assert_eq!(check_relative_path("./lib.rs"), Err(PathRefusal::Forbidden));
    assert_eq!(check_relative_path("src/"), Err(PathRefusal::Forbidden));
    assert_eq!(check_relative_path("a\0b.txt"), Err(PathRefusal::Forbidden));
}

#[test]
fn a_name_that_windows_would_read_differently_is_refused() {
    assert_eq!(check_relative_path("notes."), Err(PathRefusal::Forbidden));
    assert_eq!(check_relative_path("notes "), Err(PathRefusal::Forbidden));
}

#[test]
fn a_device_name_of_windows_is_refused_with_or_without_an_extension() {
    for path in [
        "CON",
        "con",
        "NUL",
        "nul.txt",
        "PRN.md",
        "aux",
        "COM1",
        "com9.log",
        "LPT1",
        "lpt3.txt",
        "src/NUL",
        "docs/Con.md",
        "COM\u{B9}",
        "nul .txt",
    ] {
        assert_eq!(
            check_relative_path(path),
            Err(PathRefusal::Forbidden),
            "{path}"
        );
    }
    for path in [
        "console.rs",
        "nullable.md",
        "com10.txt",
        "auxiliar/x.md",
        "compose.yml",
        "lpt.md",
    ] {
        assert!(check_relative_path(path).is_ok(), "{path}");
    }
}

#[test]
fn an_encoded_traversal_a_backslash_or_a_lookalike_is_refused() {
    for path in [
        "%2e%2e/x.txt",
        "a/%2E%2E/b.txt",
        "..%2fsegredo",
        "a%5cb.txt",
        "docs/%2e%2e%2f%2e%2e/x",
    ] {
        assert_eq!(
            check_relative_path(path),
            Err(PathRefusal::Traversal),
            "{path}"
        );
    }
    for path in [
        "..\\x.txt",
        "a\\b.txt",
        "src\\..\\..\\x",
        "C:\\x.txt",
        "\u{FF0E}\u{FF0E}/x",
        "a\u{2215}b",
        "a\u{FF0F}b",
        "a\u{2024}\u{2024}/b",
        "a\nb.txt",
        "a\u{7f}b",
    ] {
        assert_eq!(
            check_relative_path(path),
            Err(PathRefusal::Forbidden),
            "{path:?}"
        );
    }
    assert!(
        check_relative_path("100% pronto.md").is_ok(),
        "a percent sign alone is a name"
    );
}

#[test]
fn a_file_that_runs_by_itself_is_refused() {
    for path in [
        ".git/config",
        ".git/hooks/pre-commit",
        ".git/hooks/post-checkout",
        "sub/.git/hooks/pre-push",
        ".GIT/HEAD",
    ] {
        assert_eq!(
            check_relative_path(path),
            Err(PathRefusal::Sensitive),
            "{path}"
        );
    }
    for path in [
        ".github/workflows/ci.yml",
        ".github/workflows/release.yaml",
        ".GitHub/Workflows/x.yml",
        ".github/actions/setup/action.yml",
        "pkg/.github/workflows/ci.yml",
        ".gitlab-ci.yml",
        ".gitlab/ci/build.yml",
        ".circleci/config.yml",
        ".travis.yml",
        "azure-pipelines.yml",
        "Jenkinsfile",
        ".husky/pre-commit",
        ".githooks/commit-msg",
        ".pre-commit-config.yaml",
        "lefthook.yml",
        ".gitattributes",
        ".gitmodules",
        ".vscode/tasks.json",
        ".vscode/settings.json",
        ".idea/workspace.xml",
        ".devcontainer/devcontainer.json",
        ".envrc",
        ".exrc",
        ".nvim.lua",
    ] {
        assert_eq!(
            check_relative_path(path),
            Err(PathRefusal::RunsByItself),
            "{path}"
        );
    }
    for path in [
        ".github/ISSUE_TEMPLATE/bug.md",
        ".github/CODEOWNERS",
        ".github/workflows.md",
        "docs/workflows/ci.md",
        "src/vscode.rs",
        "README.md",
    ] {
        assert!(check_relative_path(path).is_ok(), "{path}");
    }
}

#[test]
fn a_path_over_the_limit_is_refused() {
    let long = format!("{}.txt", "a/".repeat(300));
    assert_eq!(check_relative_path(&long), Err(PathRefusal::TooLong));
}

#[test]
fn credential_files_and_folders_are_sensitive_at_any_depth() {
    for path in [
        ".env",
        "config/.env.production",
        "keys/server.pem",
        "certs/tls.key",
        "id_rsa",
        "deploy/id_ed25519",
        "config/database-credentials.json",
        "app/secret_settings.toml",
        ".npmrc",
        ".git/config",
        "nested/.git/HEAD",
        ".ssh/config",
    ] {
        assert_eq!(
            check_relative_path(path),
            Err(PathRefusal::Sensitive),
            "{path} must be refused"
        );
    }
}

#[test]
fn ordinary_names_near_the_sensitive_ones_are_allowed() {
    for path in [
        ".gitignore",
        "src/tokenizer.rs",
        "docs/keyboard.md",
        "src/id_map.rs",
        "README.md",
    ] {
        assert!(check_relative_path(path).is_ok(), "{path} must be allowed");
    }
}

#[test]
fn sensitive_names_are_matched_without_regard_to_case() {
    assert!(is_sensitive_name(".ENV"));
    assert!(is_sensitive_name("Server.PEM"));
    assert!(is_sensitive_name("Credentials.json"));
}

#[test]
fn a_change_moves_only_along_the_allowed_edges() {
    use FileChangeStatus::*;
    assert!(Pending.allows(Applied));
    assert!(Pending.allows(Rejected));
    assert!(Applied.allows(Undone));
    assert!(
        Applied.allows(Pending),
        "a failed write goes back to pending"
    );
    assert!(Undone.allows(Applied), "a failed undo goes back to applied");

    assert!(!Rejected.allows(Applied), "a rejected change is closed");
    assert!(!Undone.allows(Pending));
    assert!(!Pending.allows(Undone), "a pending change cannot be undone");
    assert!(!Applied.allows(Rejected));
    assert!(!Pending.allows(Pending));
}

#[test]
fn the_journal_refuses_an_unknown_or_forbidden_move() {
    let mut journal = FileChangeJournal::default();
    journal
        .add(change("a", FileChangeStatus::Rejected))
        .unwrap();

    assert_eq!(
        journal.set_status("missing", FileChangeStatus::Applied),
        Err(JournalRefusal::Unknown)
    );
    assert_eq!(
        journal.set_status("a", FileChangeStatus::Applied),
        Err(JournalRefusal::InvalidTransition)
    );
    assert_eq!(
        journal.find("a").unwrap().status,
        FileChangeStatus::Rejected
    );
}

#[test]
fn the_journal_refuses_a_new_change_past_the_open_limit() {
    let mut journal = FileChangeJournal::default();
    for index in 0..MAXIMUM_OPEN_CHANGES {
        journal
            .add(change(&format!("open-{index}"), FileChangeStatus::Pending))
            .unwrap();
    }
    assert_eq!(
        journal.add(change("one-more", FileChangeStatus::Pending)),
        Err(JournalRefusal::TooManyOpen)
    );
}

#[test]
fn closed_changes_are_dropped_oldest_first_and_open_ones_are_kept() {
    let mut journal = FileChangeJournal::default();
    journal
        .add(change("keep-open", FileChangeStatus::Pending))
        .unwrap();
    for index in 0..(MAXIMUM_CLOSED_CHANGES + 5) {
        journal
            .add(change(
                &format!("closed-{index}"),
                FileChangeStatus::Rejected,
            ))
            .unwrap();
    }

    let closed: Vec<&str> = journal
        .changes
        .iter()
        .filter(|item| !item.status.is_open())
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(closed.len(), MAXIMUM_CLOSED_CHANGES);
    assert_eq!(
        closed.first().copied(),
        Some("closed-5"),
        "the oldest five are dropped"
    );
    assert!(
        journal.find("keep-open").is_some(),
        "an open change is never dropped"
    );
}

#[test]
fn a_change_for_one_project_does_not_show_in_another() {
    let mut journal = FileChangeJournal::default();
    journal
        .add(change("here", FileChangeStatus::Pending))
        .unwrap();
    let mut other = change("there", FileChangeStatus::Pending);
    other.workspace = "/projects/two".to_string();
    journal.add(other).unwrap();

    let ids: Vec<String> = journal
        .for_workspace("/projects/one")
        .into_iter()
        .map(|item| item.id)
        .collect();
    assert_eq!(ids, vec!["here".to_string()]);
}

#[test]
fn the_text_limit_is_the_same_byte_count_the_disk_read_uses() {
    assert!(fits_limit(&"a".repeat(MAXIMUM_FILE_BYTES as usize)));
    assert!(!fits_limit(&"a".repeat(MAXIMUM_FILE_BYTES as usize + 1)));
}

#[test]
fn markdown_is_told_by_the_ending_of_the_path() {
    for path in ["README.md", "docs/Guia.MD", "notas.markdown"] {
        assert!(is_markdown(path), "{path}");
    }
    for path in ["src/lib.rs", "md", "docs/md.txt", "README.md.bak", "a.mdx"] {
        assert!(!is_markdown(path), "{path}");
    }
}

#[test]
fn files_that_run_on_build_or_install_are_told_and_not_refused() {
    for path in [
        "build.rs",
        "crates/x/build.rs",
        "Makefile",
        "GNUmakefile",
        "justfile",
        "package.json",
        "web/package.json",
        "Dockerfile",
        "docker/Dockerfile.dev",
        "app.dockerfile",
        "scripts/setup.sh",
        "tools/install.PS1",
        "rules.mk",
    ] {
        assert!(runs_on_build(path, None, ""), "{path}");
        assert!(check_relative_path(path).is_ok(), "{path} stays allowed");
    }
    for path in [
        "src/main.rs",
        "README.md",
        "package-lock.json",
        "src/build.rs.md",
        "docs/sh",
    ] {
        assert!(!runs_on_build(path, None, ""), "{path}");
    }
    let plain = "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n";
    assert!(!runs_on_build("Cargo.toml", Some(plain), plain));
    assert!(runs_on_build(
        "Cargo.toml",
        None,
        "[build-dependencies]\ncc = \"1\"\n"
    ));
    assert!(runs_on_build(
        "crates/a/Cargo.toml",
        Some(plain),
        "[package]\nbuild = \"gen.rs\"\n"
    ));
    assert!(
        runs_on_build("Cargo.toml", Some("[build-dependencies]\n"), plain),
        "a build part being removed is told too"
    );
    assert!(runs_on_build(
        "Cargo.toml",
        None,
        "[target.'cfg(unix)'.build-dependencies]\ncc = \"1\"\n"
    ));
}
