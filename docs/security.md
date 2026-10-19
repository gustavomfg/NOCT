# Security boundaries

[Português do Brasil](security.pt-BR.md)

## Electron and IPC

- `contextIsolation` and the renderer sandbox are enabled;
- Node integration is disabled;
- the preload exposes named methods, never a generic `ipcRenderer` object;
- IPC validates origin, payload shape, rate limits and workspace authorization;
- privileged operations and credentials stay in the main process;
- external navigation and browser permissions are denied by default.

## Workspace and execution

Paths are normalized and contained within the explicitly selected workspace.
Bounded reads reject traversal, absolute external paths and symlink escapes at
the relevant file boundary. Restored workspaces are unauthorized until the user
selects the folder again. Review is read-only. Build writes are scoped to the
workspace, and every command needs an explicit approval and runs without a shell.

Validation is an explicit request to run a stack-selected command in an
authorized workspace. The renderer cannot supply an executable or arguments:
Nocturne records the planned command and arguments, rechecks workspace
authorization immediately before spawn, rereads the selected `package.json`
script, and blocks the run if that script changed. Validation runs without a
shell and with a restricted inherited environment, but it deliberately executes
project-controlled code with the developer's OS privileges. It is not an OS
sandbox.

On POSIX, supervised validation, Git, document-export and agent command processes use a
dedicated process group and cancellation asks the group to terminate before an
escalation. Validation checks the group independently of the parent exit,
escalates after three seconds, and stops waiting after four seconds; if the
group or inherited pipes remain unconfirmed, it records termination as
uncertain. Descendants that leave the dedicated group cannot be certified as
terminated. Normal application shutdown also has a finite deadline. On Windows
the Node primitive can target only the direct child, so cancellation cannot
certify the entire tree. A detached terminal
launched for the user is deliberately outside Nocturne's lifecycle ownership.

Remote Chat Completions providers require HTTPS, reject redirects and validate
all resolved addresses before a connection is pinned. HTTP without TLS is
allowed only for local loopback services.

## Credentials and local data

On Linux desktops without Chromium's native keyring selection, startup detects
installed Secret Service or KWallet D-Bus services before Electron initializes.
Explicit `--password-store` switches and recognized desktop selection are
preserved. The credential vault still rejects `basic_text` and unavailable
storage; detection never enables plaintext encryption.

Provider keys are encrypted with Electron `safeStorage`, referenced by opaque
identifiers and excluded from backups and diagnostics. SQLite, WAL/SHM, snapshots,
workspace context and credential files use restrictive permissions where the
platform supports them. Logs are sanitized and verbose diagnostics are opt-in.

## Distribution

Packaged builds use ASAR, embedded integrity validation and Electron production
fuses. Stable releases require checksum verification and platform signing only
where the release policy claims it: currently Linux has a protected GPG-signed
checksum manifest, while Windows and macOS are unsigned and not notarized. These controls reduce
risk but do not constitute a security certification.
