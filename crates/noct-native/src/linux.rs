//! Linux backend: capability-based operations over retained directory descriptors.
//! Port of the `__linux__` branch of `native/rollback-boundary.cpp`.

use crate::protocol::{Failure, MAX_BYTES, Outcome, hex, require, split_frame, unhex};
use std::ffi::CString;
use std::io::{BufRead, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

// Not exported by the libc crate for every glibc target; values from <linux/fs.h>
// and <linux/openat2.h>.
const RENAME_NOREPLACE: libc::c_uint = 1;
const RESOLVE_NO_XDEV: u64 = 0x01;
const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
const RESOLVE_NO_SYMLINKS: u64 = 0x04;
const RESOLVE_BENEATH: u64 = 0x08;
const READ_CHUNK: usize = 16 * 1024;
const FRAME_BUDGET: usize = MAX_BYTES * 2 + 16 * 1024;

#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

fn errno() -> libc::c_int {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// Failure for the errno that the last syscall left behind.
fn classify_errno() -> Failure {
    match errno() {
        libc::ENOSYS | libc::EOPNOTSUPP | libc::EXDEV | libc::EINVAL => {
            Failure::new("UNSUPPORTED", "Required native primitive unavailable")
        }
        _ => Failure::new("CONFLICT", "Filesystem operation refused"),
    }
}

fn checked(result: libc::c_int) -> Outcome<libc::c_int> {
    if result >= 0 {
        Ok(result)
    } else {
        Err(classify_errno())
    }
}

/// Takes ownership of a descriptor returned by a syscall, or reports its failure.
fn own(result: libc::c_int) -> Outcome<OwnedFd> {
    let fd = checked(result)?;
    // SAFETY: `checked` accepted a non-negative descriptor freshly returned by the kernel,
    // and nothing else holds it.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn cstr(bytes: &[u8]) -> Outcome<CString> {
    CString::new(bytes).map_err(|_| Failure::new("CONFLICT", "Invalid path bytes"))
}

fn text(bytes: Vec<u8>) -> String {
    // Identities are ASCII digits and a colon, so lossy conversion never produces a false match.
    String::from_utf8_lossy(&bytes).into_owned()
}

fn stat_of(fd: RawFd) -> Outcome<libc::stat> {
    // SAFETY: an all-zero `stat` is a valid output buffer for fstat.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `fd` is a live descriptor held by the caller; `st` is a valid out-pointer.
    require(
        unsafe { libc::fstat(fd, &mut st) } == 0,
        "Identity unavailable",
    )?;
    Ok(st)
}

fn identity(fd: RawFd) -> Outcome<String> {
    let st = stat_of(fd)?;
    Ok(format!("{}:{}", st.st_dev as u64, st.st_ino as u64))
}

/// Resolves `name` beneath `parent` through openat2 constraints. Returns the raw result
/// so callers can inspect errno, matching the C++ helper.
fn constrained(parent: RawFd, name: &CString, directory: bool, crossing: bool) -> libc::c_int {
    let how = OpenHow {
        flags: (libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if directory {
                libc::O_DIRECTORY
            } else {
                libc::O_NONBLOCK
            }) as u64,
        mode: 0,
        resolve: RESOLVE_BENEATH
            | RESOLVE_NO_SYMLINKS
            | RESOLVE_NO_MAGICLINKS
            | if crossing { 0 } else { RESOLVE_NO_XDEV },
    };
    // SAFETY: `name` is NUL-terminated and `how` outlives the call; the kernel validates both.
    unsafe {
        libc::syscall(
            libc::SYS_openat2,
            parent,
            name.as_ptr(),
            &how as *const OpenHow,
            std::mem::size_of::<OpenHow>(),
        ) as libc::c_int
    }
}

fn acquire_root(path: &[u8]) -> Outcome<OwnedFd> {
    require(
        path.len() > 1 && path[0] == b'/' && !path.contains(&0),
        "Invalid root",
    )?;
    // SAFETY: literal NUL-terminated path.
    let filesystem_root = own(unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    })?;
    let name = cstr(&path[1..])?;
    own(constrained(filesystem_root.as_raw_fd(), &name, true, true))
}

fn read_bytes(fd: RawFd) -> Outcome<Vec<u8>> {
    require(
        (stat_of(fd)?.st_mode & libc::S_IFMT) == libc::S_IFREG,
        "Only regular files are supported",
    )?;
    let mut data = Vec::new();
    let mut buffer = vec![0u8; READ_CHUNK];
    let mut offset: libc::off_t = 0;
    loop {
        // SAFETY: `buffer` is valid for `buffer.len()` bytes.
        let read = unsafe { libc::pread(fd, buffer.as_mut_ptr().cast(), buffer.len(), offset) };
        if read < 0 && errno() == libc::EINTR {
            continue;
        }
        require(read >= 0, "Read failed")?;
        if read == 0 {
            break;
        }
        let read = read as usize;
        require(
            data.len() + read <= MAX_BYTES,
            "File exceeds rollback budget",
        )?;
        data.extend_from_slice(&buffer[..read]);
        offset += read as libc::off_t;
    }
    Ok(data)
}

fn write_all(fd: RawFd, bytes: &[u8]) -> Outcome<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        // SAFETY: the slice is valid for the length passed.
        let written =
            unsafe { libc::write(fd, bytes[offset..].as_ptr().cast(), bytes.len() - offset) };
        if written < 0 && errno() == libc::EINTR {
            continue;
        }
        require(written > 0, "Write failed")?;
        offset += written as usize;
    }
    Ok(())
}

fn flush(fd: RawFd) -> Outcome<()> {
    // SAFETY: fsync on a live descriptor.
    if unsafe { libc::fsync(fd) } != 0 {
        return Err(Failure::new(
            "UNKNOWN",
            "Durability acknowledgment unavailable",
        ));
    }
    Ok(())
}

fn validate_relative(value: &[u8]) -> Outcome<()> {
    require(
        !value.is_empty()
            && value.len() < 4096
            && value[0] != b'/'
            && !value.contains(&0)
            && !value.contains(&b'\\'),
        "Invalid relative path",
    )?;
    for part in value.split(|&byte| byte == b'/') {
        // The C++ worker silently accepted `parent/`, treating the empty leaf as absent.
        // The port rejects it up front.
        require(!part.is_empty(), "Invalid relative path")?;
        let protected = part == &b"."[..]
            || part == &b".."[..]
            || part == &b".git"[..]
            || part == &b".nocturne"[..]
            || part.starts_with(b".nocturne-rollback-");
        require(!protected, "Protected path")?;
    }
    Ok(())
}

/// Probe of a name under the retained parent. Absence is a value, not an error.
enum Entry {
    Present(OwnedFd),
    Absent,
}

#[derive(Default)]
pub struct Operation {
    root: Option<OwnedFd>,
    recovery: Option<OwnedFd>,
    parent: Option<OwnedFd>,
    observed: Option<OwnedFd>,
    staged: Option<OwnedFd>,
    displaced: Option<OwnedFd>,
    log: Option<OwnedFd>,
    snapshot: Option<OwnedFd>,
    root_path: Vec<u8>,
    root_id: String,
    recovery_path: Vec<u8>,
    recovery_id: String,
    parent_path: Vec<u8>,
    parent_id: String,
    leaf: Vec<u8>,
    observed_id: String,
    observed_mode: libc::mode_t,
    observed_bytes: Vec<u8>,
    staged_bytes: Vec<u8>,
    staged_mode: libc::mode_t,
    displaced_name: Vec<u8>,
    active: bool,
    opened: bool,
    prepared: bool,
    moved: bool,
    published: bool,
}

impl Operation {
    fn root_fd(&self) -> Outcome<RawFd> {
        fd_of(&self.root)
    }

    fn parent_fd(&self) -> Outcome<RawFd> {
        fd_of(&self.parent)
    }

    fn probe(&self, name: &[u8]) -> Outcome<Entry> {
        let parent = self.parent_fd()?;
        let name = cstr(name)?;
        let raw = constrained(parent, &name, false, false);
        if raw >= 0 {
            // SAFETY: non-negative descriptor freshly returned by openat2.
            return Ok(Entry::Present(unsafe { OwnedFd::from_raw_fd(raw) }));
        }
        if errno() == libc::ENOENT {
            return Ok(Entry::Absent);
        }
        Err(classify_errno())
    }

    /// Revalidates the retained bindings against fresh pathname resolution.
    /// Any mismatch revokes the operation for the rest of its lifetime.
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
        let fresh_root = acquire_root(&self.root_path)?;
        let fresh_recovery = acquire_root(&self.recovery_path)?;
        require(
            identity(fresh_root.as_raw_fd())? == self.root_id
                && identity(fresh_recovery.as_raw_fd())? == self.recovery_id,
            "Root binding lost",
        )?;
        if self.opened {
            let parent = own(constrained(
                self.root_fd()?,
                &cstr(&self.parent_path)?,
                true,
                false,
            ))?;
            require(
                identity(parent.as_raw_fd())? == self.parent_id,
                "Parent binding lost",
            )?;
        }
        Ok(())
    }

    fn verify_observed(&self) -> Outcome<()> {
        let entry = self.probe(&self.leaf)?;
        match (&self.observed, entry) {
            (None, Entry::Absent) => Ok(()),
            (None, Entry::Present(_)) => Err(Failure::new("CONFLICT", "Destination appeared")),
            (Some(_), Entry::Absent) => Err(Failure::new("CONFLICT", "Observed AFTER changed")),
            (Some(_), Entry::Present(current)) => {
                let fd = current.as_raw_fd();
                require(
                    identity(fd)? == self.observed_id
                        && stat_of(fd)?.st_mode == self.observed_mode
                        && read_bytes(fd)? == self.observed_bytes,
                    "Observed AFTER changed",
                )
            }
        }
    }

    pub fn revoke(&mut self) {
        self.active = false;
    }

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

    fn init(&mut self, args: &[&[u8]]) -> Outcome<String> {
        require(
            args.len() == 5 && self.root.is_none(),
            "Invalid initialization",
        )?;
        self.root_path = unhex(args[1])?;
        self.root_id = text(unhex(args[2])?);
        self.recovery_path = unhex(args[3])?;
        self.recovery_id = text(unhex(args[4])?);
        self.root = Some(acquire_root(&self.root_path)?);
        self.recovery = Some(acquire_root(&self.recovery_path)?);
        let root = self.root_fd()?;
        let recovery = fd_of(&self.recovery)?;
        require(
            identity(root)? == self.root_id && identity(recovery)? == self.recovery_id,
            "Root identity changed",
        )?;

        // SAFETY: an all-zero statfs is a valid output buffer.
        let mut filesystem: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY: live descriptor and valid out-pointer.
        require(
            unsafe { libc::fstatfs(root, &mut filesystem) } == 0,
            "Filesystem unavailable",
        )?;
        let supported = [
            libc::TMPFS_MAGIC,
            libc::EXT4_SUPER_MAGIC,
            libc::BTRFS_SUPER_MAGIC,
        ];
        if !supported.contains(&(filesystem.f_type as libc::c_long)) {
            return Err(Failure::new(
                "UNSUPPORTED",
                "V1 requires local tmpfs, ext4 or Btrfs",
            ));
        }

        const WRITE_NEW: libc::c_int = libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL;
        const READ_WRITE_NEW: libc::c_int = libc::O_RDWR | libc::O_CREAT | libc::O_EXCL;
        const HARDENED: libc::c_int = libc::O_CLOEXEC | libc::O_NOFOLLOW;
        // SAFETY: literal NUL-terminated names; the descriptor is live.
        self.log = Some(own(unsafe {
            libc::openat(
                recovery,
                c"steps.jsonl".as_ptr(),
                WRITE_NEW | HARDENED,
                0o600,
            )
        })?);
        self.snapshot = Some(own(unsafe {
            libc::openat(
                recovery,
                c"operation.json".as_ptr(),
                READ_WRITE_NEW | HARDENED,
                0o600,
            )
        })?);
        flush(recovery)?;
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
        let log = fd_of(&self.log)?;
        let snapshot = fd_of(&self.snapshot)?;
        write_all(log, &entry)?;
        flush(log)?;
        // SAFETY: live descriptor; plain lseek/ftruncate on the snapshot file.
        let rewound = unsafe { libc::lseek(snapshot, 0, libc::SEEK_SET) } == 0
            && unsafe { libc::ftruncate(snapshot, 0) } == 0;
        require(rewound, "Journal snapshot unavailable")?;
        write_all(snapshot, &data)?;
        flush(snapshot)?;
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
        let relative = unhex(args[1])?;
        validate_relative(&relative)?;
        let (parent_path, leaf) = match relative.iter().rposition(|&byte| byte == b'/') {
            None => (b".".to_vec(), relative.clone()),
            Some(slash) => (relative[..slash].to_vec(), relative[slash + 1..].to_vec()),
        };
        self.parent_path = parent_path;
        self.leaf = leaf;

        let parent = own(constrained(
            self.root_fd()?,
            &cstr(&self.parent_path)?,
            true,
            false,
        ))?;
        self.parent_id = identity(parent.as_raw_fd())?;
        self.parent = Some(parent);
        self.opened = true;

        let parent_id = hex(self.parent_id.as_bytes());
        match self.probe(&self.leaf)? {
            Entry::Absent => Ok(format!("DATA\t0\t0\t\t\t{parent_id}")),
            Entry::Present(fd) => {
                let raw = fd.as_raw_fd();
                let observed_id = identity(raw)?;
                let mode = stat_of(raw)?.st_mode;
                let bytes = read_bytes(raw)?;
                let response = format!(
                    "DATA\t1\t{mode}\t{}\t{}\t{parent_id}",
                    hex(observed_id.as_bytes()),
                    hex(&bytes),
                );
                self.observed_id = observed_id;
                self.observed_mode = mode;
                self.observed_bytes = bytes;
                self.observed = Some(fd);
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
        self.staged_mode = mode;
        require((mode & 0o7000) == 0, "Special permission bits unsupported")?;

        // O_TMPFILE: an anonymous object with no basename anywhere in the namespace.
        let parent = self.parent_fd()?;
        // SAFETY: "." is a literal; the parent descriptor is live.
        let staged = own(unsafe {
            libc::openat(
                parent,
                c".".as_ptr(),
                libc::O_TMPFILE | libc::O_RDWR | libc::O_CLOEXEC,
                0o600,
            )
        })?;
        let raw = staged.as_raw_fd();
        write_all(raw, &self.staged_bytes)?;
        // SAFETY: live descriptor.
        require(
            unsafe { libc::fchmod(raw, mode & 0o777) } == 0,
            "Stage mode failed",
        )?;
        flush(raw)?;
        require(
            read_bytes(raw)? == self.staged_bytes,
            "Staged bytes changed",
        )?;
        let staged_id = identity(raw)?;
        self.staged = Some(staged);
        self.prepared = true;
        // Linux stages have no artifact name; the empty field is part of the contract.
        Ok(format!("STAGED\t{}\t", hex(staged_id.as_bytes())))
    }

    fn displace(&mut self, args: &[&[u8]]) -> Outcome<String> {
        require(
            args.len() == 2 && self.opened && !self.moved && !self.published,
            "Invalid displacement state",
        )?;
        self.verify_observed()?;
        let retention = unhex(args[1])?;
        require(
            retention.starts_with(b".nocturne-rollback-")
                && !retention.contains(&b'/')
                && !retention.contains(&0),
            "Invalid retention entry",
        )?;
        self.displaced_name = retention;
        if self.observed.is_none() {
            return Ok("OK".into());
        }

        // No source-inode CAS is claimed: RENAME_NOREPLACE only refuses to overwrite.
        let parent = self.parent_fd()?;
        let leaf = cstr(&self.leaf)?;
        let retained = cstr(&self.displaced_name)?;
        // SAFETY: both names are NUL-terminated and relative to the live parent descriptor.
        checked(unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                parent,
                leaf.as_ptr(),
                parent,
                retained.as_ptr(),
                RENAME_NOREPLACE,
            ) as libc::c_int
        })?;
        self.moved = true;
        flush(parent)?;

        let Entry::Present(displaced) = self.probe(&self.displaced_name)? else {
            return Err(Failure::new(
                "CONFLICT",
                "Competing displaced object preserved; conflict",
            ));
        };
        let raw = displaced.as_raw_fd();
        let displaced_id = identity(raw)?;
        require(
            displaced_id == self.observed_id
                && stat_of(raw)?.st_mode == self.observed_mode
                && read_bytes(raw)? == self.observed_bytes,
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
        if self.prepared {
            let staged = fd_of(&self.staged)?;
            require(
                read_bytes(staged)? == self.staged_bytes,
                "Staged bytes changed",
            )?;
            let parent = self.parent_fd()?;
            // The retained /proc/self/fd reference selects the anonymous source. No temp
            // name, no AT_EMPTY_PATH privilege, no fallback.
            let source = cstr(format!("/proc/self/fd/{staged}").as_bytes())?;
            let leaf = cstr(&self.leaf)?;
            // SAFETY: NUL-terminated names relative to live descriptors.
            checked(unsafe {
                libc::linkat(
                    libc::AT_FDCWD,
                    source.as_ptr(),
                    parent,
                    leaf.as_ptr(),
                    libc::AT_SYMLINK_FOLLOW,
                )
            })?;
            self.published = true;
            flush(parent)?;

            let Entry::Present(produced) = self.probe(&self.leaf)? else {
                return Err(classify_errno());
            };
            let produced_fd = produced.as_raw_fd();
            require(
                identity(produced_fd)? == identity(staged)?,
                "Published source identity diverged",
            )?;
            require(
                read_bytes(produced_fd)? == self.staged_bytes
                    && (stat_of(produced_fd)?.st_mode & 0o777) == (self.staged_mode & 0o777),
                "Produced state diverged",
            )?;
        } else {
            if let Entry::Present(_) = self.probe(&self.leaf)? {
                return Err(Failure::new("CONFLICT", "Deleted entry recreated"));
            }
            self.published = true;
        }
        if self.moved {
            let displaced = fd_of(&self.displaced)?;
            require(
                identity(displaced)? == self.observed_id
                    && read_bytes(displaced)? == self.observed_bytes,
                "Displaced bytes changed",
            )?;
        }
        self.admit()?;
        Ok("OK".into())
    }
}

fn fd_of(slot: &Option<OwnedFd>) -> Outcome<RawFd> {
    slot.as_ref()
        .map(AsRawFd::as_raw_fd)
        .ok_or(Failure::new("CONFLICT", "Filesystem operation refused"))
}

fn parse_mode(field: &[u8]) -> Outcome<libc::mode_t> {
    std::str::from_utf8(field)
        .ok()
        .and_then(|value| value.parse::<libc::mode_t>().ok())
        .ok_or(Failure::new("UNKNOWN", "Native protocol interrupted"))
}

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
