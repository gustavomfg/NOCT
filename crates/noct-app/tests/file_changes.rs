//! Proposed changes to project files, against the real disk: nothing is written before approval, the path stays inside the
//! project, no link is followed, a file changed by someone else is a conflict, undo restores the text, and the journal
//! survives a restart without leaving the data folder.

mod common;

use std::path::{Path, PathBuf};

use common::Scratch;
use noct_app::bootstrap::{self, Persistence};
use noct_app::error::CommandError;
use noct_app::file_changes::{self, JOURNAL_FILE};
use noct_core::file_change::{FileChange, FileChangeProposal, FileChangeStatus};

fn fixture(name: &str) -> (Scratch, Persistence) {
    let scratch = Scratch::new(name);
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    (scratch, persistence)
}

/// A real folder in the scratch directory, saved and authorized as a project.
fn project(scratch: &Scratch, persistence: &Persistence, name: &str) -> String {
    let path = unsaved(scratch, name);
    persistence
        .store
        .lock()
        .expect("the store is not poisoned")
        .workspaces()
        .touch(&path)
        .expect("the project is saved");
    path
}

/// A real folder in the scratch directory, not saved as a project.
fn unsaved(scratch: &Scratch, name: &str) -> String {
    let folder = scratch.path().join(name);
    std::fs::create_dir_all(&folder).expect("the folder is created");
    folder.to_string_lossy().into_owned()
}

fn proposal(workspace: &str, path: &str, content: &str) -> FileChangeProposal {
    FileChangeProposal {
        workspace: workspace.to_string(),
        path: path.to_string(),
        content: content.to_string(),
    }
}

fn data_directory(scratch: &Scratch) -> PathBuf {
    scratch.path().join("data")
}

fn text(path: &Path) -> String {
    std::fs::read_to_string(path).expect("the file is readable")
}

#[test]
fn a_proposal_writes_nothing_until_it_is_approved() {
    let (scratch, persistence) = fixture("file-change-approve");
    let root = project(&scratch, &persistence, "project");
    std::fs::create_dir_all(Path::new(&root).join("src")).unwrap();
    std::fs::write(Path::new(&root).join("src/lib.rs"), "old\n").unwrap();

    let change = file_changes::propose(&persistence, &[], proposal(&root, "src/lib.rs", "new\n"))
        .expect("the proposal is recorded");
    assert_eq!(change.status, FileChangeStatus::Pending);
    assert_eq!(change.base.as_deref(), Some("old\n"));
    assert_eq!(
        text(&Path::new(&root).join("src/lib.rs")),
        "old\n",
        "a proposal writes nothing"
    );

    let applied =
        file_changes::approve(&persistence, &[], &change.id).expect("the change is approved");
    assert_eq!(applied.status, FileChangeStatus::Applied);
    assert_eq!(text(&Path::new(&root).join("src/lib.rs")), "new\n");
}

#[test]
fn undo_restores_the_text_the_file_had() {
    let (scratch, persistence) = fixture("file-change-undo");
    let root = project(&scratch, &persistence, "project");
    std::fs::write(Path::new(&root).join("notes.txt"), "before\n").unwrap();

    let change =
        file_changes::propose(&persistence, &[], proposal(&root, "notes.txt", "after\n")).unwrap();
    file_changes::approve(&persistence, &[], &change.id).unwrap();
    let undone = file_changes::undo(&persistence, &[], &change.id).expect("the change is undone");

    assert_eq!(undone.status, FileChangeStatus::Undone);
    assert_eq!(text(&Path::new(&root).join("notes.txt")), "before\n");
}

#[test]
fn undo_of_a_change_that_created_its_file_removes_the_file() {
    let (scratch, persistence) = fixture("file-change-create");
    let root = project(&scratch, &persistence, "project");

    let change =
        file_changes::propose(&persistence, &[], proposal(&root, "created.txt", "hello\n"))
            .unwrap();
    assert_eq!(change.base, None, "a file that does not exist has no base");
    file_changes::approve(&persistence, &[], &change.id).unwrap();
    assert!(Path::new(&root).join("created.txt").exists());

    file_changes::undo(&persistence, &[], &change.id).unwrap();
    assert!(!Path::new(&root).join("created.txt").exists());
}

#[test]
fn a_path_that_leaves_the_project_is_refused_and_nothing_is_written() {
    let (scratch, persistence) = fixture("file-change-traversal");
    let root = project(&scratch, &persistence, "project");

    for path in [
        "../escape.txt",
        "src/../../escape.txt",
        "/tmp/absolute.txt",
        "a\\..\\b.txt",
    ] {
        assert_eq!(
            file_changes::propose(&persistence, &[], proposal(&root, path, "x")),
            Err(CommandError::PathNotAllowed),
            "{path} must be refused"
        );
    }
    assert!(!scratch.path().join("escape.txt").exists());
}

#[test]
fn a_sensitive_name_is_refused_and_nothing_is_written() {
    let (scratch, persistence) = fixture("file-change-sensitive");
    let root = project(&scratch, &persistence, "project");

    for path in [
        ".env",
        "keys/server.pem",
        ".git/config",
        "config/database-credentials.json",
    ] {
        assert_eq!(
            file_changes::propose(&persistence, &[], proposal(&root, path, "x")),
            Err(CommandError::PathNotAllowed),
            "{path} must be refused"
        );
    }
    assert!(!Path::new(&root).join(".env").exists());
    assert!(!Path::new(&root).join("keys").exists());
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_is_never_followed_to_write_outside_the_project() {
    let (scratch, persistence) = fixture("file-change-symlink");
    let root = project(&scratch, &persistence, "project");
    let outside = scratch.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("victim.txt"), "keep\n").unwrap();
    let project_root = Path::new(&root);
    std::os::unix::fs::symlink(&outside, project_root.join("link")).unwrap();
    std::os::unix::fs::symlink(outside.join("victim.txt"), project_root.join("leak.txt")).unwrap();

    assert_eq!(
        file_changes::propose(
            &persistence,
            &[],
            proposal(&root, "link/victim.txt", "pwned\n")
        ),
        Err(CommandError::PathNotAllowed),
        "a folder that is a link must not be written through"
    );
    assert_eq!(
        file_changes::propose(&persistence, &[], proposal(&root, "leak.txt", "pwned\n")),
        Err(CommandError::PathNotAllowed),
        "a file that is a link must not be written through"
    );
    assert_eq!(text(&outside.join("victim.txt")), "keep\n");
}

#[cfg(unix)]
#[test]
fn a_file_replaced_by_a_link_after_the_proposal_is_not_written_through() {
    let (scratch, persistence) = fixture("file-change-swap");
    let root = project(&scratch, &persistence, "project");
    let outside = scratch.path().join("outside.txt");
    std::fs::write(&outside, "keep\n").unwrap();
    let project_root = Path::new(&root);
    std::fs::write(project_root.join("swap.txt"), "v1\n").unwrap();

    let change =
        file_changes::propose(&persistence, &[], proposal(&root, "swap.txt", "v2\n")).unwrap();
    std::fs::remove_file(project_root.join("swap.txt")).unwrap();
    std::os::unix::fs::symlink(&outside, project_root.join("swap.txt")).unwrap();

    assert_eq!(
        file_changes::approve(&persistence, &[], &change.id),
        Err(CommandError::PathNotAllowed)
    );
    assert_eq!(text(&outside), "keep\n");
}

#[test]
fn a_file_that_changed_after_the_proposal_is_a_conflict_and_the_change_stays_pending() {
    let (scratch, persistence) = fixture("file-change-conflict");
    let root = project(&scratch, &persistence, "project");
    let file = Path::new(&root).join("a.txt");
    std::fs::write(&file, "v1\n").unwrap();

    let change =
        file_changes::propose(&persistence, &[], proposal(&root, "a.txt", "mine\n")).unwrap();
    std::fs::write(&file, "edited by hand\n").unwrap();

    assert_eq!(
        file_changes::approve(&persistence, &[], &change.id),
        Err(CommandError::ChangeConflict)
    );
    assert_eq!(text(&file), "edited by hand\n", "the other edit is kept");
    let listed = file_changes::list(&persistence, &[], &root).unwrap();
    assert_eq!(listed[0].status, FileChangeStatus::Pending);
}

#[test]
fn undo_refuses_when_the_file_changed_after_the_approval() {
    let (scratch, persistence) = fixture("file-change-undo-conflict");
    let root = project(&scratch, &persistence, "project");
    let file = Path::new(&root).join("a.txt");
    std::fs::write(&file, "v1\n").unwrap();
    let change =
        file_changes::propose(&persistence, &[], proposal(&root, "a.txt", "v2\n")).unwrap();
    file_changes::approve(&persistence, &[], &change.id).unwrap();
    std::fs::write(&file, "changed later\n").unwrap();

    assert_eq!(
        file_changes::undo(&persistence, &[], &change.id),
        Err(CommandError::ChangeConflict)
    );
    assert_eq!(text(&file), "changed later\n");
    assert_eq!(
        file_changes::list(&persistence, &[], &root).unwrap()[0].status,
        FileChangeStatus::Applied
    );
}

#[test]
fn a_rejected_change_writes_nothing_and_cannot_be_approved_or_rejected_again() {
    let (scratch, persistence) = fixture("file-change-reject");
    let root = project(&scratch, &persistence, "project");
    let file = Path::new(&root).join("a.txt");
    std::fs::write(&file, "v1\n").unwrap();
    let change =
        file_changes::propose(&persistence, &[], proposal(&root, "a.txt", "v2\n")).unwrap();

    let rejected = file_changes::reject(&persistence, &change.id).unwrap();
    assert_eq!(rejected.status, FileChangeStatus::Rejected);
    assert_eq!(text(&file), "v1\n");
    assert_eq!(
        file_changes::approve(&persistence, &[], &change.id),
        Err(CommandError::InvalidChangeTransition)
    );
    assert_eq!(
        file_changes::reject(&persistence, &change.id),
        Err(CommandError::InvalidChangeTransition)
    );
}

#[test]
fn an_unknown_change_is_reported_as_unknown() {
    let (_scratch, persistence) = fixture("file-change-unknown");
    assert_eq!(
        file_changes::approve(&persistence, &[], "no-such-change"),
        Err(CommandError::UnknownChange)
    );
    assert_eq!(
        file_changes::undo(&persistence, &[], "no-such-change"),
        Err(CommandError::UnknownChange)
    );
}

#[test]
fn a_project_that_is_not_saved_or_is_blocked_is_refused_for_every_operation() {
    let (scratch, persistence) = fixture("file-change-scope");
    let root = project(&scratch, &persistence, "saved");
    let other = unsaved(&scratch, "not-saved");

    assert_eq!(
        file_changes::propose(&persistence, &[], proposal(&other, "a.txt", "x")),
        Err(CommandError::UnknownWorkspace)
    );
    assert_eq!(
        file_changes::list(&persistence, &[], &other),
        Err(CommandError::UnknownWorkspace)
    );
    assert_eq!(
        file_changes::propose(
            &persistence,
            &[PathBuf::from(&root)],
            proposal(&root, "a.txt", "x")
        ),
        Err(CommandError::UnknownWorkspace),
        "a blocked root is refused even when it is saved"
    );
}

#[test]
fn a_folder_in_place_of_a_file_and_a_missing_folder_are_refused() {
    let (scratch, persistence) = fixture("file-change-shape");
    let root = project(&scratch, &persistence, "project");
    std::fs::create_dir_all(Path::new(&root).join("dir.txt")).unwrap();

    assert_eq!(
        file_changes::propose(&persistence, &[], proposal(&root, "dir.txt", "x")),
        Err(CommandError::PathNotAllowed)
    );
    assert_eq!(
        file_changes::propose(&persistence, &[], proposal(&root, "missing/new.txt", "x")),
        Err(CommandError::PathNotAllowed),
        "a new file needs an existing folder"
    );
}

#[test]
fn a_file_that_is_too_large_is_not_text_or_would_be_too_large_is_refused() {
    let (scratch, persistence) = fixture("file-change-limits");
    let root = project(&scratch, &persistence, "project");
    let project_root = Path::new(&root);
    std::fs::write(project_root.join("big.txt"), "a".repeat(40 * 1024)).unwrap();
    std::fs::write(project_root.join("binary.bin"), [0xff_u8, 0xfe, 0x00]).unwrap();

    assert_eq!(
        file_changes::propose(&persistence, &[], proposal(&root, "big.txt", "small")),
        Err(CommandError::FileNotEditable)
    );
    assert_eq!(
        file_changes::propose(&persistence, &[], proposal(&root, "binary.bin", "text")),
        Err(CommandError::FileNotEditable)
    );
    let too_much = "b".repeat(33 * 1024);
    assert_eq!(
        file_changes::propose(&persistence, &[], proposal(&root, "new.txt", &too_much)),
        Err(CommandError::FileNotEditable)
    );
}

#[test]
fn the_journal_survives_a_restart_and_undo_still_works_after_it() {
    let scratch = Scratch::new("file-change-restart");
    let data = data_directory(&scratch);
    let root;
    let change_id;
    {
        let persistence = bootstrap::open(&data).expect("the shell opens");
        root = project(&scratch, &persistence, "project");
        std::fs::write(Path::new(&root).join("a.txt"), "before\n").unwrap();
        let change =
            file_changes::propose(&persistence, &[], proposal(&root, "a.txt", "after\n")).unwrap();
        file_changes::approve(&persistence, &[], &change.id).unwrap();
        change_id = change.id;
    }
    let reopened = bootstrap::open(&data).expect("the shell opens again");
    let listed: Vec<FileChange> = file_changes::list(&reopened, &[], &root).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].status, FileChangeStatus::Applied);

    file_changes::undo(&reopened, &[], &change_id).expect("undo works after the restart");
    assert_eq!(text(&Path::new(&root).join("a.txt")), "before\n");
}

#[test]
fn a_journal_that_cannot_be_read_is_refused_and_is_not_overwritten() {
    let (scratch, persistence) = fixture("file-change-damaged");
    let root = project(&scratch, &persistence, "project");
    let journal = data_directory(&scratch).join(JOURNAL_FILE);
    std::fs::write(&journal, "{ not json").unwrap();

    assert_eq!(
        file_changes::propose(&persistence, &[], proposal(&root, "a.txt", "x")),
        Err(CommandError::Persistence)
    );
    assert_eq!(std::fs::read_to_string(&journal).unwrap(), "{ not json");
}

#[test]
fn the_open_changes_are_capped_so_the_journal_stays_bounded() {
    let (scratch, persistence) = fixture("file-change-cap");
    let root = project(&scratch, &persistence, "project");
    for index in 0..noct_core::limits::file_change::MAXIMUM_OPEN_CHANGES {
        file_changes::propose(
            &persistence,
            &[],
            proposal(&root, &format!("f{index}.txt"), "x"),
        )
        .unwrap();
    }
    assert_eq!(
        file_changes::propose(&persistence, &[], proposal(&root, "one-more.txt", "x")),
        Err(CommandError::TooManyOpenChanges)
    );
}

#[cfg(unix)]
#[test]
fn the_journal_is_private_and_an_approved_write_keeps_the_mode_of_the_file() {
    use std::os::unix::fs::PermissionsExt;

    let (scratch, persistence) = fixture("file-change-modes");
    let root = project(&scratch, &persistence, "project");
    let file = Path::new(&root).join("modes.txt");
    std::fs::write(&file, "v1\n").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();

    let change =
        file_changes::propose(&persistence, &[], proposal(&root, "modes.txt", "v2\n")).unwrap();
    file_changes::approve(&persistence, &[], &change.id).unwrap();

    let journal_mode = std::fs::metadata(data_directory(&scratch).join(JOURNAL_FILE))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        journal_mode, 0o600,
        "the journal holds project text and is private"
    );
    let file_mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(file_mode, 0o640, "the write keeps the mode the file had");
}
