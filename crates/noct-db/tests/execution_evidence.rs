//! Execution evidence (commands, errors and validation links), checked against
//! `ExecutionEvidenceRepository.ts`. The row gate compares the same reads with the rows the
//! TypeScript repository writes.

use noct_core::change_control::{
    ExecutionCommandRecord, ExecutionCommandSource, ExecutionCommandStatus, ExecutionErrorRecord,
    ExecutionErrorStage, ExecutionValidationLink, ExecutionValidationPhase,
};
use noct_core::text::utf16_len;
use noct_db::Database;
use noct_db::repositories::execution_evidence::ExecutionEvidence;

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";

fn open(name: &str) -> Database {
    let db = Database::open(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) \
                 VALUES('/work/alpha','alpha','{AT}','{AT}');\
             INSERT INTO conversations(id,title,workspace,created_at,updated_at) \
                 VALUES('conv-1','Conversa','/work/alpha','{AT}','{AT}');\
             INSERT INTO executions(id,workspace,conversation_id,prompt,mode,status,decision,started_at) \
                 VALUES('exec-1','/work/alpha','conv-1','Tarefa','build','completed','pending','{AT}');\
             INSERT INTO checkpoints(id,execution_id,workspace,phase,status,captured_at,root_path) \
                 VALUES('set-1-before','exec-1','/work/alpha','before','ready','{AT}','/work/alpha'),\
                       ('set-1-after','exec-1','/work/alpha','after','ready','{AT}','/work/alpha');\
             INSERT INTO change_sets(id,execution_id,before_checkpoint_id,after_checkpoint_id,status,created_at,updated_at) \
                 VALUES('set-1','exec-1','set-1-before','set-1-after','pending','{AT}','{AT}');\
             INSERT INTO changes(id,change_set_id,execution_id,relative_path,operation,origin,status,validation_status,created_at,updated_at) \
                 VALUES('chg-1','set-1','exec-1','src/a.ts','modify','agent-file-change','pending','unknown','{AT}','{AT}');\
             INSERT INTO validation_runs(id,workspace,execution_id,kind,command,args_json,status,output_summary,artifacts_json,started_at) \
                 VALUES('val-1','/work/alpha','exec-1','test','npm','[]','passed','','[]','{AT}');"
        ))
        .expect("fixtures");
    db
}

fn command(id: &str, started_at: &str) -> ExecutionCommandRecord {
    ExecutionCommandRecord {
        id: id.to_string(),
        execution_id: "exec-1".to_string(),
        command: "npm".to_string(),
        args: vec!["run".to_string(), "test".to_string()],
        source: ExecutionCommandSource::Validation,
        status: ExecutionCommandStatus::Running,
        exit_code: None,
        duration_ms: None,
        output_summary: String::new(),
        started_at: started_at.to_string(),
        finished_at: None,
    }
}

fn error(id: &str, message: &str, path: Option<&str>) -> ExecutionErrorRecord {
    ExecutionErrorRecord {
        id: id.to_string(),
        execution_id: "exec-1".to_string(),
        stage: ExecutionErrorStage::Validation,
        message: message.to_string(),
        path: path.map(str::to_string),
        created_at: AT.to_string(),
    }
}

fn link(change_id: Option<&str>, phase: ExecutionValidationPhase) -> ExecutionValidationLink {
    ExecutionValidationLink {
        execution_id: "exec-1".to_string(),
        change_id: change_id.map(str::to_string),
        validation_id: "val-1".to_string(),
        phase,
    }
}

#[test]
fn commands_keep_bounded_text_and_are_listed_in_start_order() {
    let db = open("evidence-commands");
    let evidence = ExecutionEvidence::new(db.connection());
    evidence
        .create_command(&command("cmd-late", "2026-01-01T00:00:09.000Z"))
        .expect("late command");
    let mut long = command("cmd-early", "2026-01-01T00:00:01.000Z");
    long.command = "c".repeat(5_000);
    long.args = (0..101)
        .map(|index| {
            if index == 0 {
                "a".repeat(4_200)
            } else {
                format!("arg-{index}")
            }
        })
        .collect();
    evidence.create_command(&long).expect("early command");

    let listed = evidence.list_commands("exec-1", None).expect("list");

    let ids: Vec<&str> = listed.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(ids, ["cmd-early", "cmd-late"]);
    assert_eq!(utf16_len(&listed[0].command), 4_096);
    assert_eq!(
        listed[0].args.len(),
        100,
        "only the first 100 arguments are kept"
    );
    assert_eq!(utf16_len(&listed[0].args[0]), 4_096);
}

#[test]
fn a_saved_command_is_updated_in_place() {
    let db = open("evidence-command-save");
    let evidence = ExecutionEvidence::new(db.connection());
    evidence
        .save_command(&command("cmd-1", AT))
        .expect("first save");

    let mut finished = command("cmd-1", AT);
    finished.status = ExecutionCommandStatus::Passed;
    finished.exit_code = Some(0);
    finished.duration_ms = Some(1200);
    finished.output_summary = "Sem erros.".to_string();
    finished.finished_at = Some("2026-01-01T00:00:02.000Z".to_string());
    evidence.save_command(&finished).expect("second save");

    assert_eq!(
        evidence.list_commands("exec-1", None).expect("list"),
        vec![finished]
    );
}

#[test]
fn command_listings_default_to_one_hundred_and_stop_at_five_hundred() {
    let db = open("evidence-command-limits");
    let evidence = ExecutionEvidence::new(db.connection());
    for index in 0..101 {
        evidence
            .create_command(&command(
                &format!("cmd-{index:03}"),
                &format!("2026-01-01T00:00:{:02}.{index:03}Z", index / 1_000),
            ))
            .expect("command");
    }

    assert_eq!(
        evidence
            .list_commands("exec-1", None)
            .expect("default")
            .len(),
        100
    );
    assert_eq!(
        evidence
            .list_commands("exec-1", Some(1_000))
            .expect("clamped")
            .len(),
        101,
        "the largest accepted limit is 500, and 101 commands exist"
    );
    assert_eq!(
        evidence
            .list_commands("exec-1", Some(0))
            .expect("minimum")
            .len(),
        1
    );
}

#[test]
fn errors_keep_their_message_and_path_within_the_stored_limits() {
    let db = open("evidence-errors");
    let evidence = ExecutionEvidence::new(db.connection());
    evidence
        .create_error(&error(
            "err-1",
            &"m".repeat(500_010),
            Some(&"p".repeat(4_200)),
        ))
        .expect("long error");
    evidence
        .create_error(&error("err-2", "Curto.", None))
        .expect("short error");

    let listed = evidence.list_errors("exec-1", None).expect("list");

    assert_eq!(listed.len(), 2);
    assert_eq!(utf16_len(&listed[0].message), 500_000);
    assert_eq!(listed[0].path.as_deref().map(utf16_len), Some(4_096));
    assert_eq!(listed[0].stage, ExecutionErrorStage::Validation);
    assert_eq!(listed[1].path, None);
}

#[test]
fn a_validation_link_is_recorded_once_per_validation_change_and_phase() {
    let db = open("evidence-links");
    let evidence = ExecutionEvidence::new(db.connection());
    let proposed = link(Some("chg-1"), ExecutionValidationPhase::Proposed);
    evidence.link_validation(&proposed).expect("first link");
    evidence
        .link_validation(&proposed)
        .expect("same link again");
    evidence
        .link_validation(&link(None, ExecutionValidationPhase::Before))
        .expect("link without a change");
    evidence
        .link_validation(&link(None, ExecutionValidationPhase::Before))
        .expect("same link without a change");
    evidence
        .link_validation(&link(
            Some("chg-1"),
            ExecutionValidationPhase::AfterDecision,
        ))
        .expect("another phase");

    let listed = evidence
        .list_validation_links("exec-1")
        .expect("validation links");

    assert_eq!(listed.len(), 3, "{listed:?}");
    assert!(listed.contains(&proposed));
    assert!(listed.contains(&link(None, ExecutionValidationPhase::Before)));
    assert!(listed.contains(&link(
        Some("chg-1"),
        ExecutionValidationPhase::AfterDecision
    )));
}

#[test]
fn an_error_is_listed_with_its_stage_and_path() {
    let db = open("evidence-error-stage");
    let evidence = ExecutionEvidence::new(db.connection());
    evidence
        .create_error(&error("err-1", "Falhou.", Some("src/a.ts")))
        .expect("error");

    let listed = evidence.list_errors("exec-1", None).expect("list");

    assert_eq!(listed, vec![error("err-1", "Falhou.", Some("src/a.ts"))]);
}
