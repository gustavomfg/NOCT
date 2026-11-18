//! The artifacts of an assistant reply, checked against `saveAssistant` and `artifactType` in
//! `electron/ipc/registerAiIpc.ts` and `electron/ipc/fileAccess.ts`.

use noct_core::ai::turn::{TurnArtifact, artifact_kind, basename, turn_artifacts};
use serde_json::json;

fn file(kind: &'static str, path: &str) -> TurnArtifact {
    TurnArtifact {
        kind,
        title: basename(path).to_string(),
        file_path: Some(path.to_string()),
        content: None,
    }
}

#[test]
fn each_extension_group_has_its_kind_and_the_others_are_code() {
    let cases = [
        ("a/logo.PNG", "image"),
        ("a/photo.jpeg", "image"),
        ("a/icon.svg", "image"),
        ("docs/readme.md", "markdown"),
        ("config/app.TOML", "configuration"),
        ("config/app.yml", "configuration"),
        ("config/.env", "code"),
        ("report/summary.pdf", "document"),
        ("report/page.html", "document"),
        ("src/main.rs", "code"),
        ("README", "code"),
        (".md", "code"),
        ("archive.md.gz", "code"),
    ];
    for (path, kind) in cases {
        assert_eq!(artifact_kind(path), kind, "{path}");
    }
}

#[test]
fn basename_drops_trailing_separators_and_keeps_dot_names() {
    assert_eq!(basename("src/app.ts"), "app.ts");
    assert_eq!(basename("src/dir/"), "dir");
    assert_eq!(basename("app.ts"), "app.ts");
    assert_eq!(basename("/"), "");
    assert_eq!(basename(".."), "..");
}

#[test]
fn each_listed_file_with_a_path_becomes_a_file_artifact_and_the_rest_is_skipped() {
    let metadata = json!({
        "files": [
            { "path": "src/app.ts" },
            { "path": "docs/readme.md", "lines": 12 },
            "not an object",
            null,
            ["an", "array"],
            { "path": "" },
            { "name": "no path" },
            { "path": 7 },
        ]
    });
    let artifacts = turn_artifacts(Some(&metadata));
    assert_eq!(
        artifacts,
        vec![
            file("code", "src/app.ts"),
            file("markdown", "docs/readme.md")
        ]
    );
}

#[test]
fn a_non_empty_diff_is_a_report_and_an_empty_one_is_nothing() {
    let report = turn_artifacts(Some(&json!({ "diff": "--- a\n+++ b" })));
    assert_eq!(
        report,
        vec![TurnArtifact {
            kind: "report",
            title: "Alterações do turno".to_string(),
            file_path: None,
            content: Some("--- a\n+++ b".to_string()),
        }]
    );
    assert!(turn_artifacts(Some(&json!({ "diff": "" }))).is_empty());
    assert!(turn_artifacts(Some(&json!({ "diff": 3 }))).is_empty());
}

#[test]
fn a_metadata_that_is_not_an_object_or_has_no_files_gives_no_artifact() {
    assert!(turn_artifacts(None).is_empty());
    assert!(
        turn_artifacts(Some(&json!(["src/app.ts"]))).is_empty(),
        "an array is not read"
    );
    assert!(turn_artifacts(Some(&json!("src/app.ts"))).is_empty());
    assert!(
        turn_artifacts(Some(&json!({ "files": "src/app.ts" }))).is_empty(),
        "files must be a list"
    );
    assert!(turn_artifacts(Some(&json!({ "other": [{ "path": "x.md" }] }))).is_empty());
}

#[test]
fn files_come_before_the_diff_report() {
    let metadata = json!({ "files": [{ "path": "a.md" }], "diff": "d" });
    let kinds: Vec<&str> = turn_artifacts(Some(&metadata))
        .iter()
        .map(|artifact| artifact.kind)
        .collect();
    assert_eq!(kinds, vec!["markdown", "report"]);
}
