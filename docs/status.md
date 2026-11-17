# Migration status

Short record of each package of the Rust migration on `migration/rust`: what is done, its commits, its CI result, and
what is not verified. Only what is in a published commit is listed as done. The full verification tables are in
[the README](../README.md).

Legend: **CI** is the result of the Rust shell validation and the row gate for that commit, per system, read from
GitHub. Only finished runs are reported. **Not verified** lists what no run covers, such as a real window.

## Round 1: report of the queue P1–P10

### Packages

| Package | Commits | Pushed | CI | Not verified |
| --- | --- | --- | --- | --- |
| P1: open items | `a1c048d`, `cba4544`, `a508a37`, `2212de2`, `7d79ca1`, `0dafe7c` | `0dafe7c` | Row gate: success. Shell: success on Ubuntu, Windows and macOS. | Real window on Windows and macOS (CI opens none). |
| P2: IPC inventory | `3f2a9ce` | `f1f5b5f` | Shell: success on Ubuntu, Windows and macOS. Row gate: success. | Real window. |
| P3: read commands | `94a2dba` (`workspaces_list` paged) | `f1f5b5f` | Same run as P2: success on all three systems; row gate success. | A real window. The sidebar reads the first page only (round 2 covers the rest). |
| P4: database writes | `a94642c` (`workspace_set_favorite`), `35b99ac` (theme through `settings_set`), `conversations_create` (`93af490`), `brain_create` (`595bb08`), `brain_update` (`a17000d`), `brain_extract` (`9ebcbc9`), `suggestions_create` (`e383ec2`), `suggestions_status` (`2000228`), `models_set_bindings` (`cfa48ec`), `ai_save_assistant` (`e9064c4`), `conversations_delete` (`ecebef1`), `workspace_remove` (`44f2fbf`), `brain_delete` (`1028507`), `artifacts_delete` (this change) | `f1f5b5f` | `93af490`, `595bb08`, `a17000d`, `9ebcbc9`, `e383ec2`, `2000228`, `cfa48ec`, `e9064c4`, `ecebef1`, `44f2fbf` and `1028507`: Shell success on all three systems; row gate success. | No control sets a favorite yet, and no control creates a conversation, changes a memory, reads a reply, moves a suggestion, saves model bindings, saves an assistant reply, deletes a conversation, removes a project, deletes a memory, or deletes an artifact. `conversations_create` does not create the `.nocturne` workspace context. A failed write is forced in eight tests (the conversation insert, the memory insert's history entry, the update's history entry, a refused candidate in a batch, the suggestion's decision entry on a review and on a status change, the model bindings row, and the conversation row). Each delete has its own failed-write test: a refused `workspaces`, `brain_memories` or `artifacts` delete leaves the row in place. |
| P5: observability | `b5b27ef` | `f1f5b5f` | Same run: success on all three systems; row gate success. | Warnings in a real window and log file. The favorite write is not measured: it has no catalog name. |
| P6: native rollback, macOS and Windows | `3f0e6f3` (comment only) | `f1f5b5f` | Same run: success. No backend was ported in round 1. | Both backends were missing in round 1. See round 2. |
| P7: UI slices | `1103543` (`Construir` and `Documentar` not yet available), editable Ajustes (this change) | `1103543` | Shell and row gate on `1103543`: success on Ubuntu, Windows and macOS; row gate success. | Real window. Chromium is the approximation the check uses. |
| P8: updater | `tauri-plugin-updater` 2.14.1 pinned, configured empty, registered only when complete and safe (this change) | | | |
| P9: release dry run | stopped in round 1 | | | |
| P10: documents | `85f4cb4`, `5748b33`, `f1f5b5f`, `4a62c3c` | `4a62c3c` | Shell and row gate on `4a62c3c`: success on Ubuntu, Windows and macOS; row gate success. | The phase table is not revised beyond ADR decision 4, the counts and the command list. |

### CI by system (finished runs only)

- `f1f5b5f` (P2 to P6): Rust shell validation success on Ubuntu, Windows and macOS; row gate success.
- `4a62c3c` (status only): Rust shell validation success on Ubuntu, Windows and macOS (run `44038625669`); row gate
  success (run `44038625533`).
- Earlier heads `a94642c` and `0dafe7c`: shell and row gate success on all three systems.

### Local validation (round 1 tree, P5 first cut)

- `cargo fmt --check`; `cargo clippy --workspace --all-targets -D warnings`; `cargo clippy -p noct-ui --target wasm32-unknown-unknown -D warnings`.
- `cargo test --workspace --no-fail-fast`: 456 passed, 0 failed, 19 ignored.
- `node scripts/verify-row-gate.mjs`: exit 0, including the reverse check.
- `npm run typecheck`, `npm run lint`, `npm test` (95 files, 602 tests), `node --test scripts/*.test.mjs` (7 of 7).
- `node scripts/check-shell-csp.mjs`; `node scripts/check-shell-ui.mjs` (17 scenarios, after `trunk build --release` of `crates/noct-ui`).
- Output differential: `node scripts/generate-output-differential.mjs`, then
  `cargo test -p noct-app --test output_differential -- --ignored`: passes for the nine release lines and the current line.

The TypeScript, Vitest, CSP and browser checks were last run on the theme-save commit. The later commits change none of their files.

### P1 detail

- Rust writes, TypeScript reads and writes, Rust reads back: `a1c048d` adds `crates/noct-db/tests/rust_written_gate.rs`,
  `scripts/verify-rust-written-gate.mjs` and `scripts/row-gate-typescript.mjs`. `verify-row-gate.mjs` runs it.
- Vitest excludes `scripts/**/*.test.mjs` (`cba4544`). Those files use `node:test` and run with `node --test`, as CI does.
- Documents: `a508a37` and `0dafe7c` describe the reverse gate and the platform floors in the READMEs.
- `2212de2`: macOS durability keeps `fsync`, as the C++ worker does. Round 2 changes this to `F_FULLFSYNC` (see below).
- `7d79ca1`: platform floors recorded as the project decision of 2026-11-17 in `docs/compatibility.md` and its Portuguese copy.

### P2 detail

- `3f2a9ce`: `docs/ipc-inventory.md`, generated by `scripts/ipc-inventory.mjs`. It covers 108 channels: 99 invokable and
  9 events. The generator fails if a channel has no classification, no preload entry or no handler. Every invokable
  channel has an input check, found in the handler or in a helper it calls. Four channels already have a Rust command.

### P3 detail (first cut)

- `workspaces_list` takes a page request: `limit` is required, from 1 to 500; `offset` is optional, from 0 to 1,000,000.
  Any other field, a null offset and a missing limit are refused as invalid input. The reply is `{ items, hasMore }`.
- The shell reads the rows of the page under the store lock, then checks the availability of those rows only.
- The sidebar asks for the first page at 500, lists it, and shows a note when `hasMore` is true.
- Round 2 replaces the offset with a key cursor (see round 2).

### P4 detail (first cut)

- `workspace_set_favorite` takes `{ workspace, favorite }`. Both fields are required. `workspace` has from 1 to 4,000
  characters; `favorite` is a boolean. Any other field, a null, or a wrong type is refused as invalid input before the
  store is read.
- The project must be saved. A path that is not saved is refused with `unknown-workspace`, and nothing is written. The
  check and the write run under one store lock.
- The write is one `UPDATE`, so SQLite leaves the row as it was if the statement fails. A forced failure is not tested.
- The command is granted to the main window only. The interface does not call it yet.
- The sidebar theme button saves the theme with `settings_set` (`{ input: { theme } }`). A save the shell refuses keeps the
  theme on screen and shows the shell's message beside the button.

### P4 detail: `conversations_create`

- The argument is the project path as a string, the same as `conversations.create(workspace)`. An empty or non-string
  argument is invalid input before the store is read.
- The project must be saved, authorized, available (`workspace_scope::inspect`) and outside the blocked roots. Any of these
  refusals is `unknown-workspace`, and nothing is written. The TypeScript shell gives a different message for each refusal;
  the Rust shell gives the one message of `unknown-workspace` for all of them.
- The workspace row and the conversation are written in one transaction (`Store::create_conversation`). A refused
  conversation insert rolls the workspace row back too. The test installs a trigger that refuses the insert and checks that
  no workspace row remains. With the transaction removed, the test fails.
- **Not ported:** the `.nocturne` workspace context that the TypeScript shell creates before the insert (`project.json`,
  `memory.md`, `rules.md`, with project detection). The Rust command writes the database only. This is a gap against the
  TypeScript behaviour, and it belongs with the context commands (`memory:get`, `memory:set`, later). The inventory says so.
- The command is granted to the main window only. The interface does not call it yet.
- Checks: `crates/noct-app/tests/services.rs` (two tests), `crates/noct-app/tests/commands_acl.rs` (one test),
  `crates/noct-db/tests/store.rs` (the failed write). The command is not measured and has no fixture in `noct-core`: its
  argument is a bare string, and its output is the existing `Conversation` contract.
- Local validation: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace`
  (480 passed, 0 failed, 19 ignored; this change adds four tests), `node scripts/generate-ipc-samples.mjs --check`.

### P4 detail: `brain_create`

- The request is `{ conversationId, kind, scope, content }`, checked by the request contract before the store is read. It
  follows `brainMemoryCreateSchema`: `conversationId` is a UUID; `kind` and `scope` are the enumerated values; `content` is
  trimmed, has from 1 to 8,000 UTF-16 units, and is not a credential (`is_safe_brain_memory_content`). An unknown or missing
  field, or a wrong type, is `invalid-input`. The refusal of a credential is `invalid-input` too, where the TypeScript parse
  gives its own message.
- The conversation must exist, otherwise `unknown-conversation` with the TypeScript message `Conversa não encontrada.`. Its
  project must pass the same check as a conversation write (`authorized_workspace`, shared with `conversations_create`).
  A refusal there is `unknown-workspace`. Nothing is written in any refusal.
- The memory is a manual candidate. A conversation-scoped memory keeps its conversation; a workspace-scoped one keeps none.
  The repository writes the memory and its history entry in one transaction. A failed history write removes the memory too
  (`a_failed_brain_history_write_leaves_no_memory_behind`).
- Not ported: the log line the TypeScript shell writes for each memory. The interface does not call the command yet.
- Checks: `crates/noct-core/tests/ipc_brain_create.rs` (eight tests of the request), `crates/noct-app/tests/services.rs` (two
  tests), `crates/noct-app/tests/commands_acl.rs` (one test), `crates/noct-db/tests/store.rs` (the failed history write).
- Tests after this change: 492 passed, 0 failed, 19 ignored in the workspace.

### P4 detail: `brain_update`

- The request is `{ conversationId, memoryId, kind?, scope?, content?, confidence?, status? }`, checked by the request contract
  before the store is read. It follows `brainMemoryUpdateSchema`: both ids are UUIDs; at least one field must be present, and
  the refusal says so in the TypeScript words (`Informe ao menos uma alteração.`); `content` follows the create rules;
  `confidence` is a whole number from 0 to 100 (`80.0` is accepted, as JavaScript reads it as 80); `kind`, `scope` and
  `status` are the enumerated values. An unknown field, or a wrong type, is `invalid-input`.
- The conversation and the project are checked as in `brain_create`: `unknown-conversation`, then `unknown-workspace`.
- The memory must exist in that project: `unknown-memory`, with the TypeScript message `Memória não encontrada.`.
- A status change must follow the lifecycle. The transition table moved from the repository into `noct-core`
  (`allowed_transition`), so the service and the repository share one definition. A refusal is `invalid-transition`, with
  the message `Transição de memória inválida.`. **Deviation:** the TypeScript message also names the two statuses
  (`candidate → archived.`); the Rust message does not. A status that stays the same is not a change, as in TypeScript.
- Scope follows the request, as in TypeScript: `conversation` sets the request's conversation, `workspace` clears it, and an
  absent scope keeps the memory's own.
- The repository writes the change and its history entry in one transaction. A test refuses the history entry and checks the
  memory keeps its content, and that the refusal is the trigger's (not a foreign-key error).
- Not ported: the log line the TypeScript shell writes per update. The interface does not call the command yet.
- Checks: `crates/noct-core/tests/ipc_brain_update.rs` (six tests), `crates/noct-app/tests/services.rs` (three tests),
  `crates/noct-app/tests/commands_acl.rs` (one test), `crates/noct-db/tests/store.rs` (the failed history write).
- Tests after this change: 503 passed, 0 failed, 19 ignored in the workspace.
- A correction found while writing this: the two earlier failed-write tests (`conversations_create` and `brain_create`) passed
  without reaching their trigger when the workspace row was missing (a foreign-key error). They now create the workspace first
  and assert the trigger's message, so each one fails only because of the trigger.

### P4 detail: `brain_extract`

- The request is `{ conversationId, content }`. The reply is kept as it is: the request does not trim it. `content` has at most
  2,000,000 UTF-16 units (`ASSISTANT_CHARACTERS`), as `brainMemoryExtractSchema` allows.
- The reply is read by `extract_candidates` in `noct-core`, which follows `extractBrainMemoryCandidates`:
  - the block marker is ASCII-folded, as the `/i` flag of JavaScript is (a long `ſ` does not match the `s`);
  - a block that is not JSON is skipped; inside a block, an item that does not match the candidate schema is skipped and the
    others are kept; a single object counts as a one-item list;
  - `confidence` defaults to 60 and is a whole number from 0 to 100; no other key is allowed;
  - at most five candidates are kept; every block is removed from the reply, kept or not, and the reply is trimmed.
- The content rule of a candidate is the one of `brain_create` (`normalize_brain_content`): trimmed, 1 to 8,000 UTF-16 units,
  and not a credential. The rule now has one definition, used by the three memory commands.
- The conversation and its project are checked as in `brain_create`. The candidates are stored with
  `create_candidates`: equivalent memories are skipped, the rest are stored as agent candidates in one transaction.
- A failed store is not an error. The reply comes back with `memories: []` and the TypeScript warning
  (`A resposta foi preservada, mas não foi possível salvar as candidatas do Segundo Cérebro.`), as the TypeScript shell does.
  `warning` is absent when there is none.
- Known difference, not covered by a test: the request is read as a Rust string. A lone surrogate escape in the reply is
  refused by the request parser, where `JSON.parse` in the TypeScript shell accepts it.
- Not ported: the log lines of the TypeScript shell. The interface does not call the command yet.
- Checks: `crates/noct-core/tests/brain_candidates.rs` (nine tests), `crates/noct-app/tests/services.rs` (three tests),
  `crates/noct-app/tests/commands_acl.rs` (one test), `crates/noct-db/tests/store.rs` (the batch that is refused as a whole).
- Tests after this change: 517 passed, 0 failed, 19 ignored in the workspace; `cargo fmt --check`, clippy with `-D warnings`
  and `node scripts/generate-ipc-samples.mjs --check` pass.

### P4 detail: `suggestions_create`

- The request is `{ conversationId, content }`, the shared `ReplyRequest` of `brain_extract`, with the same
  `ASSISTANT_CHARACTERS` bound on `content`. The reply is kept as it is.
- The reply is read by `extract_suggestions` in `noct-core`, which follows `extractSuggestions` in `shared/suggestions.ts`:
  - one marker and one block, read strictly: every item must match `suggestionInputSchema`, or the whole block is refused.
    Two markers, or a block that is not an array, is invalid;
  - without a marker, a JSON fence at the very end of the reply is read the same way;
  - the marker is ASCII-folded and ends at a word boundary; the block marker does not need the boundary;
  - a valid snapshot is removed from the reply and the reply is trimmed. An invalid one leaves the reply as it is, trimmed.
- The item schema is ported field by field: lengths in UTF-16 units after the trim (zod order), the title sanitized
  after its 200-unit check, `affectedFiles` at most 100, `expectedBenefits` at most 20, `evidence` at most 50 with no
  unknown key in an item, `proposedChanges` at most 100,000 units and not trimmed, `confidence` a whole number from 0
  to 100 (`80.0` is 80), and the defaults of the schema (`medium`, `[]`, `60`, `Análise do agente`, `Agente de revisão`).
  An unknown key on the item is ignored, as zod strips it.
- A structured reply is reconciled by `Suggestions::reconcile` with `resolve_missing = false`, as the TypeScript handler
  does. The reply comes back with the suggestions, the comparison, and the content: the trimmed text and
  `review_comparison_markdown`, joined by a blank line, with the empty part left out.
- A reply without a usable snapshot stores nothing and comes back with `suggestions: []`, its trimmed text and one of the
  two TypeScript warnings. It has no `comparison` key, as the TypeScript reply has none.
- The conversation and its project are checked as in `brain_extract`: a conversation that does not exist is
  `unknown-conversation`, and a project that is not saved, authorized, available or allowed is `unknown-workspace`.
- Unlike `brain_extract`, a store failure is an error, not a warning: the TypeScript handler has no fallback for it either,
  and the whole reconciliation is one transaction.
- Known difference, not covered by a test: the same lone-surrogate difference as `brain_extract`.
- Not ported: the `logger.info` line of the TypeScript handler. The interface does not call the command yet.
- Shared types moved for this change, with no change of behavior: `ReplyRequest` now lives at `noct_core::ipc` (it was
  under `ipc::brain`), and `ascii_insensitive` now lives in `noct_core::text`, used by the memory and suggestion parsers.
- Checks: `crates/noct-core/tests/suggestion_extraction.rs` (eleven tests: the strict block, the fallback, the markers, the
  schema bounds and defaults, the edge values, and the comparison text), `crates/noct-app/tests/services.rs` (three tests,
  one of them a second review that keeps the same finding as persistent), `crates/noct-app/tests/commands_acl.rs` (one
  test), `crates/noct-db/tests/store.rs` (a refused decision entry rolls back the suggestion written by the same call).
- Tests after this change: 533 passed, 0 failed, 19 ignored in the workspace; `cargo fmt --check`, clippy with `-D warnings`
  and `node scripts/generate-ipc-samples.mjs --check` pass.

### P4 detail: `suggestions_status`

- The request is `{ conversationId, suggestionId, status, result? }`. `status` is one of the seven values of
  `suggestionStatuses`, spelled as in TypeScript (`in-analysis`). `result` is at most 20,000 UTF-16 units
  (`SUGGESTION_RESULT_CHARACTERS`), kept as it is, and `null` is refused. An unknown field is refused.
- The checks run in the order of `suggestionStatusSchema` and the TypeScript handler: the conversation must exist
  (`unknown-conversation`), its project must be saved, authorized and available (`unknown-workspace`), the suggestion must
  belong to the conversation (`unknown-suggestion`, with the TypeScript message), and the move must be allowed
  (`invalid-suggestion-transition`).
- The transition table is `allowed_suggestion_transition` in `noct-core`, moved from the store so the service checks it
  before it writes. It was checked pair by pair against the table of `SuggestionRepository.setStatus`
  (`crates/noct-core/tests/suggestion_transitions.rs`). Setting the status a suggestion already has changes nothing and
  writes no decision, as in TypeScript.
- The store writes the status, the result and the decision entry in one transaction, as the TypeScript repository does.
  The returned suggestion carries the new status and the decision in its history; the result is in that history entry,
  not in a top-level field, as in the TypeScript `Suggestion`.
- Codes and messages: the messages of the two new refusals are the TypeScript ones (`Sugestão não pertence a esta conversa.`
  and `Transição de sugestão inválida.`). The transition message does not name the two statuses, as the memory one does
  not. The `invalid-suggestion-transition` code is new; the TypeScript shell throws a plain error.
- Not ported: the `.nocturne` decision log. The TypeScript handler writes the decision into the project's `.nocturne`
  folder after the store update, and logs a warning when that write fails. The Rust command does not write there yet, the
  same gap as `conversations_create` for the project context. The interface needs this decided before users rely on the
  history file. Recorded as an open item.
- Known difference, not covered by a test: the same lone-surrogate difference as `brain_extract` for the request.
- Checks: `crates/noct-core/tests/suggestion_transitions.rs` (two tests: all 49 pairs against the table, and the final
  states), `crates/noct-core/tests/ipc_suggestion_status.rs` (six tests: the spelling, the list, the null and unknown and
  missing fields, the 20,000-unit edge with astral characters, the uuids), `crates/noct-app/tests/services.rs` (two
  tests: a full lifecycle with a decision count per move, and the order of the checks), `crates/noct-app/tests/commands_acl.rs`
  (one test), `crates/noct-db/tests/store.rs` (a refused decision entry leaves the status and the result unchanged).
- A mutation check: with the service's transition check disabled, the lifecycle test fails at the refusal, so the test
  depends on the check. The file was restored and checked after the run.
- Tests after this change: 545 passed, 0 failed, 19 ignored in the workspace; `cargo fmt --check`, clippy with `-D warnings`
  and `node scripts/generate-ipc-samples.mjs --check` pass.

### P4 detail: `models_set_bindings`

- The argument is the bindings object itself, as `models.setBindings` takes it. The shape is the one the core contract
  already had (`WorkspaceModelBindings`, with `deny_unknown_fields`, the explicit-null refusal of `.optional()`, and the
  zod field order); the contract tests in `crates/noct-core/tests/ai_contracts.rs` cover it, so this change does not add
  parser tests.
- Order, as in the TypeScript handler: the bindings are validated (the identifier is trimmed and bounded, and each model
  reference is checked), then the project is checked. The check is the one of a conversation write: saved, authorized and
  available, and not in a blocked root. TypeScript's `getAuthorizedWorkspace` requires the same, through
  `assertSafeWorkspaceScope`.
- The write is the repository's `set`: one upsert per project, which replaces the bindings and the update time. The stored
  JSON has the field order of the TypeScript record, so a database written by either shell reads in the other.
- Refusals: a request that does not validate is `invalid-input`; a project that does not qualify is `unknown-workspace`.
  Its message is the one the other project commands use: the TypeScript shell gives a different message for each
  refusal, and the Rust shell gives one for all of them (see `conversations_create`).
- Not ported: nothing of the TypeScript handler is left out. The reading side, `models.bindings`, is a P3 read and is not
  part of this change.
- Checks: `crates/noct-app/tests/services.rs` (two tests: a store with a trimmed identifier and a replacement with one record
  per project; the refusals of an unsaved, a blocked and an empty project, with nothing stored), `crates/noct-app/tests/commands_acl.rs`
  (one test: the grant, an unknown field, an explicit null, an unknown key in a model reference, and an unsaved project),
  `crates/noct-db/tests/store.rs` (a refused row leaves no bindings behind).
- A mutation check: with the project check removed from the service, the refusal test fails. The file was restored and checked.
- Tests after this change: 549 passed, 0 failed, 19 ignored in the workspace; `cargo fmt --check`, clippy with `-D warnings`
  and `node scripts/generate-ipc-samples.mjs --check` pass.

### P4 detail: `ai_save_assistant`

- The argument is `{ conversationId, content, metadata? }`. The schema of `saveAssistant` is not strict, so an unknown field
  is ignored (zod strips it). This differs from the reply commands of suggestions and memories, which are strict, and the
  test `an_unknown_field_is_ignored_because_the_schema_is_not_strict` pins it.
- `content` is kept as it is and is bounded by `ASSISTANT_CHARACTERS`. `metadata` is any JSON value within the metadata
  limit (`serialize_within_limit`: nesting and size, as `isJsonValueWithinLimit`). An explicit `null` is no metadata, which is
  how the TypeScript record stores it (`NULL`).
- The artifacts come from the metadata, as the TypeScript handler reads them. `turn_artifacts` in `noct-core::ai::turn`: each
  object in `files` with a non-empty string `path` is a file artifact, with its kind from the extension (`artifact_kind`,
  the same groups as `artifactType`, compared without case) and its title from the basename. A non-empty `diff` is a
  report titled `Alterações do turno`. A metadata that is not an object gives none.
- `basename` follows Node's `path.basename` for the system it runs on: `/` is the separator, and `\` is one too on Windows.
  A Windows drive prefix is not removed; the TypeScript `path.basename` on Windows removes it. This is a known, untested gap.
- The message, the Markdown artifact and the file artifacts are written in one transaction by `save_assistant_turn`, which
  already existed. A refusal of any part leaves none of them (the existing test `a_refused_artifact_rolls_back_the_whole_turn`).
- Known deviation: the title of the Markdown artifact is `Resposta · ` and the UTC time, `2026-11-17 14:03:22 UTC`. The
  TypeScript shell writes `Resposta · ` and the local time from `toLocaleString()`, which depends on the system's locale and
  zone. The shell has no local-time formatter yet. The interface shows this title, so the choice needs a decision before the
  interface uses the command.
- Not ported: nothing else. The TypeScript handler has no log line of its own.
- Checks: `crates/noct-core/tests/ipc_ai_save.rs` (four tests: the content and the optional metadata, the ignored unknown
  field, the depth and size limits of the metadata, the content bound and the required fields), `crates/noct-core/tests/turn_artifacts.rs`
  (six tests: the kinds, the basename, the files, the diff, the shapes that give none, and the order), `crates/noct-app/tests/services.rs`
  (two tests: the stored reply, its metadata and its four artifacts; the refusals of an unknown conversation and a blocked
  project, with nothing stored), `crates/noct-app/tests/commands_acl.rs` (one test).
- Dependency: `time` is now a direct dependency of `noct-app` (`time.workspace = true`), for the title. It is the version
  `noct-db` already uses; the lockfile records the new edge only.
- Tests after this change: 562 passed, 0 failed, 19 ignored in the workspace; `cargo fmt --check`, clippy with `-D warnings`
  and `node scripts/generate-ipc-samples.mjs --check` pass.

### P4 detail: `conversations_delete`

- The argument is the conversation id as a bare string, as `conversations_create` takes the project path. Any other JSON
  shape is `invalid-input`, and so is an id that is not a UUID (`idSchema`).
- Order, as in `getAuthorizedConversation`: the conversation must exist (`unknown-conversation`), then its project must be
  saved, authorized, available and not blocked (`unknown-workspace`). Only then is the row deleted.
- The delete is the repository's `DELETE FROM conversations`. The messages, artifacts and suggestions of the conversation go
  with it by their `ON DELETE CASCADE` foreign keys, which the store turns on at open. Nothing is removed from the project's
  folder, as in TypeScript.
- Known difference, not covered by a test: brain memories are not linked to a conversation by a foreign key in the schema,
  so a memory of a deleted conversation keeps its `conversation_id`. TypeScript has the same schema and does the same; the
  behavior is the schema's, and the test does not pin it.
- Checks: `crates/noct-app/tests/services.rs` (two tests: the cascade removes the messages and artifacts of one conversation
  and leaves the other intact; the refusals of a malformed id, a missing conversation and a blocked project, with nothing
  removed), `crates/noct-app/tests/commands_acl.rs` (one test: the grant, the argument shape, the missing conversation, and
  the listing after the delete), `crates/noct-db/tests/store.rs` (a refused delete leaves the conversation in place).
- Tests after this change: 566 passed, 0 failed, 19 ignored in the workspace; `cargo fmt --check`, clippy with
  `-D warnings` and `node scripts/generate-ipc-samples.mjs --check` pass.

### P7 detail: Construir and Documentar not yet available

- Construir and Documentar no longer show the example command, the example diff, or the approval, revert and confirmation
  controls. Each shows its heading, its description, and the note `Ainda não disponível nesta versão. Nada é executado nem
  gravado nesta tela.` (`data-testid="unavailable"`).
- Why: the approval, revert and confirmation buttons changed local state only. A control that looks approvable and runs
  nothing misleads the person about what happened, so it is removed rather than labelled.
- Removed with them: `EXAMPLE_COMMAND`, `EXAMPLE_BUILD_FILE`, `EXAMPLE_BUILD_DIFF`, `EXAMPLE_DOC_FILE`, `EXAMPLE_DOC_DIFF`, the
  diff types and their two line renderers. The findings, the model and the vault stay as EXAMPLE data, unchanged.
- Scenarios: 17 (16 before). The new `build-and-document-not-yet-available` checks each mode's heading and note, that it
  offers no Aprovar, Negar, Reverter or Confirmar alteração control, and that no `.diff`, `.prose` or `.cmd` element is shown.
  The `full` scenario no longer clicks Aprovar or Confirmar alteração.
- A mutation check: a hidden Aprovar button put back into the unavailable panel makes the new scenario fail on both modes.
  The source was restored, and the Trunk build was run again.
- Checks: `trunk build --release` in `crates/noct-ui` passed; `node scripts/check-shell-ui.mjs` 17 of 17; clippy with
  `-D warnings` for `wasm32-unknown-unknown` passed; the workspace has fmt, clippy `-D warnings`, and 578 tests passed, 0
  failed, 19 ignored.
- Not verified: a real window. Chromium is an approximation of the webview.

### P8 detail: updater (prepared, not configured)

- Dependency: `tauri-plugin-updater` `=2.14.1` in `Cargo.toml` (`[workspace.dependencies]`), used by `noct-app` only. It is the
  latest stable 2.x; the 3.0.0 line is an alpha and was not used. It requires `tauri ^2.12`, which the workspace has at 2.12.5.
- Lock file: 47 package entries added (the plugin and its TLS stack: `rustls`, `ring`, `rustls-webpki`, `hyper-rustls`, `zip`,
  `tar`, `minisign-verify`, and a second `windows-sys` 0.52 used only by the plugin). No existing package changed version.
- Licences: `cargo deny` refused `ISC` (`ring`, `rustls-webpki`, `untrusted`) and `CDLA-Permissive-2.0` (`webpki-root-certs`).
  Both are permissive and are now allowed in `deny.toml`, with the reason in a comment. This is a policy change, so the owner
  should confirm it.
- Configuration: `plugins.updater` in `crates/noct-app/tauri.conf.json` is `pubkey: ""`, `endpoints: []`,
  `requireSignedVersion: true`. `requireSignedVersion` is required once the updater is configured; that default is a decision
  of this change, for the owner to confirm.
- Registration: `crates/noct-app/src/updater.rs` decides from the section. The absent section and the placeholder register
  nothing. A complete and safe section registers the plugin. A partial, insecure, unsafe-flagged, unsigned or malformed section
  is refused, and `run()` stops at start with that refusal. The window is granted no updater permission.
- Checks: `cargo test -p noct-app --lib updater` 10 passed (the shipped section, the absent section, a complete one, the
  partial ones, http and mixed endpoints, uppercase https, each unsafe flag, the signed-version check, malformed values);
  `the_window_is_granted_no_updater_permission` passed. The workspace has 592 passed, 0 failed, 19 ignored. `cargo deny check`: advisories, bans, licences and sources ok.
  `cargo audit` exit 0 with the seven allowed warnings already in the tree (Linux GTK and zbus crates).
- Not verified: an update request, a signature check, and an install. The shell makes no request with the shipped section, and
  there is no key or feed to check against. Nothing was published or signed.
- Owner's to do: the key pair, the feed and its host, and filling the section in a reviewed change.

### P7 detail: editable Ajustes

- Ajustes edits Idioma, Segurança do agente and Diagnóstico. Each change is one `settings_set` with that field alone, and
  the area shows the settings the shell replies with.
- Each select carries the value the contract uses (`pt-BR` and `en`; `untrusted` and `on-request`; `on` and `off` for the
  boolean). A choice is read back from that value, never from its words. Tests: `the_choices_carry_the_values_the_contract_uses`
  and `a_choice_is_read_back_from_the_value_the_select_carries` in `crates/noct-ui/src/settings.rs`.
- While a save runs, the area says `Salvando…`. A refused save shows the shell's message in an alert, and the select goes back
  to the saved value: the reactive state is notified again, so the controls read the settings the shell holds.
- Tema is not a select here. It keeps its one control, the sidebar button, which changes the theme on screen, saves it, and
  shows a refused save beside it. The Tema row shows the saved theme.
- A correction: the note of Ajustes said the theme button changed the theme "for this session only". The button saves the
  theme, and the check has always required the save. The note, `rust-shell.md` and the code comments now say it saves.
- The mock keeps the settings the shell holds: a save changes them, and the next reply shows the change. Before, a save
  returned the same settings, so a check could not see a change come back.
- Scenarios: still 17. The `full` scenario saves each of the three settings and checks the payload and the value the reply
  shows. `theme-not-saved-says-so` refuses a language save and checks the alert and the revert. The save count is split:
  theme saves (2 in `full`, 1 refused) and Ajustes saves (3 in `full`, 1 refused).
- A mutation check: without the revert of a refused save, `theme-not-saved-says-so` fails on `the language goes back to the
  saved value`. The source was restored and the Trunk build was run again.
- Checks: `cargo test -p noct-ui` 30 passed; clippy with `-D warnings` for `wasm32-unknown-unknown`; `trunk build --release`;
  `node scripts/check-shell-ui.mjs` 17 of 17; the workspace has fmt, clippy with `-D warnings`, and 581 tests passed, 0
  failed, 19 ignored.
- Not verified: a real window, and how a native select looks in the webview of each system. Chromium only.

### P4 detail: `artifacts_delete`

- The argument is `{ conversationId, artifactId }`, both UUIDs, and the object is strict (`artifactDeleteSchema` in
  `registerArtifactIpc.ts`): an extra field is `invalid-input`.
- Order, as in TypeScript: the conversation must exist (`unknown-conversation`), its project must be saved, authorized,
  available and not blocked (`unknown-workspace`), and then the row is deleted where the artifact belongs to that
  conversation. An artifact of another conversation is not found and not touched.
- Only the row is removed. The file the artifact names stays in the project folder. TypeScript does the same, so the
  "does it also remove files" question of the plan is answered: no.
- The reply is `{ deleted: true }`. An artifact that is not there, or was already removed, is `unknown-artifact` with the
  TypeScript message `Artefato não encontrado ou já removido.`. The message is the same as TypeScript's.
- A reply stores its Markdown copy as an artifact too, so a reply with one file has two artifacts. The tests find the file
  artifact by its path.
- Checks: `crates/noct-core/tests/ipc_artifact_delete.rs` (the two ids, the strict object, the missing keys),
  `crates/noct-app/tests/services.rs` (one test: a delete once, the other conversation's artifact kept, the file left on
  disk, a second delete and a missing conversation refused), `crates/noct-app/tests/commands_acl.rs` (one test, with the
  message of the refusal), `crates/noct-db/tests/store.rs` (a refused delete leaves the artifact in place).
- A mutation check: with the conversation scope removed from the repository's delete, the service test fails. The file was
  restored and checked.
- Tests after this change: 578 passed, 0 failed, 19 ignored in the workspace; `cargo fmt --check`, clippy with
  `-D warnings` and `node scripts/generate-ipc-samples.mjs --check` pass.

### P4 detail: `brain_delete`

- The argument is `{ conversationId, memoryId }`, both UUIDs, and the object is strict (`brainMemoryDeleteSchema`): an extra
  field is `invalid-input`.
- Order, as in the TypeScript handler: the conversation must exist (`unknown-conversation`), its project must be saved,
  authorized, available and not blocked (`unknown-workspace`), and then the memory is deleted only where it belongs to that
  project. A memory of another project is not found, and it is not touched.
- The delete is the repository's `DELETE FROM brain_memories WHERE id AND workspace_id`. The history of the memory goes with it
  by its foreign key, as in TypeScript.
- Known deviation: a memory that is not there is `unknown-memory`, with the message `Memória não encontrada.`. TypeScript says
  `Memória não encontrada ou já removida.`. The code is the same, and the interface does not show the message yet.
- Not ported: the `logger.info` line of the TypeScript handler, as for the other commands.
- Checks: `crates/noct-core/tests/ipc_brain_delete.rs` (the two ids, the strict object, the missing keys),
  `crates/noct-app/tests/services.rs` (one test: a delete once, another project's memory kept, the second delete and the
  missing conversation), `crates/noct-app/tests/commands_acl.rs` (one test), `crates/noct-db/tests/store.rs` (a refused delete
  leaves the memory in place).
- A mutation check: with the project filter removed from the repository's delete, the service test fails. The file was
  restored and checked.
- Tests after this change: 574 passed, 0 failed, 19 ignored in the workspace; `cargo fmt --check`, clippy with
  `-D warnings` and `node scripts/generate-ipc-samples.mjs --check` pass.

### P4 detail: `workspace_remove`

- The argument is the project path as a bare string. An empty string or any other shape is `invalid-input`, as
  `z.string().min(1)` requires (no trim: a blank path is not empty, as in TypeScript).
- The delete is the repository's `DELETE FROM workspaces WHERE path = ?`. Removing a path that is not saved is not an error,
  as in TypeScript.
- What the store removes with the project, by the foreign keys that reference `workspaces(path)` with `ON DELETE CASCADE`:
  the model bindings, the index runs, files, stack evidence and exclusions, the validation runs, the executions and their
  checkpoints, the semantic index runs, the engineering signals, snapshots and insights, the interrupted operations, the
  workspace evidence, and the brain memories of the project.
- What it does not remove: the conversations, their messages and their artifacts. `conversations.workspace` has no foreign
  key, so the rows stay after the project record is gone. TypeScript has the same schema and the same behavior. This is the
  case to decide before the interface offers the command: a removed project leaves its conversation history in the store.
- Not checked, as in TypeScript: that the project is saved or authorized, and that it is not blocked. The command is a
  destructive write without the authorization check that the other project commands make. Recorded as an open decision for the
  owner; the Rust port does not add the check, so that it does not change the behavior silently.
- Not verified: a project that has change sets. The checkpoints of a change set are referenced with `ON DELETE RESTRICT`, so the
  removal may fail for such a project, in Rust and in TypeScript alike. No test covers it yet.
- Checks: `crates/noct-app/tests/services.rs` (two tests: the cascade for the path's own rows, with the conversation and its
  messages kept and the other project's bindings intact; the empty path refused and the unsaved path accepted),
  `crates/noct-app/tests/commands_acl.rs` (one test: the grant, the argument shapes, and the listing after the removal),
  `crates/noct-db/tests/store.rs` (a refused removal leaves the project saved).
- Tests after this change: 570 passed, 0 failed, 19 ignored in the workspace; `cargo fmt --check`, clippy with
  `-D warnings` and `node scripts/generate-ipc-samples.mjs --check` pass.

### P4 blocked: `changeControl.editHunk`

- The TypeScript handler checks the hunk, its change and its execution against the authorized conversation, then calls
  `ChangeHunkService.edit`. That method does not write a file of the project. It stores the edited patch in the database, and
  before that it checks the patch: the stored checkpoint of the file is read, its SHA-256 must match the change's
  `beforeHash`, the patch is applied in memory with `applyPatch`, and the hunk becomes `edited` or `conflicted`.
- Three parts are missing in Rust, and the check cannot be left out, because the status of the hunk depends on it:
  1. a reader of the checkpoint content (the stored bytes of a file before the execution), which `checkpoints.readContent`
     does in TypeScript; the Rust checkpoint repository has the records but not the bytes;
  2. the change diff, `ChangeDiffService.get`, which the check reads for the patch's kind and unified text;
  3. `applyPatch`, the line-based patch application (about forty lines of TypeScript, with the line-ending normalization).
- Until those exist, the command is not registered. Porting the database update alone would store a patch that is never
  checked. Recorded as blocked in the inventory (`P4 (blocked)`).

### P5 detail (first cut)

- `ShellObserver` (in `crates/noct-app/src/logging.rs`) applies the database's warning policy to each measured operation:
  a failed one, or one that takes at least 50 ms, is written as a warning through the diagnostic gate. The policy function,
  `warn_on_attention`, is public in `noct_db::observer`.
- `measured` times a command and reports it once. A panic counts as a failure.
- Measured under the TypeScript names from `noct_db::observer::CATALOG`: `settings.get`, `settings.set`, `workspaces.list`,
  `conversations.page`.
- `workspace_set_favorite` is not measured: its TypeScript path has no name in the catalog, and none was invented.
- A warning names the operation and its duration. It has no path, no value and no error text.
- Checks: `crates/noct-app/tests/observation.rs` (six tests).

### P6 detail (round 1)

- The worker's comment in `crates/noct-native/src/main.rs` said the `UNSUPPORTED` answer on macOS and Windows was the C++
  `#else` behaviour. It is not: the C++ worker has backends there (`__APPLE__` and `_WIN32`). The comment now says the port
  is missing. A comment change only.

### Stops (round 1) and their outcome in round 2

- **P8, updater:** needed `tauri-plugin-updater`, not on the approved list. Round 2: approved (see below).
- **P9, release dry run:** the attestation step needed a third-party Action, not approved; a dry run also changed the release
  workflows. Round 2: one Action approved, in a new workflow only (see below).
- **P6, native backends:** not started beyond the comment. Round 2 has the next priority.
- **P7, UI slices:** not started. Round 2 includes it.

### Decisions taken in round 1

- I worked in the worktree on `migration/rust` (`noct-rust-spike`). `main` and the other worktrees were not touched.
  `spike/rust-rollback-worker` is untouched.
- Vitest excludes the `node:test` launcher files, which CI already runs with `node --test`. Test tooling only.
- The reverse gate runs from `verify-row-gate.mjs`, so `row-gate.yml` runs it with no workflow change.
- macOS durability kept `fsync` for parity with the C++ worker. Round 2 overrides this with the owner's decision.
- The IPC inventory classifies every channel that touches a process, the network, a secret, the updater or the vault as
  `stop` or `later`. It does not port them.
- `clipboard.readText` is classified as a read of the OS clipboard and left for later, because it sends clipboard contents
  to the renderer.
- P3 used an offset for the project page. Round 2 changes it to a key cursor (see below).
- `workspaces_list` now takes an input. Its only caller is the sidebar in this repository.
- The page type for the TypeScript check is written in `scripts/generate-ipc-samples.mjs`, because the shared types have
  the list only. No product TypeScript changed.
- P4 adds one error code, `unknown-workspace`, to the shell's own codes.
- The favorite write is not measured in P5: its TypeScript path has no name in the catalog.
- P5 makes `warn_on_attention` public in `noct_db::observer`, so the shell applies the database's policy and does not copy it.
- P6 changed a comment only in round 1.

### New dependencies (round 1)

None.

### Not verified (round 1)

- A real window on Windows or macOS. CI does not open windows.
- Ubuntu 22.04, the Linux floor. No CI job runs on it (`ubuntu-latest` and `ubuntu-24.04`). The WebKitGTK version is unchecked.
- Power-loss durability on macOS and Windows.
- The Rust rollback backends for macOS and Windows (round 1: not ported).
- The project list beyond the first 500 projects (round 1).
- Offset paging over a list whose order has ties. Round 2 replaces it.

## Round 2: decisions received and queue

Decisions from the owner, recorded as given:

1. **`tauri-plugin-updater`: approved.** Official plugin (`tauri-apps`), version pinned, `cargo deny` and `cargo audit`
   passing, and the public key left as a value to fill in. No keys are generated, no secrets are created, nothing is
   published. The owner keeps custody of the keys.
2. **Build provenance attestation: only `actions/attest-build-provenance`, pinned by SHA, in a NEW workflow in dry run,
   with no secrets.** The Electron release workflows are not changed. No other third-party Action is approved.
3. **macOS durability: `F_FULLFSYNC`, definitive.** **Windows: `windows-sys` in `crates/noct-native`, as approved.**
4. **P7 is in the queue.** The rest of P4 follows the order of `docs/ipc-inventory.md`. Operations that delete come last,
   each in its own commit with `[review]` in the subject.
5. **`workspaces_list`: the offset becomes a key cursor.** `id` is added as the final tie-break of the order. It only
   affects ties. Parity with the TypeScript order must be kept and proven in the differential. The change is recorded.
6. **Explain the `strip` warning of commit `4cb423e`** (what it is, where it appears, whether it affects the release
   binary), without changing anything yet.

Priority of the queue: (a) P6, native rollback for macOS and Windows, fail-closed, tested on the CI matrix, with
`docs/rust-native-rollback-backends.md` updated; (b) the rest of P4; (c) P7 in functional slices with real data from the
commands already ported; (d) P8 and P9, preparation only, per decisions 1 and 2.

Rules unchanged: no cryptography, vault, updater key, secrets, or network calls to providers without `[security-review]`;
no TypeScript product change; nothing on `main`; no PR; no force push; fast-forward pushes only after local validation.

### Round 2 progress

#### Cursor for `workspaces_list` (decision 5), commit `724d7db`

- The offset is replaced by a key cursor. The request is `{ limit, cursor? }` with `cursor` as
  `{ favorite, lastOpenedAt, path }`. The reply is `{ items, hasMore, nextCursor }`, and `nextCursor` is present exactly
  when `hasMore` is true. The order is `favorite DESC, last_opened_at DESC, path ASC`, unchanged for the first page.
- **Deviation from decision 5, to confirm.** The decision asked for `id` as the final tie-break. The `workspaces` table has
  no `id` column: its key is `path TEXT PRIMARY KEY`. The tie-break is therefore `path`, which is unique and is the table's
  own key. If an `id` column is wanted, it needs a schema migration, which this change does not make.
- The TypeScript list is untouched. The differential walks the list by cursor, with limits 1, 2 and the maximum, on the nine
  release lines and the current tree. Ties are built on purpose (two projects with the same `favorite` and
  `last_opened_at`). Every walk must equal the whole list, and `nextCursor` must be present exactly when `hasMore` is set.
- Tests after this change: 460 passed, 0 failed, 19 ignored in the workspace (before the native backends).

#### P6: native rollback for macOS and Windows (decision 3, priority a)

- Commits: `352ba2e` (split into `operation.rs` and `platform/`), `36d515f` (C++ refusal messages on failed lookups),
  `e008be1` (macOS backend), `a0c926e` (Windows backend, `BIND`, `windows-sys`), `7b4826a` (binary-level tests),
  `5af3278` (backends document: CI result).
- The Linux backend keeps its behaviour. Local differential against the C++ worker, built with `g++`: 35 of 36 scenarios give
  the same responses and the same final tree. The one difference is intended: `OPEN parent/` is refused by Rust and answered
  as an absent file by C++.
- macOS: APFS only; `F_FULLFSYNC` (decision 3); protected names compared without ASCII case, as in C++ on Apple.
- Windows: fixed NTFS only; `NtCreateFile` relative to retained handles; reparse points refused; `BIND` for the holder process;
  publication renames the staged handle. Dependency: `windows-sys` 0.61.2, pinned, in `noct-native` only, with the Win32
  features it needs (decision 3). The NT structures are declared in the backend, so the `Wdk` feature set stays off.
- CI on `7b4826a` (run `44124578455`, `rust-shell-validation.yml`): `Shell on ubuntu-latest`, `Shell on windows-latest` and
  `Shell on macos-latest` succeeded, and so did the workspace test step on each. The native tests ran on each system:
  Windows, `rollback_protocol` 12 passed and `rollback_binary` 4 passed; macOS, `rollback_protocol` 13 passed (with the APFS
  case test) and `rollback_binary` 4 passed. `Row gate`, `Keyring round trip` and `Rust dependency audit` succeeded on the
  same commit.
- Local validation on the final tree: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test
  --workspace` (472 passed, 0 failed, 19 ignored), clippy for `x86_64-pc-windows-msvc` and `aarch64-apple-darwin` for
  `noct-native`. `cargo deny` (advisories, bans, licenses, sources) passed; `cargo audit` exited 0 with the seven allowed
  transitive warnings that were already in the tree.
- **Not verified:** the differential against the C++ worker on macOS and Windows (the C++ worker was not built there); a real
  user's APFS or NTFS volume; the cost of `F_FULLFSYNC` (not measured, so no performance claim is made).
- **Open decisions for the owner:** (1) whether the C++ product worker also uses `F_FULLFSYNC` (it uses `fsync` today; the
  product path is not changed by this branch); (2) whether a directory flush is added on Windows after a rename or create
  (the C++ header does not flush, and the port keeps that).
- **Stops:** none in P6. **New dependencies:** `windows-sys` 0.61.2 (approved, decision 3); no other crate was added.
