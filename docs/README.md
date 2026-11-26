# Documentation index

[Português do Brasil](README.pt-BR.md)

English is the canonical public documentation language. User-facing documents
have a `.pt-BR.md` companion with the same technical scope. Commands, paths,
API names, warnings and security claims are kept identical across the pair.

## Public user documentation

- [Installation](installation.md)
- [Getting started](getting-started.md)
- [Providers and models](providers.md)
- [Configuration](configuration.md)
- [Review, Build and Docs modes](modes.md)
- [Second Brain](second-brain.md) and [Awareness](awareness.md)
- [Code Intelligence — Phase 2](code-intelligence.md)
- [Engineering intelligence](engineering-intelligence.md)
- [Backup, restore and recovery](backup-and-recovery.md)
- [Data retention and recovery boundaries](data-retention.md)
- [Updates](updates.md)
- [Security boundaries](security.md) and [privacy](privacy.md)
- [Troubleshooting](troubleshooting.md)
- [Diagnostics](diagnostics.md)
- [Compatibility](compatibility.md)

## Contributor documentation

- [Architecture](architecture.md)
- [Development](development.md)
- [Docs Mode details](docs-mode.md)
- [Build recovery](build-recovery.md)
- [Database migrations](database-migrations.md)
- [Error handling](error-handling.md)
- [Performance budgets](performance.md)
- [Rollback filesystem boundary](rollback-filesystem-boundary.md)
- [Workspace evidence](workspace-evidence.md)
- [Contributing](../CONTRIBUTING.md)
- [Code of Conduct](../CODE_OF_CONDUCT.md)

## Maintainer/internal documentation

Release readiness, GitHub Actions, database migrations, provider contract,
performance budgets, product identity, security audit and the historical archive
are kept in the repository for engineering traceability. They are
not user support promises and do not all require translation.

The [model strategy](model-strategy.md) is a design guideline. It does not
describe automatic model routing or a reasoning-effort control available in
the current release.

The [GitHub Actions release guide](github-actions.md) describes the current
single-maintainer release procedure. The [1.0.2 readiness document](release-readiness-1.0.2.md)
records historical candidate validation; do not treat it as the current runbook.

The release notes of the Electron line (up to 1.0.5) are no longer on this
branch; they are in the tag `electron-final`. The
[release-candidate checklist](release-rc-checklist.md) remains a historical
record for the published stable line. The notes of the Rust line go in
[releases](releases/).

Superseded first-release drafts are grouped in the [historical archive](archive/README.md).
