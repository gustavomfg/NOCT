//! The rollback protocol through the operation API, on real files in a scratch directory. Each test is one scenario
//! of the protocol: the publication of a new file, the replacement of an observed one, and the refusals that revoke an
//! operation. The scenarios are the same on every system with a backend; the expectations that differ are gated.

#![cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use noct_native::Operation;
use noct_native::protocol::{Failure, hex};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A scratch directory, removed when the scenario ends.
struct Scratch {
    base: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "noct-native-{name}-{}-{serial}",
            std::process::id()
        ));
        fs::create_dir_all(&base).expect("the scratch directory is created");
        let base = fs::canonicalize(&base).expect("the scratch directory has a canonical path");
        Self { base }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

/// The identity the worker reports for a path: device and inode on Unix.
#[cfg(unix)]
fn identity_of(path: &Path) -> String {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::metadata(path).expect("the path has metadata");
    format!("{}:{}", metadata.dev(), metadata.ino())
}

/// A wire field: the hex of the text, as the Electron client sends paths and contents.
fn field(text: &str) -> Vec<u8> {
    hex(text.as_bytes()).into_bytes()
}

/// The response fields of an answer, tab-separated.
fn fields(response: &str) -> Vec<&str> {
    response.split('\t').collect()
}

/// A workspace with `parent/target.txt` holding `AFTER`, and a recovery directory, bound by `INIT`.
struct Fixture {
    _scratch: Scratch,
    workspace: PathBuf,
    recovery: PathBuf,
    operation: Operation,
}

impl Fixture {
    fn initialized(name: &str) -> Self {
        let scratch = Scratch::new(name);
        let workspace = scratch.base.join("workspace");
        let recovery = scratch.base.join("recovery");
        fs::create_dir_all(workspace.join("parent")).expect("the parent is created");
        fs::create_dir_all(&recovery).expect("the recovery directory is created");
        fs::write(workspace.join("parent").join("target.txt"), b"AFTER")
            .expect("the target is written");
        let mut fixture = Self {
            _scratch: scratch,
            workspace,
            recovery,
            operation: Operation::default(),
        };
        let init = [
            field(&fixture.workspace.to_string_lossy()),
            field(&identity_of(&fixture.workspace)),
            field(&fixture.recovery.to_string_lossy()),
            field(&identity_of(&fixture.recovery)),
        ];
        let answer = fixture.operation.execute(&[
            b"INIT".as_slice(),
            init[0].as_slice(),
            init[1].as_slice(),
            init[2].as_slice(),
            init[3].as_slice(),
        ]);
        assert_eq!(answer.expect("INIT binds the roots"), "OK");
        fixture
    }

    fn target(&self) -> PathBuf {
        self.workspace.join("parent").join("target.txt")
    }

    fn command(&mut self, parts: &[&[u8]]) -> Result<String, Failure> {
        self.operation.execute(parts)
    }

    fn open(&mut self, relative: &str) -> Result<String, Failure> {
        let relative = field(relative);
        self.command(&[b"OPEN".as_slice(), relative.as_slice()])
    }

    fn stage(&mut self, content: &str, mode: &str) -> Result<String, Failure> {
        let content = field(content);
        self.command(&[b"STAGE".as_slice(), content.as_slice(), mode.as_bytes()])
    }

    fn displace(&mut self, retention: &str) -> Result<String, Failure> {
        let retention = field(retention);
        self.command(&[b"DISPLACE".as_slice(), retention.as_slice()])
    }
}

fn code(answer: Result<String, Failure>) -> &'static str {
    match answer {
        Ok(text) => panic!("expected a refusal, got {text}"),
        Err(failure) => failure.code,
    }
}

#[test]
fn a_missing_destination_is_published_with_the_staged_bytes_and_mode() {
    let mut fixture = Fixture::initialized("missing");

    let observed = fixture
        .open("parent/new.txt")
        .expect("OPEN reads an absent leaf");
    assert!(
        observed.starts_with("DATA\t0\t0\t\t\t"),
        "absence is reported: {observed}"
    );

    let staged = fixture
        .stage("HELLO", "420")
        .expect("STAGE writes the bytes");
    let parts = fields(&staged);
    assert_eq!(parts[0], "STAGED");
    assert_eq!(parts.len(), 3, "the staged name field is present");

    assert_eq!(
        fixture.command(&[b"PUBLISH".as_slice()]),
        Ok("OK".to_string())
    );
    assert_eq!(fixture.command(&[b"NEXT".as_slice()]), Ok("OK".to_string()));

    let published = fixture.workspace.join("parent").join("new.txt");
    assert_eq!(
        fs::read(&published).expect("the file is published"),
        b"HELLO"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&published)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644, "the requested mode is kept");
    }
}

#[test]
fn an_observed_file_is_displaced_and_the_staged_file_takes_its_place() {
    let mut fixture = Fixture::initialized("displace");

    let observed = fixture
        .open("parent/target.txt")
        .expect("OPEN reads the target");
    let parts = fields(&observed);
    assert_eq!(parts[0], "DATA");
    assert_eq!(parts[1], "1", "the target exists");
    assert_eq!(parts[4], hex(b"AFTER"), "the observed bytes are returned");

    fixture
        .stage("AUTHORIZED", "420")
        .expect("STAGE writes the bytes");
    let displaced = fixture
        .displace(".nocturne-rollback-test.after")
        .expect("DISPLACE moves the observed file aside");
    assert_eq!(fields(&displaced)[0], "DISPLACED");
    assert_eq!(
        fixture.command(&[b"PUBLISH".as_slice()]),
        Ok("OK".to_string())
    );
    assert_eq!(fixture.command(&[b"NEXT".as_slice()]), Ok("OK".to_string()));

    assert_eq!(
        fs::read(fixture.target()).expect("the target is replaced"),
        b"AUTHORIZED"
    );
    let retained = fixture
        .workspace
        .join("parent")
        .join(".nocturne-rollback-test.after");
    assert_eq!(
        fs::read(retained).expect("the observed file is kept"),
        b"AFTER"
    );
}

#[test]
fn a_destination_that_changes_after_it_was_observed_is_refused() {
    let mut fixture = Fixture::initialized("changed");
    fixture
        .open("parent/target.txt")
        .expect("OPEN reads the target");
    fs::write(fixture.target(), b"CHANGED").expect("a peer changes the target");

    fixture
        .stage("AUTHORIZED", "420")
        .expect("STAGE writes the bytes");
    assert_eq!(
        code(fixture.displace(".nocturne-rollback-test.after")),
        "CONFLICT"
    );
    assert_eq!(
        fs::read(fixture.target()).expect("the peer's file stays"),
        b"CHANGED"
    );
}

#[test]
fn a_destination_that_appears_after_an_absent_observation_is_refused() {
    let mut fixture = Fixture::initialized("appeared");
    fixture
        .open("parent/new.txt")
        .expect("OPEN reads an absent leaf");
    fs::write(fixture.workspace.join("parent").join("new.txt"), b"PEER")
        .expect("a peer creates the file");

    assert_eq!(code(fixture.command(&[b"PUBLISH".as_slice()])), "CONFLICT");
}

#[test]
fn names_outside_the_workspace_and_protected_names_are_refused() {
    let mut fixture = Fixture::initialized("names");
    for relative in [
        "../outside.txt",
        "/absolute.txt",
        ".git/config",
        ".nocturne/state",
        "parent/.nocturne-rollback-x",
        "parent/./x",
        "parent/",
        "parent//x",
        "parent\\x",
        "",
    ] {
        assert_eq!(
            code(fixture.open(relative)),
            "CONFLICT",
            "{relative:?} is refused"
        );
    }
}

#[test]
fn special_permission_bits_are_refused_before_anything_is_staged() {
    let mut fixture = Fixture::initialized("special");
    fixture
        .open("parent/new.txt")
        .expect("OPEN reads an absent leaf");

    // 0o4755 is 2541 in decimal, the setuid form of the requested mode.
    assert_eq!(code(fixture.stage("HELLO", "2541")), "CONFLICT");
}

#[test]
fn a_destination_recreated_after_a_deletion_is_refused() {
    let mut fixture = Fixture::initialized("recreated");
    fs::remove_file(fixture.target()).expect("the target is removed");
    fixture
        .open("parent/target.txt")
        .expect("OPEN reads an absent leaf");
    fs::write(fixture.target(), b"RECREATED").expect("a peer recreates the target");

    assert_eq!(code(fixture.command(&[b"PUBLISH".as_slice()])), "CONFLICT");
}

#[test]
fn a_root_that_is_replaced_revokes_the_operation() {
    let mut fixture = Fixture::initialized("moved");
    let moved = fixture.workspace.with_file_name("workspace-moved");
    fs::rename(&fixture.workspace, &moved).expect("the root is moved");
    fs::create_dir(&fixture.workspace).expect("a new root takes its place");

    assert_eq!(code(fixture.open("parent/target.txt")), "REVOKED");
    fixture.operation.revoke();
    assert_eq!(code(fixture.open("parent/target.txt")), "REVOKED");
}

#[test]
fn next_refuses_an_operation_whose_publication_was_not_verified() {
    let mut fixture = Fixture::initialized("next");
    fixture
        .open("parent/target.txt")
        .expect("OPEN reads the target");
    assert_eq!(
        code(fixture.command(&[b"NEXT".as_slice()])),
        "CONFLICT",
        "NEXT needs a verified publication"
    );
}

#[test]
fn init_binds_the_roots_once_and_refuses_a_different_identity() {
    let scratch = Scratch::new("init");
    let workspace = scratch.base.join("workspace");
    let recovery = scratch.base.join("recovery");
    fs::create_dir_all(&workspace).expect("the workspace is created");
    fs::create_dir_all(&recovery).expect("the recovery directory is created");

    let mut wrong = Operation::default();
    let refused = wrong.execute(&[
        b"INIT".as_slice(),
        field(&workspace.to_string_lossy()).as_slice(),
        field("0:0").as_slice(),
        field(&recovery.to_string_lossy()).as_slice(),
        field(&identity_of(&recovery)).as_slice(),
    ]);
    assert_eq!(
        refused.expect_err("a wrong identity is refused").code,
        "CONFLICT"
    );

    let mut operation = Operation::default();
    let init = [
        field(&workspace.to_string_lossy()),
        field(&identity_of(&workspace)),
        field(&recovery.to_string_lossy()),
        field(&identity_of(&recovery)),
    ];
    let bind = [
        b"INIT".as_slice(),
        init[0].as_slice(),
        init[1].as_slice(),
        init[2].as_slice(),
        init[3].as_slice(),
    ];
    assert_eq!(operation.execute(&bind), Ok("OK".to_string()));
    assert_eq!(
        operation
            .execute(&bind)
            .expect_err("a second INIT is refused")
            .code,
        "CONFLICT"
    );
}

#[test]
fn the_journal_keeps_each_entry_and_replaces_the_snapshot() {
    let mut fixture = Fixture::initialized("journal");
    let snapshot = field("{\"first\":true}");
    let entry = field("{\"step\":1}");
    assert_eq!(
        fixture.command(&[b"JOURNAL".as_slice(), snapshot.as_slice(), entry.as_slice()]),
        Ok("OK".to_string())
    );
    let snapshot = field("{\"second\":true}");
    let entry = field("{\"step\":2}");
    assert_eq!(
        fixture.command(&[b"JOURNAL".as_slice(), snapshot.as_slice(), entry.as_slice()]),
        Ok("OK".to_string())
    );

    let steps =
        fs::read_to_string(fixture.recovery.join("steps.jsonl")).expect("the journal is read");
    assert_eq!(steps, "{\"step\":1}\n{\"step\":2}\n");
    let state =
        fs::read_to_string(fixture.recovery.join("operation.json")).expect("the snapshot is read");
    assert_eq!(state, "{\"second\":true}");
}

#[test]
fn a_command_before_init_is_revoked_and_an_unknown_one_is_refused_after_it() {
    let mut operation = Operation::default();
    assert_eq!(
        operation
            .execute(&[b"OPEN".as_slice(), b"00".as_slice()])
            .expect_err("nothing runs before INIT")
            .code,
        "REVOKED"
    );

    let mut fixture = Fixture::initialized("unknown");
    assert_eq!(code(fixture.command(&[b"BOGUS".as_slice()])), "CONFLICT");
}
