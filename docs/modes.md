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

- Revisar's agent answers the chat in Revisar. Its default instruction is the sentence the chat has always used. The rules the
  shell adds after the instruction are not part of it and cannot be edited. Revisar never records a change: an answer there is
  text and findings, and nothing becomes pending.
- Construir's and Documentar's agents answer the chat in those modes, from the conversation button in the top bar. A complete
  answer may propose changes; each one waits for the person's approval, and the shell records it under the rule of its mode:
  - Construir: any path that the shared path rules accept.
  - Documentar: Markdown files only (`.md` or `.markdown`). The shell refuses any other path, and the answer shows why.
- "Gerar alteração", in the suggestions of Revisar, is an implementation step: its change waits in Construir under Construir's
  rule. It does not use an agent's instruction.
- Permissions belong to the mode and the shell enforces them. An instruction or a model cannot widen them, and an agent has no
  field for permissions.
- Known gap: approving a change in Documentar is not refused by the shell when the change is not Markdown. The list in
  Documentar shows Markdown only, and that filter is in the interface.
