//! The protocol state machine of the rollback worker, the same on every system. The system calls are in `platform`.
//! Port of the `Operation` class and of `main` in `native/rollback-boundary.cpp`.
//!
//! An operation is one capability: its root and recovery directories are bound at `INIT`, and every later command
//! first checks that the bindings still resolve to the same identities. A failed command revokes the operation for
//! the rest of its life, so a worker never continues from an uncertain state.

use std::io::{BufRead, Write};

use crate::platform::{self, Object};
use crate::protocol::{Failure, MAX_BYTES, Outcome, hex, require, split_frame, unhex};

/// The largest frame the worker reads: two hex digits per byte of a file, and room for the fields.
const FRAME_BUDGET: usize = MAX_BYTES * 2 + 16 * 1024;

/// The object that a command needs and that the protocol has not yet acquired.
fn held(slot: &Option<Object>) -> Outcome<&Object> {
    slot.as_ref()
        .ok_or(Failure::new("CONFLICT", "Filesystem operation refused"))
}

fn parse_mode(field: &[u8]) -> Outcome<u32> {
    std::str::from_utf8(field)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(Failure::new("UNKNOWN", "Native protocol interrupted"))
}

/// An identity from the wire. Identities are ASCII digits and separators, so lossy conversion never makes a false match.
fn text(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

#[derive(Default)]
pub struct Operation {
    root: Option<Object>,
    recovery: Option<Object>,
    parent: Option<Object>,
    observed: Option<Object>,
    staged: Option<Object>,
    displaced: Option<Object>,
    log: Option<Object>,
    snapshot: Option<Object>,
    root_path: Vec<u8>,
    root_id: String,
    recovery_path: Vec<u8>,
    recovery_id: String,
    parent_path: Vec<u8>,
    parent_id: String,
    leaf: Vec<u8>,
    observed_id: String,
    observed_mode: u32,
    observed_bytes: Vec<u8>,
    staged_bytes: Vec<u8>,
    staged_mode: u32,
    displaced_name: Vec<u8>,
    active: bool,
    opened: bool,
    prepared: bool,
    moved: bool,
    published: bool,
}

impl Operation {
    pub fn revoke(&mut self) {
        self.active = false;
    }

    /// Runs one command. The answer is the response fields, tab-separated, or the failure that revokes the operation.
    pub fn execute(&mut self, args: &[&[u8]]) -> Outcome<String> {
        let command = *args
            .first()
            .ok_or(Failure::new("UNKNOWN", "Native protocol interrupted"))?;
        match command {
            b"INIT" => self.init(args),
            b"JOURNAL" => self.journal(args),
            b"REVOKE" => {
                self.revoke();
                Ok("OK".into())
            }
            _ => {
                self.admit()?;
                match command {
                    b"NEXT" => self.next(),
                    b"OPEN" => self.open(args),
                    b"STAGE" => self.stage(args),
                    b"DISPLACE" => self.displace(args),
                    b"PUBLISH" => self.publish(args),
                    _ => Err(Failure::new("CONFLICT", "Unknown operation")),
                }
            }
        }
    }

    /// Revalidates the retained bindings against fresh pathname resolution. Any mismatch revokes the operation for the
    /// rest of its lifetime.
    fn admit(&mut self) -> Outcome<()> {
        if !self.active {
            return Err(Failure::new("REVOKED", "Operation no longer active"));
        }
        if self.check_bindings().is_err() {
            self.active = false;
            return Err(Failure::new("REVOKED", "Required structural binding lost"));
        }
        Ok(())
    }

    fn check_bindings(&self) -> Outcome<()> {
        let fresh_root = platform::acquire_root(&self.root_path)?;
        let fresh_recovery = platform::acquire_root(&self.recovery_path)?;
        require(
            platform::identity(&fresh_root)? == self.root_id
                && platform::identity(&fresh_recovery)? == self.recovery_id,
            "Root binding lost",
        )?;
        if self.opened {
            let parent = platform::walk(held(&self.root)?, &self.parent_path)?;
            require(
                platform::identity(&parent)? == self.parent_id,
                "Parent binding lost",
            )?;
        }
        Ok(())
    }

    /// The observed entry must still be the one that `OPEN` read, or absent when `OPEN` found nothing.
    fn verify_observed(&self) -> Outcome<()> {
        let parent = held(&self.parent)?;
        if self.observed.is_none() {
            // Anything but a clean absence (an entry, or a lookup that failed) means the destination appeared.
            return match platform::child(parent, &self.leaf, false) {
                Ok(None) => Ok(()),
                _ => Err(Failure::new("CONFLICT", "Destination appeared")),
            };
        }
        match platform::child(parent, &self.leaf, false)? {
            // The C++ worker reports a vanished observed entry as the failed reopen that it is.
            None => Err(Failure::new("CONFLICT", "Filesystem operation refused")),
            Some(current) => require(
                platform::identity(&current)? == self.observed_id
                    && platform::mode_of(&current)? == self.observed_mode
                    && platform::read_all(&current)? == self.observed_bytes,
                "Observed AFTER changed",
            ),
        }
    }

    fn init(&mut self, args: &[&[u8]]) -> Outcome<String> {
        require(
            args.len() == 5 && self.root.is_none(),
            "Invalid initialization",
        )?;
        self.root_path = unhex(args[1])?;
        self.root_id = text(unhex(args[2])?);
        self.recovery_path = unhex(args[3])?;
        self.recovery_id = text(unhex(args[4])?);
        // The roots are kept before any check, so a failed INIT leaves no second INIT possible.
        self.root = Some(platform::acquire_root(&self.root_path)?);
        self.recovery = Some(platform::acquire_root(&self.recovery_path)?);
        let root = held(&self.root)?;
        let recovery = held(&self.recovery)?;
        require(
            platform::identity(root)? == self.root_id
                && platform::identity(recovery)? == self.recovery_id,
            "Root identity changed",
        )?;
        platform::check_filesystem(root)?;

        self.log = Some(platform::create_new(recovery, b"steps.jsonl")?);
        self.snapshot = Some(platform::create_new(recovery, b"operation.json")?);
        platform::flush_directory(recovery)?;
        self.active = true;
        Ok("OK".into())
    }

    fn journal(&mut self, args: &[&[u8]]) -> Outcome<String> {
        require(args.len() == 3 && self.log.is_some(), "Journal unavailable")?;
        let data = unhex(args[1])?;
        let mut entry = unhex(args[2])?;
        require(
            data.len() <= 1024 * 1024 && entry.len() <= 65536,
            "Journal budget exceeded",
        )?;
        entry.push(b'\n');
        let log = held(&self.log)?;
        platform::write_all(log, &entry)?;
        platform::flush(log)?;
        let snapshot = held(&self.snapshot)?;
        platform::rewind_truncate(snapshot)?;
        platform::write_all(snapshot, &data)?;
        platform::flush(snapshot)?;
        Ok("OK".into())
    }

    fn next(&mut self) -> Outcome<String> {
        require(self.published, "Previous file not verified")?;
        self.observed = None;
        self.staged = None;
        self.displaced = None;
        self.parent = None;
        self.opened = false;
        self.prepared = false;
        self.moved = false;
        self.published = false;
        self.observed_id.clear();
        self.observed_bytes.clear();
        self.staged_bytes.clear();
        Ok("OK".into())
    }

    fn open(&mut self, args: &[&[u8]]) -> Outcome<String> {
        require(args.len() == 2 && !self.opened, "File already acquired")?;
        let (parent_path, leaf) = platform::split_relative(&unhex(args[1])?)?;
        self.parent_path = parent_path;
        self.leaf = leaf;

        let parent = platform::walk(held(&self.root)?, &self.parent_path)?;
        self.parent_id = platform::identity(&parent)?;
        self.parent = Some(parent);
        self.opened = true;

        let parent_id = hex(self.parent_id.as_bytes());
        match platform::child(held(&self.parent)?, &self.leaf, true)? {
            None => Ok(format!("DATA\t0\t0\t\t\t{parent_id}")),
            Some(file) => {
                let observed_id = platform::identity(&file)?;
                let mode = platform::mode_of(&file)?;
                let bytes = platform::read_all(&file)?;
                let response = format!(
                    "DATA\t1\t{mode}\t{}\t{}\t{parent_id}",
                    hex(observed_id.as_bytes()),
                    hex(&bytes),
                );
                self.observed_id = observed_id;
                self.observed_mode = mode;
                self.observed_bytes = bytes;
                self.observed = Some(file);
                Ok(response)
            }
        }
    }

    fn stage(&mut self, args: &[&[u8]]) -> Outcome<String> {
        require(
            args.len() == 3 && self.opened && !self.prepared,
            "Invalid staging state",
        )?;
        self.staged_bytes = unhex(args[1])?;
        let mode = parse_mode(args[2])?;
        require((mode & 0o7000) == 0, "Special permission bits unsupported")?;
        self.staged_mode = mode;

        let (staged, name) = platform::stage(held(&self.parent)?, &self.staged_bytes, mode)?;
        let staged_id = platform::identity(&staged)?;
        self.staged = Some(staged);
        self.prepared = true;
        Ok(format!(
            "STAGED\t{}\t{}",
            hex(staged_id.as_bytes()),
            hex(&name)
        ))
    }

    fn displace(&mut self, args: &[&[u8]]) -> Outcome<String> {
        require(
            args.len() == 2 && self.opened && !self.moved && !self.published,
            "Invalid displacement state",
        )?;
        self.verify_observed()?;
        let retention = unhex(args[1])?;
        platform::validate_retention(&retention)?;
        self.displaced_name = retention;
        if self.observed.is_none() {
            return Ok("OK".into());
        }

        let parent = held(&self.parent)?;
        platform::displace(
            parent,
            &self.leaf,
            self.observed.as_ref(),
            &self.displaced_name,
        )?;
        self.moved = true;
        platform::flush_directory(parent)?;

        // A missing retention entry is a failed reopen, as in the C++ worker; the message is the same.
        let displaced = platform::child(parent, &self.displaced_name, true)?
            .ok_or(Failure::new("CONFLICT", "Filesystem operation refused"))?;
        let displaced_id = platform::identity(&displaced)?;
        require(
            displaced_id == self.observed_id
                && platform::mode_of(&displaced)? == self.observed_mode
                && platform::read_all(&displaced)? == self.observed_bytes,
            "Competing displaced object preserved; conflict",
        )?;
        let response = format!("DISPLACED\t{}", hex(displaced_id.as_bytes()));
        self.displaced = Some(displaced);
        Ok(response)
    }

    fn publish(&mut self, args: &[&[u8]]) -> Outcome<String> {
        require(
            args.len() == 1
                && self.opened
                && !self.published
                && (self.observed.is_none() || self.moved),
            "Invalid publication state",
        )?;
        let parent = held(&self.parent)?;
        if self.prepared {
            let staged = held(&self.staged)?;
            require(
                platform::read_all(staged)? == self.staged_bytes,
                "Staged bytes changed",
            )?;
            platform::publish(staged, parent, &self.leaf, self.staged_mode)?;
            self.published = true;
            platform::flush_directory(parent)?;

            let produced = platform::child(parent, &self.leaf, false)?
                .ok_or(Failure::new("CONFLICT", "Filesystem operation refused"))?;
            require(
                platform::read_all(&produced)? == self.staged_bytes
                    && (platform::mode_of(&produced)? & 0o777)
                        == platform::produced_mode(self.staged_mode),
                "Produced state diverged",
            )?;
        } else {
            // The C++ worker refuses a recreated entry, and any other failed lookup as an unavailable absence.
            match platform::child(parent, &self.leaf, false) {
                Ok(None) => {}
                Ok(Some(_)) => return Err(Failure::new("CONFLICT", "Deleted entry recreated")),
                Err(_) => return Err(Failure::new("CONFLICT", "Absence unavailable")),
            }
            self.published = true;
        }
        if self.moved {
            let displaced = held(&self.displaced)?;
            require(
                platform::identity(displaced)? == self.observed_id
                    && platform::read_all(displaced)? == self.observed_bytes,
                "Displaced bytes changed",
            )?;
        }
        self.admit()?;
        Ok("OK".into())
    }
}

/// The worker loop: one command per line on stdin, one response per line on stdout. A failure is answered with its
/// code and the hex of its message, and it revokes the operation.
pub fn run() {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut output = std::io::stdout().lock();
    let mut operation = Operation::default();
    let mut line = Vec::new();

    loop {
        line.clear();
        match input.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }

        let response = if line.len() > FRAME_BUDGET {
            Err(Failure::new("CONFLICT", "Frame budget exceeded"))
        } else if line == b"CLOSE" {
            break;
        } else {
            operation.execute(&split_frame(&line))
        };

        let written = match response {
            Ok(text) => writeln!(output, "{text}"),
            Err(failure) => {
                operation.revoke();
                writeln!(
                    output,
                    "ERR\t{}\t{}",
                    failure.code,
                    hex(failure.message.as_bytes())
                )
            }
        };
        if written.and_then(|_| output.flush()).is_err() {
            break;
        }
    }
}
