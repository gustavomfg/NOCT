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

The product on this branch is written in Rust. It is a Tauri shell (`crates/noct-app`), a Leptos interface compiled to
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

Authentication data must never be exposed through the renderer.

---

## Code Quality

Before submitting a Pull Request, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
(cd crates/noct-ui && trunk build --release)
npm ci
npm test
npm run check:csp
npm run check:ui
```

Describe in the Pull Request the commands you ran and any manual validation.

---

# Commit Messages

Conventional Commits are recommended.

Examples:

```text
feat(provider): add provider registry

fix(ipc): validate renderer origin

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

- WebStorm
- Rust 1.97.1, which `rust-toolchain.toml` pins, with the `wasm32-unknown-unknown` target
- trunk 0.22.1 (`cargo install --locked trunk@0.22.1`)
- Node.js 24 LTS and npm 11+, for the Playwright checks and the Node tests of the scripts

The `webstorm` launcher should be available in your system `PATH`.

---

# Project Philosophy

Every contribution should respect the project's design priorities:

1. Security before convenience.
2. Workspace before AI.
3. Knowledge before conversations.
4. Human control before automation.
5. Extensibility before provider-specific implementations.

Thank you for helping improve NOCT.
