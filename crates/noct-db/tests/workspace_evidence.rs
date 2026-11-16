//! Workspace evidence, checked against `WorkspaceEvidenceRepository.ts`. The gate in
//! `scripts/verify-row-gate.mjs` compares the same reads against TypeScript output.

use noct_core::workspace_evidence::{
    EvidenceKind, EvidenceReference, EvidenceVerification, KnownWorkspacePath, Validity,
};
use noct_db::Database;
use noct_db::repositories::workspace_evidence::{EvidenceInput, WorkspaceEvidence};
use rusqlite::params;

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";

fn open(name: &str) -> Database {
    let db = Database::open(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) \
                 VALUES('/work/alpha','alpha','{AT}','{AT}'),('/work/beta','beta','{AT}','{AT}');"
        ))
        .expect("workspaces");
    db
}

fn input(kind: EvidenceKind, workspace: &str, source_id: &str) -> EvidenceInput {
    EvidenceInput {
        kind,
        workspace: workspace.to_string(),
        source_id: source_id.to_string(),
        execution_id: None,
        started_at: Some(AT.to_string()),
        paths: Vec::new(),
        references: Vec::new(),
    }
}

fn path(name: &str) -> KnownWorkspacePath {
    KnownWorkspacePath {
        path: name.to_string(),
        hash: Some("a".repeat(64)),
        exists: None,
    }
}

fn reference_to(id: &str) -> EvidenceReference {
    EvidenceReference {
        kind: "manifest".to_string(),
        id: id.to_string(),
        payload_hash: None,
    }
}

#[test]
fn a_source_is_recorded_once_and_refused_for_another_workspace() {
    let db = open("evidence-identity");
    let evidence = WorkspaceEvidence::new(db.connection());

    let first = evidence
        .record(input(EvidenceKind::Before, "/work/alpha", "ck-1"))
        .expect("record");
    let again = evidence
        .record(input(EvidenceKind::Before, "/work/alpha", "ck-1"))
        .expect("same source");
    assert_eq!(again.id, first.id);
    assert_eq!(first.validity, Validity::Unknown);
    assert!(!first.truncated);

    let error = evidence
        .record(input(EvidenceKind::Before, "/work/beta", "ck-1"))
        .expect_err("other workspace");
    assert_eq!(
        error.to_string(),
        "Identidade de evidência já pertence a outro workspace ou execução."
    );
    assert_eq!(evidence.list("/work/beta", None).expect("list"), vec![]);
}

#[test]
fn oversized_paths_and_references_are_cut_and_flagged() {
    let db = open("evidence-truncation");
    let evidence = WorkspaceEvidence::new(db.connection());
    let mut long = input(EvidenceKind::ProjectIndex, "/work/alpha", "pix-1");
    long.paths = (0..2_001)
        .map(|index| path(&format!("f{index}.ts")))
        .collect();

    let record = evidence.record(long).expect("record");

    assert!(record.truncated);
    assert_eq!(record.paths.len(), 2_000);
}

#[test]
fn a_stale_reference_makes_the_citing_manifest_stale() {
    let db = open("evidence-stale-chain");
    let evidence = WorkspaceEvidence::new(db.connection());
    let parent = evidence
        .record(input(EvidenceKind::ProjectIndex, "/work/alpha", "pix-1"))
        .expect("parent");
    let mut child_input = input(EvidenceKind::Before, "/work/alpha", "ck-1");
    child_input.references = vec![reference_to(&parent.id)];
    let child = evidence.record(child_input).expect("child");

    evidence
        .mark_stale(&parent.id, "Fonte alterada.")
        .expect("mark parent");
    let listed = evidence.list("/work/alpha", None).expect("list");

    let stored_child = listed
        .iter()
        .find(|manifest| manifest.id == child.id)
        .expect("child listed");
    assert_eq!(stored_child.validity, Validity::Stale);
    assert_eq!(
        stored_child.reason.as_deref(),
        Some(format!("Evidência referenciada está stale: {}", parent.id).as_str()),
    );
    // The stale state is stored, so a later read without the chain agrees.
    let again = evidence
        .by_source(EvidenceKind::Before, "ck-1")
        .expect("by source")
        .expect("present");
    assert_eq!(again.validity, Validity::Stale);
}

#[test]
fn cyclic_references_terminate() {
    let db = open("evidence-cycle");
    let evidence = WorkspaceEvidence::new(db.connection());
    let first = evidence
        .record(input(EvidenceKind::Context, "/work/alpha", "ctx-1"))
        .expect("first");
    let mut second_input = input(EvidenceKind::Decision, "/work/alpha", "dec-1");
    second_input.references = vec![reference_to(&first.id)];
    let second = evidence.record(second_input).expect("second");
    db.connection()
        .execute(
            "UPDATE workspace_evidence SET manifest_json=json_set(manifest_json,'$.references',json(?1)) WHERE id=?2",
            params![
                format!(r#"[{{"kind":"manifest","id":"{}"}}]"#, second.id),
                first.id
            ],
        )
        .expect("close the cycle");

    let listed = evidence.list("/work/alpha", None).expect("list terminates");

    assert_eq!(listed.len(), 2);
}

#[test]
fn invalidation_keeps_the_first_reason_of_each_manifest() {
    let db = open("evidence-invalidate");
    let evidence = WorkspaceEvidence::new(db.connection());
    let first = evidence
        .record(input(EvidenceKind::Context, "/work/alpha", "ctx-1"))
        .expect("first");
    evidence
        .invalidate("/work/alpha", "Arquivo alterado.")
        .expect("invalidate");
    evidence
        .invalidate("/work/alpha", "Outra alteração.")
        .expect("invalidate again");

    let stored = evidence
        .by_source(EvidenceKind::Context, "ctx-1")
        .expect("by source")
        .expect("present");

    assert_eq!(stored.id, first.id);
    assert_eq!(stored.validity, Validity::Stale);
    assert_eq!(stored.reason.as_deref(), Some("Arquivo alterado."));
}

#[test]
fn a_saved_verification_is_read_back_with_the_manifest() {
    let db = open("evidence-verification");
    let evidence = WorkspaceEvidence::new(db.connection());
    let record = evidence
        .record(input(EvidenceKind::After, "/work/alpha", "ck-2"))
        .expect("record");
    let verification = EvidenceVerification {
        checked_at: AT.to_string(),
        matched_paths: 3,
        mismatched_paths: 1,
        unchecked_paths: 0,
    };

    evidence
        .save_verification(&record.id, &verification)
        .expect("save");

    let stored = evidence
        .by_source(EvidenceKind::After, "ck-2")
        .expect("by source")
        .expect("present");
    assert_eq!(stored.verification, Some(verification));
}

#[test]
fn project_index_evidence_lists_only_included_files() {
    let db = open("evidence-project-index");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO project_index_files(workspace,relative_path,classification,language,extension,size,\
                 mtime_ms,ctime_ms,mode,analyzed_hash,state,excluded,discovered_at) VALUES\
                 ('/work/alpha','src/a.ts','source','typescript','.ts',1,1,1,420,'{hash}','indexed',0,'{AT}'),\
                 ('/work/alpha','src/b.ts','source','typescript','.ts',1,1,1,420,NULL,'excluded',1,'{AT}');",
            hash = "b".repeat(64),
        ))
        .expect("project files");
    let evidence = WorkspaceEvidence::new(db.connection());

    let record = evidence
        .record_project_index("/work/alpha", "pix-1", AT)
        .expect("record");

    assert_eq!(
        record.paths,
        vec![KnownWorkspacePath {
            path: "src/a.ts".to_string(),
            hash: Some("b".repeat(64)),
            exists: None,
        }],
    );
}

#[test]
fn index_references_point_at_the_latest_manifest_of_each_index() {
    let db = open("evidence-index-references");
    let evidence = WorkspaceEvidence::new(db.connection());
    evidence
        .record(input(EvidenceKind::ProjectIndex, "/work/alpha", "pix-old"))
        .expect("old project index");
    let latest = evidence
        .record(input(EvidenceKind::ProjectIndex, "/work/alpha", "pix-new"))
        .expect("new project index");

    let references = evidence
        .index_references("/work/alpha")
        .expect("references");

    assert_eq!(references, vec![reference_to(&latest.id)]);
}
