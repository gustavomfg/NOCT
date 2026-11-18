//! The worker as the client runs it: a child process, one command per line on stdin, one answer per line on stdout. These
//! tests cover the framing, the revocation after a refusal, and the end of the session, which the scenario tests do not
//! reach because they call the operation directly.

#![cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Runs the worker with `lines` on stdin, closes it, and returns the answers and the exit status.
fn run_worker(lines: &[&str]) -> (Vec<String>, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rollback-boundary"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the worker starts");
    {
        let mut stdin = child.stdin.take().expect("the worker has stdin");
        for line in lines {
            writeln!(stdin, "{line}").expect("a command is written");
        }
    }
    let stdout = child.stdout.take().expect("the worker has stdout");
    let answers: Vec<String> = BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
        .collect();
    let status = child.wait().expect("the worker exits");
    (answers, status.success())
}

/// hex("Operation no longer active"), the refusal of a command before `INIT`.
const NOT_ACTIVE: &str = "4f7065726174696f6e206e6f206c6f6e67657220616374697665";

#[test]
fn a_command_before_init_is_refused_with_revoked_and_the_session_continues() {
    let (answers, clean) = run_worker(&["OPEN\t00", "REVOKE", "CLOSE"]);
    assert!(clean, "CLOSE ends the worker with success");
    assert_eq!(
        answers,
        vec![format!("ERR\tREVOKED\t{NOT_ACTIVE}"), "OK".to_string(),]
    );
}

#[test]
fn an_unknown_command_after_a_refusal_is_answered_on_the_next_line() {
    // The refusal revokes the operation; the next line is still read and answered.
    let (answers, clean) = run_worker(&["BOGUS", "NEXT", "CLOSE"]);
    assert!(clean);
    assert_eq!(answers.len(), 2, "one answer per command: {answers:?}");
    assert!(answers[0].starts_with("ERR\tREVOKED\t"), "{answers:?}");
    assert!(answers[1].starts_with("ERR\tREVOKED\t"), "{answers:?}");
}

#[test]
fn a_frame_beyond_the_budget_is_refused_before_it_is_parsed() {
    let frame = "A".repeat(32 * 1024 * 1024 * 2 + 16 * 1024 + 1);
    let (answers, clean) = run_worker(&[frame.as_str(), "CLOSE"]);
    assert!(clean);
    assert_eq!(answers.len(), 1, "{answers:?}");
    assert!(answers[0].starts_with("ERR\tCONFLICT\t"), "{answers:?}");
}

#[test]
fn an_empty_session_exits_cleanly_without_answers() {
    let (answers, clean) = run_worker(&[]);
    assert!(clean);
    assert!(answers.is_empty(), "{answers:?}");
}
