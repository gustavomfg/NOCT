//! What becomes of the findings of a complete chat answer. Each way a block can fail is named, and a stored finding is
//! counted, so no answer is silent about its findings. The store is a scratch one, as in the service tests.

mod common;

use common::Scratch;
use noct_app::bootstrap;
use noct_app::suggestion_analysis::{FindingsStatus, store_findings};
use noct_core::limits::suggestion_analysis::MAXIMUM_TEXT_CHARACTERS;

fn finding(title: &str) -> String {
    format!(
        "{{\"title\":\"{title}\",\"description\":\"d\",\"reasoning\":\"r\",\"category\":\"bug\",\"severity\":\"medium\"}}"
    )
}

/// A finding whose description is over the bound, so the shell refuses it and keeps the others.
fn too_long(title: &str) -> String {
    format!(
        "{{\"title\":\"{title}\",\"description\":\"{}\",\"reasoning\":\"r\",\"category\":\"bug\",\"severity\":\"medium\"}}",
        "x".repeat(MAXIMUM_TEXT_CHARACTERS + 1)
    )
}

fn answer(items: &[String]) -> String {
    format!(
        "Há um problema no projeto.\n```nocturne-suggestions\n[{}]\n```",
        items.join(",")
    )
}

#[test]
fn each_way_a_block_fails_is_named_and_a_stored_finding_is_counted() {
    let scratch = Scratch::new("findings-report");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let report = |text: &str| store_findings(&store, &[], &path, text, &[]);

    let absent = report("Há um bug, mas sem bloco.");
    assert_eq!(absent.status, FindingsStatus::Absent);
    assert_eq!(absent.saved, 0);

    let invalid = report("Texto.\n```nocturne-suggestions\n[{\"title\": \"A\",]\n```");
    assert_eq!(invalid.status, FindingsStatus::Invalid);

    let empty = report(&answer(&[]));
    assert_eq!(empty.status, FindingsStatus::Empty);
    assert_eq!(empty.discarded, 0);

    let refused = report(&answer(&[too_long("Longa")]));
    assert_eq!(refused.status, FindingsStatus::Discarded);
    assert_eq!(refused.saved, 0);
    assert_eq!(refused.discarded, 1);

    let stored = report(&answer(&[finding("Leitura sem erro"), too_long("Longa")]));
    assert_eq!(stored.status, FindingsStatus::Saved);
    assert_eq!(stored.saved, 1);
    assert_eq!(stored.discarded, 1);

    let again = report(&answer(&[finding("Leitura sem erro")]));
    assert_eq!(again.status, FindingsStatus::Saved);
    assert_eq!(again.saved, 1, "a known finding is updated, and counted");
    assert_eq!(again.discarded, 0);
}

#[test]
fn findings_for_a_project_that_is_not_saved_fail_and_say_so() {
    let scratch = Scratch::new("findings-unsaved");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let report = store_findings(
        &store,
        &[],
        "/caminho/que/nao/existe",
        &answer(&[finding("Qualquer")]),
        &[],
    );
    assert_eq!(report.status, FindingsStatus::Failed);
    assert_eq!(report.saved, 0);
}
