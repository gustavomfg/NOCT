//! Validation runs, checked against `ValidationRepository.ts`. The row gate in
//! `scripts/verify-row-gate.mjs` reads the same rows from the TypeScript repository.

use noct_core::validation_run::{
    ValidationArtifact, ValidationKind, ValidationRun, ValidationRunStatus,
};
use noct_core::workspace_evidence::{EvidenceKind, EvidenceReference};
use noct_db::Database;
use noct_db::repositories::validation_runs::ValidationRuns;
use noct_db::repositories::workspace_evidence::WorkspaceEvidence;

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";

fn open(name: &str) -> Database {
    let db = Database::open_runtime(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) \
                 VALUES('/work/alpha','alpha','{AT}','{AT}'),('/work/beta','beta','{AT}','{AT}');\
             INSERT INTO conversations(id,title,workspace,created_at,updated_at) \
                 VALUES('conv-1','Conversa','/work/alpha','{AT}','{AT}');\
             INSERT INTO executions(id,workspace,conversation_id,prompt,mode,status,decision,started_at) \
                 VALUES('exec-1','/work/alpha','conv-1','Tarefa','build','completed','pending','{AT}');\
             INSERT INTO checkpoints(id,execution_id,workspace,phase,status,captured_at,root_path) \
                 VALUES('ck-before','exec-1','/work/alpha','before','ready','{AT}','/work/alpha'),\
                       ('ck-capturing','exec-1','/work/alpha','after','capturing','{AT}','/work/alpha');"
        ))
        .expect("fixtures");
    db
}

fn run(id: &str, execution_id: Option<&str>, started_at: &str) -> ValidationRun {
    ValidationRun {
        id: id.to_string(),
        workspace: "/work/alpha".to_string(),
        execution_id: execution_id.map(str::to_string),
        kind: ValidationKind::Typecheck,
        command: "npm".to_string(),
        args: vec!["run".to_string(), "typecheck".to_string()],
        status: ValidationRunStatus::Running,
        exit_code: None,
        duration_ms: None,
        output_summary: String::new(),
        artifacts: Vec::new(),
        started_at: started_at.to_string(),
        completed_at: None,
        error: None,
    }
}

#[test]
fn a_run_is_recorded_with_evidence_that_cites_its_ready_checkpoints() {
    let db = open("validation-create");
    let runs = ValidationRuns::new(db.connection());
    runs.create(&run("val-1", Some("exec-1"), AT))
        .expect("create");

    assert_eq!(
        runs.latest("/work/alpha").expect("latest"),
        Some(run("val-1", Some("exec-1"), AT))
    );

    let evidence = WorkspaceEvidence::new(db.connection())
        .by_source(EvidenceKind::Validation, "val-1")
        .expect("evidence")
        .expect("recorded");
    assert_eq!(evidence.execution_id.as_deref(), Some("exec-1"));
    assert_eq!(
        evidence.references,
        vec![EvidenceReference {
            kind: "before".to_string(),
            id: "ck-before".to_string(),
            payload_hash: None,
        }],
        "the capturing checkpoint is not cited"
    );
}

#[test]
fn a_run_outside_an_execution_has_no_execution_link() {
    let db = open("validation-standalone");
    let runs = ValidationRuns::new(db.connection());
    runs.create(&run("val-1", None, AT)).expect("create");

    let stored = runs
        .latest("/work/alpha")
        .expect("latest")
        .expect("present");
    assert_eq!(stored.execution_id, None);
    let json = serde_json::to_value(&stored).expect("json");
    assert!(json.get("executionId").is_none(), "{json}");

    let evidence = WorkspaceEvidence::new(db.connection())
        .by_source(EvidenceKind::Validation, "val-1")
        .expect("evidence")
        .expect("recorded");
    assert!(evidence.references.is_empty());
}

#[test]
fn runs_are_listed_newest_first_within_the_limit() {
    let db = open("validation-list");
    let runs = ValidationRuns::new(db.connection());
    runs.create(&run("val-old", None, "2026-01-01T00:00:01.000Z"))
        .expect("old");
    runs.create(&run("val-mid", None, "2026-01-01T00:00:02.000Z"))
        .expect("middle");
    runs.create(&run("val-new", None, "2026-01-01T00:00:03.000Z"))
        .expect("new");

    let ids = |listed: Vec<ValidationRun>| -> Vec<String> {
        listed.into_iter().map(|run| run.id).collect()
    };
    assert_eq!(
        ids(runs.list("/work/alpha", None).expect("list")),
        ["val-new", "val-mid", "val-old"]
    );
    assert_eq!(
        ids(runs.list("/work/alpha", Some(0)).expect("list")),
        ["val-new"],
        "a limit below one is raised to one"
    );
    assert!(runs.list("/work/beta", None).expect("list").is_empty());
}

#[test]
fn a_page_reports_whether_further_runs_follow() {
    let db = open("validation-page");
    let runs = ValidationRuns::new(db.connection());
    for (id, second) in [("val-a", 1), ("val-b", 2), ("val-c", 3)] {
        runs.create(&run(id, None, &format!("2026-01-01T00:00:0{second}.000Z")))
            .expect("create");
    }

    let first = runs.page("/work/alpha", 0, Some(2)).expect("first page");
    assert_eq!(first.items.len(), 2);
    assert!(first.has_more);

    let last = runs.page("/work/alpha", 2, Some(2)).expect("last page");
    assert_eq!(last.items.len(), 1);
    assert!(!last.has_more);
}

#[test]
fn saving_an_existing_run_updates_it_and_records_its_evidence_once() {
    let db = open("validation-save");
    let runs = ValidationRuns::new(db.connection());
    runs.create(&run("val-1", Some("exec-1"), AT))
        .expect("create");

    let mut finished = run("val-1", Some("exec-1"), AT);
    finished.status = ValidationRunStatus::Passed;
    finished.exit_code = Some(0);
    finished.duration_ms = Some(1200);
    finished.output_summary = "Sem erros.".to_string();
    finished.artifacts = vec![ValidationArtifact {
        path: "dist/report.json".to_string(),
        kind: "report".to_string(),
        size: Some(42),
    }];
    finished.completed_at = Some("2026-01-01T00:00:05.000Z".to_string());
    runs.save(&finished).expect("save");

    assert_eq!(runs.latest("/work/alpha").expect("latest"), Some(finished));
    let recorded = WorkspaceEvidence::new(db.connection())
        .list("/work/alpha", None)
        .expect("evidence");
    assert_eq!(recorded.len(), 1, "saving does not record evidence again");
}

#[test]
fn a_corrupt_stored_artifact_is_reported_by_its_position() {
    let db = open("validation-corrupt");
    let runs = ValidationRuns::new(db.connection());
    runs.create(&run("val-1", None, AT)).expect("create");
    db.connection()
        .execute(
            "UPDATE validation_runs SET artifacts_json='[1]' WHERE id='val-1'",
            [],
        )
        .expect("corrupt the row");

    let error = runs
        .latest("/work/alpha")
        .expect_err("a corrupt artifact is refused");
    assert_eq!(
        error.to_string(),
        "A execução de validação persistida possui um artefato inválido."
    );
}
