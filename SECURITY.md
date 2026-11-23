# Security policy

[Português do Brasil](SECURITY.pt-BR.md)

NOCT is a local desktop application, but it handles project files, a provider key and communication with an external AI
service. Security reports are welcome.

## Reporting a vulnerability

Report privately, through **GitHub › Security › Report a vulnerability** (a private security advisory) on this
repository. Do not open a public issue for a vulnerability, and do not include secrets, project files or private prompts
in any report. If the private report is not offered on the repository page, open a minimal issue that says only that
you have a security report, with no details, and ask the maintainer for a private channel.

Include, when safe:

- the NOCT version (the installer's name carries it) or the commit;
- the operating system, its version and the architecture;
- the impact and a minimal reproduction;
- the expected and the actual behaviour;
- a mitigation or workaround, if known.

Before sharing logs, remove API keys, tokens, database files, project contents and personal information.

## Scope

The policy covers the desktop application: the Tauri shell and its commands, the interface (Leptos, WebAssembly) and its
Content-Security-Policy, workspace authorization and containment, file changes and their approval, persistence and
recovery, the OpenRouter integration and its network policy, credential storage, the installers and the updater
configuration. OpenRouter and the model providers behind it have their own security policies.

NOCT is in beta. Only the newest release receives fixes. The installers are not signed by the operating system
(README.md, "Install").

## Security design

- **The key stays in the operating system's credential store.** The OpenRouter key is written once to the vault
  (`crates/noct-secrets`), whose master key is in the operating system's credential store. It is never returned to the
  interface and never written to the logs; log lines are redacted before they are written.
- **Files are written only inside the project, and only after approval.** The assistant proposes changes; each one waits
  until the person approves it, and can be undone. The path rules refuse anything outside the project, symbolic links
  that leave it, Git and CI files, self-running files, device names and encoded paths
  (`noct_core::file_change`). Review Mode (Revisar) never writes.
- **Narrow commands.** The interface reaches native code only through the commands the shell declares, granted to the
  main window alone. Each command checks its input and returns a fixed error message; causes and paths stay in the shell.
- **A strict webview.** No inline script or style, no remote origin, navigation limited to the app; in a release build,
  no page menu, no reload or print shortcut, no inspector. Answers are drawn as text: Markdown without HTML, links not
  clickable, images not loaded.
- **A narrow network policy.** Provider calls go only to OpenRouter over HTTPS, follow no redirect, check every resolved
  address, use no proxy, and are bounded in time and size.
- **Consent per project.** Nothing is sent to the provider for a project until the person allows it.
- **Restored workspaces are untrusted** until the person chooses the folder again.

These are implemented mitigations, not a formal security certification. The record of each security review is in
[docs/status.md](docs/status.md).
