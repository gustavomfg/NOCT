# Contributing

[Português do Brasil](CONTRIBUTING.pt-BR.md)

Thank you for your interest in contributing to **NOCT**.

Whether you are fixing a bug, improving documentation, proposing a feature or submitting code, your contribution is appreciated.

---

# Before You Start

Before opening a Pull Request, please:

- Search existing issues and discussions.
- Open an issue for significant behavior changes or new features.
- Keep Pull Requests focused on a single objective.
- Follow the project's architectural principles.

---

# Development Guidelines

## Architecture

NOCT is written in Rust. It is a Tauri shell (`crates/noct-app`), a Leptos interface compiled to
WebAssembly (`crates/noct-ui`) and a SQLite layer (`crates/noct-db`).

Please preserve the separation between:

- the interface, which calls only the commands the shell declares;
- the shell's commands, which check their arguments and the window that calls them;
- the persistence layer, which the commands use.

Native capabilities cross the command boundary. Avoid introducing unnecessary coupling between these layers.

---

## Security

Never commit:

- API keys
- Access tokens
- Local databases
- Logs containing sensitive information
- Personal credentials

The provider key lives only in the operating system's credential store: it must never reach the interface, the logs or
the repository. Files of a project are written only inside that project, and only after the person approves the change.
A change to a trust boundary is marked `[security-review]` in its commit and recorded in `docs/status.md`.

---

## Code Quality

Before submitting a Pull Request, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p noct-ui --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace
(cd crates/noct-ui && trunk build --release)
npm ci                              # once: installs Playwright for the two checks below
node scripts/check-shell-ui.mjs     # the interface in Chromium, with a mocked shell
node scripts/check-shell-csp.mjs    # the Content-Security-Policy in Chromium
```

The two Chromium checks read the build from `crates/noct-ui/dist`, so run them after `trunk build --release`. CI runs the
same steps (`.github/workflows/rust-shell-validation.yml`) and, on Linux, also the Node tests of the scripts. To try the
app in a window, use `node scripts/run-rust-shell.mjs dev --data-dir <empty temporary folder>`; never point it at a data
folder that holds real data.

Describe in the Pull Request the commands you ran and any manual validation.

---

# Commit Messages

Conventional Commits are recommended.

Examples:

```text
feat(provider): add provider registry

fix(files): refuse a path that leaves the project

docs(readme): update architecture overview

refactor(workspace): simplify awareness pipeline
```

---

# Pull Requests

A good Pull Request should:

- clearly describe the change;
- explain the motivation;
- mention possible risks;
- include manual validation steps;
- include screenshots for UI changes.

Small and focused Pull Requests are preferred over large changes.

---

# Development Environment

Recommended environment:

- Rust 1.97.1, which `rust-toolchain.toml` pins, with the `wasm32-unknown-unknown` target
- trunk 0.22.1 (`cargo install --locked trunk@0.22.1`)
- Node.js 24 LTS and npm 11+, for the Playwright checks and the Node tests of the scripts
- On Linux, the Tauri system packages listed in the README (section 5)

Any editor works: WebStorm (the maintainer's), or any editor with `rust-analyzer`.

---

# Project Philosophy

Every contribution should respect the project's design priorities:

1. Security before convenience.
2. Workspace before AI.
3. Knowledge before conversations.
4. Human control before automation.
5. Extensibility before provider-specific implementations.

Thank you for helping improve NOCT.
