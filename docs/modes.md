# Review, Build and Docs modes

[Português do Brasil](modes.pt-BR.md)

## Review Mode

Review is analysis-only. It reads the authorized workspace and produces
structured suggestions with evidence, confidence, source, severity, rationale
and decision history. A later review reconciles new and persistent findings and
resolves open findings no longer supported by current evidence. Review never
changes project files by itself.

## Build Mode

Build can modify files only inside the authorized workspace and under the active
approval policy. With a provider model, the agent uses Nocturne's workspace
tools: file writes stay inside the authorized root (generated directories,
`.git`, `.nocturne` and likely secret files are refused) and every command
requires explicit approval and runs without a shell. Progress, requested
approvals, changed files and diffs remain visible.

Guarded rollback is available only when the workspace was clean before the run,
there is a `HEAD` commit and the agent reported paths that remain within the
authorized root. Review the current diff before confirming a rollback.

## Docs Mode

Docs generates proposals read-only. The user selects a Markdown file, compares
the current and proposed content, then confirms append, replace or create. The
hash is checked again before writing, and the write is atomic. HTML, DOCX and
PDF exports are derived copies and are not incremental source-document updates.

Advanced autonomous build, documentation and orchestration features are outside
the current 1.0 contract.

## Agents of the modes

Each mode has one agent, listed in Ajustes › Agentes. An agent has a name that follows its mode, an instruction (text, up to
4 KiB) and a model. An empty model means the project's model. "Restaurar padrão" puts an agent back to its default instruction
and the project's model.

- The Review agent answers the chat. Its default instruction is the sentence the chat has always used. The rules the shell adds
  after the instruction (no commands, no writes, the answer format, at most five files) are not part of it and cannot be edited.
  As in Review Mode itself, a complete answer may leave its proposed changes as pending, for the person to approve.
- The Build and Docs agents can be edited and are stored, but no call uses them yet. Change generation still runs from Review,
  with its own prompt.
- Permissions belong to the mode and the shell enforces them. An instruction or a model cannot widen them, and an agent has no
  field for permissions.
- Known gap: in Docs only Markdown changes are listed, and this filter is in the interface. The shell does not refuse another
  kind of change when it is approved.
