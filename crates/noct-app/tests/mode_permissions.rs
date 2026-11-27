//! One test per permission rule of the modes. The shell applies each rule when an answer records its changes. Nothing
//! here reads an agent's instruction or model, so no stored value can change a result.

mod common;

use std::path::Path;

use common::Scratch;
use noct_app::bootstrap::{self, Persistence};
use noct_app::file_changes;
use noct_app::provider_chat::{self, Proposals};
use noct_core::agent::AgentMode;
use noct_core::agents::MARKDOWN_ONLY_NOTICE;

fn open(scratch: &Scratch) -> Persistence {
    bootstrap::open(&scratch.path().join("data")).expect("the shell opens")
}

/// A real folder in the scratch directory, saved and authorized as a project.
fn project(scratch: &Scratch, persistence: &Persistence, name: &str) -> String {
    let folder = scratch.path().join(name);
    std::fs::create_dir_all(&folder).expect("the folder is created");
    let path = folder.to_string_lossy().into_owned();
    persistence
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspaces()
        .touch(&path)
        .expect("the project is saved");
    path
}

/// An answer with one complete block per (path, content).
fn answer(blocks: &[(&str, &str)]) -> String {
    blocks
        .iter()
        .map(|(path, content)| format!("````noct-change path={path}\n{content}\n````\n"))
        .collect()
}

#[test]
fn revisao_records_no_pending_change_even_when_its_answer_holds_blocks() {
    let scratch = Scratch::new("mode-review");
    let persistence = open(&scratch);
    let workspace = project(&scratch, &persistence, "app");
    std::fs::write(Path::new(&workspace).join("README.md"), "antigo\n").expect("written");
    let text = answer(&[("README.md", "novo\n"), ("notas.md", "nota\n")]);

    let recorded =
        provider_chat::record_proposals(&persistence, &[], &workspace, AgentMode::Review, &text);

    assert_eq!(recorded, Proposals::default(), "Revisão records nothing");
    assert_eq!(recorded.event(), None, "and tells the window nothing");
    assert!(
        file_changes::list(&persistence, &[], &workspace)
            .expect("the changes are listed")
            .is_empty(),
        "no pending change exists"
    );
    assert_eq!(
        std::fs::read_to_string(Path::new(&workspace).join("README.md")).expect("read"),
        "antigo\n",
        "and the project is untouched"
    );
}

#[test]
fn documentacao_refuses_every_path_that_is_not_markdown_with_a_clear_notice() {
    let scratch = Scratch::new("mode-docs");
    let persistence = open(&scratch);
    let workspace = project(&scratch, &persistence, "app");
    std::fs::write(Path::new(&workspace).join("README.md"), "antigo\n").expect("written");
    let text = answer(&[
        ("README.md", "novo\n"),
        ("src/lib.rs", "codigo\n"),
        ("notas.markdown", "nota\n"),
        ("Cargo.toml", "[package]\n"),
    ]);

    let recorded =
        provider_chat::record_proposals(&persistence, &[], &workspace, AgentMode::Docs, &text);

    let paths: Vec<&str> = recorded.changes.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, ["README.md", "notas.markdown"]);
    assert_eq!(
        recorded.refused, 2,
        "the two non-Markdown blocks are refused"
    );
    assert_eq!(recorded.notice, Some(MARKDOWN_ONLY_NOTICE));
    let listed: Vec<String> = file_changes::list(&persistence, &[], &workspace)
        .expect("the changes are listed")
        .into_iter()
        .map(|change| change.path)
        .collect();
    assert_eq!(listed, ["README.md", "notas.markdown"]);
    assert!(
        !Path::new(&workspace).join("src").exists(),
        "a refused path is not created"
    );
}

#[test]
fn construcao_keeps_every_path_refusal_of_the_shared_rules_and_takes_any_other_path() {
    let scratch = Scratch::new("mode-build");
    let persistence = open(&scratch);
    let workspace = project(&scratch, &persistence, "app");
    let text = answer(&[
        ("notas.txt", "x\n"),
        ("../fora.txt", "x\n"),
        ("/tmp/absoluto.txt", "x\n"),
        ("docs/readme.md", "x\n"),
    ]);

    let recorded =
        provider_chat::record_proposals(&persistence, &[], &workspace, AgentMode::Build, &text);

    let paths: Vec<&str> = recorded.changes.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(
        paths,
        ["notas.txt"],
        "a path outside the folder is never proposed"
    );
    assert_eq!(
        recorded.refused, 3,
        "the two escaping paths and the missing folder are refused"
    );
    assert_eq!(
        recorded.notice, None,
        "Construção has no mode notice of its own"
    );
    assert!(!scratch.path().join("fora.txt").exists());
}
