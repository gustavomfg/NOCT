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
| P7: UI slices | `1103543` (`Construir` and `Documentar` not yet available), editable Ajustes (`c6318cd`) | `c6318cd` | Shell on `c6318cd` (run `44195972611`): success on Ubuntu, Windows and macOS. Row gate (`44195972598`): success. | Real window. Chromium is the approximation the check uses. |
| P8: updater | `004dba3`: `tauri-plugin-updater` 2.14.1 pinned, configured empty, registered only when complete and safe | `004dba3` | Shell on `004dba3` (run `44200957653`): success on Ubuntu, Windows and macOS. Row gate (`44200957805`), Rust dependency audit (`44200957489`) and Keyring round trip (`44200957630`): success. | An update request, a signature check and an install: none is made, since there is no key and no endpoint. The public updater guide does not define `requireSignedVersion`; the plugin source does (see `docs/rust-shell.md`). |
| P9: release dry run | `bb3f357`: `build-provenance-dry-run.yml`, one action pinned by SHA, a subject file from the run | `bb3f357` | `bb3f357` (run `44205153413`): dry run success. Shell (run `44205153278`): success on Ubuntu, Windows and macOS. Row gate (`44205153290`): success. The attestation was verified with `gh attestation verify` from this session (exit 0). | No release or Electron artifact is attested. The verification ran from this session's machine, not in CI. |
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

### P9 detail: build provenance dry run

- New workflow `.github/workflows/build-provenance-dry-run.yml`. It writes `dry-run/subject.txt` (the commit and the run id),
  prints its digest, and attests it with `actions/attest-build-provenance` pinned to `4d101475d8b20a2381f78447822ac1eab6504dd8`
  (`v4.2.2`, the latest stable release, which is the commit its tag points to).
- No repository secret, and no `actions/checkout`: the subject is made by the job, so the action is the only one used. The job
  permissions are `contents: read`, `id-token: write` and `attestations: write`, the minimum the attestation needs.
- Triggers: a push to `migration/rust` that changes the workflow file, and `workflow_dispatch`. Not `main`, not pull requests.
- The Electron release workflows are not changed.
- Checks: the YAML parses; the triggers, the permissions and the single `uses` line were read back with a parser; the pinned
  reference is a 40-character commit that the API returned.
- Run `44205153413` succeeded on `bb3f357`. The attestation was verified from this session with `gh attestation verify` on the
  same subject content: exit 0; subject `subject.txt`, digest `d023436fa74336bbee6bf8a9f88aac26a5fd9e2799b7fdebb8ddacd725a10a63`;
  signer workflow `build-provenance-dry-run.yml` on `refs/heads/migration/rust`; commit `bb3f35765f75993bd5bb16714e2c656448e3c83f`.
  The check ran here, not in CI. The repository is public, so the attestation is in the public transparency log.
- Decision for the owner: the attestation records the commit and the run in the public transparency log. This is what the
  action does, and the subject holds only public values. Confirm this is acceptable for the dry run.

### P8 detail: updater (prepared, not configured)

- Dependency: `tauri-plugin-updater` `=2.14.1` in `Cargo.toml` (`[workspace.dependencies]`), used by `noct-app` only. It is the
  latest stable 2.x; the 3.0.0 line is an alpha and was not used. It requires `tauri ^2.12`, which the workspace has at 2.12.5.
- Lock file: 576 packages before, 623 after (47 added). The new entries are the plugin and its TLS stack: `tauri-plugin` 2.7.2,
  `minisign-verify` 0.3.0, `ring` 0.17.14, `rustls` 0.23.46, `rustls-webpki` 0.103.17, `hyper-rustls` 0.27.10, `untrusted` 0.9.0,
  `webpki-root-certs` 1.0.12, `zip` 9.7.0, `tar` 0.4.46 and `windows-sys` 0.52.0, which `ring` brings in (the lock had 0.45.0 and
  0.61.2 before). `reqwest` 0.13.5 was already present. No existing package changed version.
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
  checked. Recorded as blocked in the inventory (`beta 2 (blocked)`).
- **Decision of the owner (round after beta 1 planning):** not ported now. It enters beta 2 together with capture, the change
  diff, the checkpoint reader, the hunks and the accept and reject decisions. The answers to questions (a) to (f) are in
  `docs/rust-edit-hunk-design.md`, section 13. No code is written for it in this round, and nothing in it is verified.

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

## Round 2 report: close of the review

This section closes the second review round. It records what is validated and what is not, the decisions of this review, and
what the next session starts with. CI results are read from GitHub runs.

### CI per system, finished runs

| Commit | Work | Ubuntu | Windows | macOS | Row gate | Other workflows |
| --- | --- | --- | --- | --- | --- | --- |
| `c6318cd` | P7, editable Ajustes | success (`Rust shell validation`, run `44195972611`) | success | success | success (`44195972598`) | none |
| `004dba3` | P8, updater prepared | success (run `44200957653`) | success | success | success (`44200957805`) | Rust dependency audit `44200957489` success; Keyring round trip `44200957630` success |
| `bb3f357` | P9, dry run | success (run `44205153278`) | success | success | success (`44205153290`) | Build provenance dry run `44205153413` success |

The documentation commits pushed after `bb3f357` start the Rust shell validation again. Their results are not in this table,
because this section was written before those runs finished.

### Commits of this review

- P4, eight commits from `e383ec2` to `8126f82` (see the P4 detail sections above).
- P7: `1103543`, `c6318cd`. P8: `004dba3`. P9: `bb3f357`.
- Documentation: `1e602cb` (editHunk design), `4a3fd13` (the `requireSignedVersion` citation), `02a165d` (the seven advisories),
  and the commit that adds this section.

### Status by package

- **P4, database writes:** done except `changeControl.editHunk`. Blocked: no code. Its design is in
  `docs/rust-edit-hunk-design.md`, with seven open questions.
- **P7, UI slices:** two slices (`1103543`, `c6318cd`). Not done: the `workspace_remove` confirmation (decision 1) and the
  conversations list.
- **P8, updater:** prepared and not configured (`004dba3`). No update request, no feed and no key exist.
- **P9, release dry run:** done (`bb3f357`). The attestation was verified from this session (see its row above).

### Owner decisions of this review

1. **`workspace_remove` keeps TypeScript parity.** The service does not check whether the project is saved or authorized. The UI
   must confirm before removing a project, and must say what goes and what stays. What goes: model bindings, the index,
   executions, checkpoints, brain memories and evidence. What stays: conversations, messages and artifacts. The UI is not
   written. Its text must not promise removal for a project that has change sets until decision 2 is settled.
2. **Project removal with change sets: first task of the next session.** `change_sets.before_checkpoint_id` and
   `after_checkpoint_id` reference `checkpoints` with `ON DELETE RESTRICT` (`crates/noct-db/src/migrations/generated.rs`, lines 523
   and 524). Removing a project whose checkpoints a change set references may fail. This is untested, and the TypeScript behaviour
   is not compared. The task: a test with such a project, the same data through the TypeScript removal
   (`electron/ipc/registerWorkspaceIpc.ts`, `electron/database/Database.ts`), and a decision on the outcome.
3. **`changeControl.editHunk`: design first, no code.** `docs/rust-edit-hunk-design.md` covers the checkpoint reader, the diff,
   `applyPatch`, the path scope, undo, and what the command writes: only the `change_hunks` row, never a file in the workspace.
   Its section 13 lists the open questions (a) to (g).
4. **Updater citation and advisories.** `requireSignedVersion` is defined in the plugin source (`tauri-plugin-updater` 2.14.1,
   `src/config.rs`). The public updater guide (https://v2.tauri.app/plugin/updater/) does not mention it. The citation and its
   consequence are in `docs/rust-shell.md`. The seven `cargo audit` advisories, with their kind, reason and path into the
   workspace, are in `docs/rust-dependency-advisories.md`.

Round-2 decision 6, the `strip` warning of commit `4cb423e`, has no answer recorded. It was not done in this review, and it stays
open. Round-1 open item (f), `workspace.remove` without an authorization check, is resolved by decision 1.

### Validation of this review

- `004dba3` (P8), run locally before the push: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo test --workspace --no-fail-fast` (592 passed, 0 failed, 19 ignored); `node scripts/generate-ipc-samples.mjs --check`;
  `cargo deny check` (advisories, bans, licences and sources ok); `cargo audit` exit 0 with the seven advisories listed in
  `docs/rust-dependency-advisories.md`. Two mutation checks on the updater: removing `allowDowngrades` from the flag list, and
  accepting http endpoints. Each made its test fail, and the file was restored after each run.
- `bb3f357` (P9): the YAML was parsed; the triggers, the permissions and the single `uses` line were read back with a parser;
  the action is pinned to `4d101475d8b20a2381f78447822ac1eab6504dd8` (`v4.2.2`). The dry run succeeded in CI, and the attestation
  was verified as recorded in the P9 row.
- Documentation commits: `git diff --check`. No code changed, so no build or test ran for them.

### Dependencies added in this review

- `tauri-plugin-updater` 2.14.1, pinned in the workspace and used by `noct-app` only. The lock file went from 576 to 623 packages.
  The new entries are listed in the P8 detail. Certificate data, `webpki-root-certs` 1.0.12, is licensed `CDLA-Permissive-2.0`.
  No existing package changed version.
- Licences allowed in `deny.toml`: `ISC` and `CDLA-Permissive-2.0`, with the reason in the file. The owner should confirm this.
- GitHub Action: `actions/attest-build-provenance` v4.2.2, pinned to `4d101475d8b20a2381f78447822ac1eab6504dd8`. No other
  action was added.
- Earlier in round 2: `windows-sys` 0.61.2 in `noct-native` (owner-approved) and `time` in `noct-app`.

### Stops

- `changeControl.editHunk`: blocked. Design only, no code.
- `workspace_remove` UI: not started. The requirement is decision 1.
- Project removal with change sets: not tested. It is the first task of the next session.
- No package was started after P9. P9 was already pushed when the owner's instruction to stop arrived, and it is complete.

### Not verified

- A real window on any system: CI opens none. The native `<select>` look in the webviews. The Ubuntu 22.04 floor. Power-loss
  durability. The C++ differential for rollback on macOS and Windows.
- Updater: no update request, no signature check and no install. The public key and the endpoints are empty, so none of them runs.
  `requireSignedVersion` is set to `true` but has never been exercised against a feed.
- Attestation: verified from this session with `gh attestation verify`, not inside CI. No release or Electron artifact is attested.
- Project removal with change sets: untested, and not compared with TypeScript.
- `editHunk`: no code, so nothing is verified for it.
- Round-2 decision 6 (the `strip` warning): not explained.
- CI of the documentation commits: not recorded here.

### Dependencies on the owner

1. Updater: create and keep the signing keys. Fill `pubkey` and `endpoints` in a reviewed change marked `[security-review]`. No
   key was created here.
2. Confirm `requireSignedVersion: true`, and its consequence for releases signed before the version was recorded (none exist yet).
3. Confirm the two licences allowed in `deny.toml`.
4. Decide whether `cargo audit` should fail on warnings. Today it does not.
5. Confirm that the dry-run attestation is acceptable in the public transparency log. The repository is public, and the subject
   holds only the commit SHA and the run id.
6. Round-1 open decisions still standing: (a) whether the C++ product worker uses `F_FULLFSYNC` on macOS; (b) a directory flush on
   Windows after a rename or create; (c) the `workspaces_list` tie-break, `path` as built or `id` with a migration; (d) the
   `.nocturne` workspace context port; (e) UTC or local time for the `ai_save_assistant` title.
7. Round-2 decision 6: the `strip` warning of commit `4cb423e`.
8. The `editHunk` questions (a) to (g) in `docs/rust-edit-hunk-design.md`, section 13.

### Next session, first task

Project removal with change sets (decision 2). Then the `workspace_remove` confirmation in the UI, once the outcome of that removal
is known.

## Changes after the round 2 report

The round 2 report describes the updater as prepared, with an empty key and no endpoint. These commits fill it in, under
`[security-review]`:

- `9426e68`: `plugins.updater` gets the release public key (key id `40503B624D87D646`) and the one HTTPS endpoint of the
  latest release. The configuration is complete, so `crates/noct-app/src/updater.rs` registers `tauri-plugin-updater`
  2.14.1 at start. Registering it makes no request. No code calls the plugin's check, download or install commands, and
  the main window has no updater permission.
- `47eb3f7`: the release workflow, `.github/workflows/shell-release.yml`, is triggered by `rust-v*` tags, which no Electron
  workflow matches. Its gate checks the shell CI of the tagged commit, not `main`. The attestation action is at the pin the
  build provenance dry run verified. Nothing has run it.
- `6fd041e`: `docs/noct-updater.key.pub`, the public key file, is committed. `updater_config.rs` fails if it differs from
  the configuration.

Not verified after these commits: no window was opened, so the plugin's registration has not been seen in a running
shell; the release workflow, its `release` environment and its secrets have not run; the endpoint
`releases/latest/download/latest.json` shares the "latest" release with the Electron application, which is a blocker
before the first tag (see [docs/shell-updater.md](shell-updater.md), Release workflow).

## Beta 1 OpenRouter: report of this round

> **Superseded.** This report describes the state before the chat was connected. The HTTP client, the SSE adapter, the
> chat and the model field now exist in the code, with the switch off (`PROVIDER_CHAT_ENABLED = false`). The current state is
> in "Item 2" of the practical-mode report below. The text that follows is kept as the record of that round.

Beta 1 is the OpenRouter provider: the key in the credential vault, the network policy, the context rules, the per-project
consent and the settings section. Beta 1 sends nothing to a provider. The chat that would send a conversation is not
connected, and no switch exists for it yet. The SSE adapter and the pinned HTTP client are blocked (Stop 1), so this round
does not complete the provider path. `changeControl.editHunk` was recorded as beta 2 earlier, in `2fd8bb2`, and is not part of
beta 1.

### Commits of this round

| Commit | Work |
| --- | --- |
| `8919c48` | 3A: provider key contract with a write-only input (redacted `Debug`, 8 to 512 visible ASCII characters) |
| `ef26329` | 3A: exact-key read of a hidden setting, used for the key reference |
| `2cd2350` | 3A: `provider_key_status`, `provider_key_save`, `provider_key_remove`, on the credential vault |
| `5105ceb` | 3B: network policy as a pure module, with the design answers in section 13 of `docs/provider-allowlist-design.md` |
| `c68771e` | 3D: context rules ported from the TypeScript policy, with the size budget and the sensitive-file exclusion |
| `f5adb0f` | 3D: per-project consent contract, version `openrouter-v1` |
| `c1056fc` | 3D: `provider_consent_status`, `provider_consent_grant`, `provider_consent_revoke` |
| `0df77cb` | 3E: settings section for the key and the consent, with browser checks and per-command mocks |
| `9e64a73` | CI fix: both READMEs name the release `v1.0.5`, which `verify:release-metadata` requires |
| `0a34a65` | 3F: the IPC inventory lists the Rust-only provider commands |
| the commit that adds this section | 3F: this report, and the design answers updated to match the code |

### Status by package

- **3A, provider key in the vault:** done. Status, save and removal. The key is never returned to the page, and the page holds
  it only until the save call is sent. The vault design is unchanged (ADR 0002).
- **3B, network policy:** done as a pure policy: URL checks, approvals per host and port, address verification through an
  injected resolver, refusal of redirects, and a status check. Not done: the pinned HTTP client, the connection itself, and
  cancellation. The timeouts and the size limit are constants in `crates/noct-core/src/limits.rs`, and no code applies them yet.
- **3C, OpenAI-compatible SSE adapter and streaming:** not started. Blocked by Stop 1.
- **3D, context and consent:** done. The context rules are ported from `SemanticPrivacyPolicy.ts` and `WorkspacePathPolicy.ts`.
  Consent is per project and per provider, and it is asked before any send. No send exists yet for it to gate.
- **3E, interface:** the key section and the consent section are done, with four browser scenarios. Not done: the model
  selector (Stop 3), the streaming review panel (blocked by 3C), and saving a real reply with `ai_save_assistant` (there is no
  real reply until 3C; the command itself exists since P4).
- **Real chat behind a switch that is off by default:** not started. There is no adapter to switch on, so there is nothing to
  switch on yet. The switch is created with the adapter, off by default, and this section is updated when it exists.
- **3F, documentation:** the IPC inventory, this report, and the design answers are updated. The READMEs state the beta 1 scope.

### Decisions taken by me

These are the choices not covered by the mission, each taken in the most conservative way.

- **D1.** The key reference is stored at `maintenance.secret.openrouter.reference`. Both settings readers hide the `maintenance.`
  prefix (`electron/database/SettingsRepository.ts` and `Settings::get`), so the reference never reaches the settings the page
  reads. The name `providers.openrouter.reference` was rejected because the TypeScript reader would list it.
- **D2.** `noct-app` depends on `noct-secrets` by path. No external crate was added.
- **D3.** `noct-secrets` re-exports `SecretString` and `ExposeSecret`, so `noct-app` needs no `secrecy` dependency of its own.
- **D4.** A status read with no saved reference does not open the vault, so it never asks the operating system for the master key.
- **D5.** Save writes the secret before the reference. Remove deletes the secret before it clears the reference. If clearing the
  reference fails, the reference stays, so a retry finishes the removal.
- **D6.** The key is 8 to 512 characters of visible ASCII after trimming. An error names the field, never the value.
- **D7.** The ACL tests do not send a valid save, so they never touch the keyring of the machine that runs them. The service
  tests use an in-memory vault.
- **D8.** `provider_key_status`, `provider_key_save` and `provider_key_remove` are granted to the `main` window only, and the
  capability lock was updated.
- **D9.** Known gap, left open: no prune of orphan secrets after a crash between creating a secret and writing its reference.
  `vault.prune` exists but is not called yet.
- **Network answers Q1 to Q6** are in section 13 of `docs/provider-allowlist-design.md`. The short form: a local source reaches
  loopback only, which closes G1 in the Rust port; approvals are per host and port; no approval table and no migration; the
  built-in list holds OpenRouter alone; there are no other endpoints; redirects are refused.
- **Consent.** Stored per project and per provider under `maintenance.consent.openrouter.<path>`, with the version `openrouter-v1`.
  Granting and revoking require a saved project.
- **Context.** Rules ported from the TypeScript policy, with one deliberate divergence: a file named exactly with a sensitive
  extension, such as `.pem`, is sensitive in the Rust port. The TypeScript rule reads the extension with `path.posix.extname`, which is empty for `.pem`, so it does not match that name. Limits: 128 KiB in
  total and 32 KiB per file (`provider_context` in `limits.rs`). Once a file does not fit the budget, the rest are excluded as
  over budget.
- **Model ids.** The repository holds no list of `:free` model ids, so none is hardcoded. The selector is not built (Stop 3), and
  the `:free` restriction is not enforced anywhere yet.

### New dependencies

- No external crate was added. `Cargo.lock` gained two internal edges, `noct-app` to `noct-secrets` and `noct-ui` to `serde`.
  The diff from `2fd8bb2` adds no `[[package]]` block.
- `cargo deny` and `cargo audit` were not run for this round, because no external package changed.
- Not added: `reqwest` (Stop 1).

### Stops

1. **Pinned HTTP client (3B) and SSE adapter (3C).** `reqwest` with the approved features brings `aws-lc-rs` 1.18.2 and
   `aws-lc-sys` 0.46.0. Those build native code with cmake, and they are not in the lock. Not added. The options for the owner:
   accept the native build and its CI cost, or use `rustls-no-provider` with `ring`, which is already in the lock but is outside
   the approved feature set. The second option was not taken.
2. **Trigger reduction for Windows and macOS (Package 1).** It needs a change to the release gate in
   `.github/workflows/shell-release.yml`, which is outside the single-file scope. Not changed.
3. **Model selector.** `models_set_bindings` replaces the whole bindings row. No shell command reads the bindings back, so a save
   of the default model would erase an embedding binding that the page cannot see. It needs a read command first, or an explicit
   decision to accept the replacement. Not built.
4. **Streaming review and saving a real reply.** Both depend on 3C.
5. **Intermittent Windows persistence failure from an earlier package.** The cause is not established. Not investigated in this round.

### CI per system (finished runs only)

The branch runs the workflows below for each push. Only runs that finished are recorded.

| Commit | Ubuntu, Windows, macOS (`Rust shell validation`) | `CI · source, renderer and packages` | `Row gate` | `Keyring round trip` | `Rust dependency audit` |
| --- | --- | --- | --- | --- | --- |
| `2cd2350` | cancelled on all three (run `44256845617`) | failure: README version (run `44256857218`) | success (`44256857182`) | success (`44256856945`) | success (`44256857022`) |
| `c1056fc` | cancelled on all three (run `44257869801`) | failure: README version (run `44257879030`) | success (`44257879034`) | success (`44257878924`) | success (`44257878915`) |
| `0df77cb` | cancelled on all three (run `44258880725`) | cancelled (run `44258888191`) | cancelled (`44258888293`) | success (`44258880704`, `44258888160`) | in progress at the time of writing |

Reading the table:

- The `Rust shell validation` runs were cancelled on all three systems. Their start times match the next push to the branch,
  so they were superseded by that push. This is an inference from the timestamps, not a platform failure. No Windows or macOS
  result exists for any commit of this round.
- The `CI · source, renderer and packages` failure on `2cd2350` and `c1056fc` was `verify:release-metadata`: the README said
  "version 1.0.5", and the check requires `v1.0.5`. The same failure occurred on `2fd8bb2` (run `44255047842`). `9e64a73` fixes
  it. Its run was in progress at the time of writing and is not recorded here.
- The `0df77cb` source and row-gate runs were cancelled after about one minute. Their timestamps match the pushes that followed,
  so they were superseded, the same inference as above.
- The runs of `9e64a73` and `0a34a65` were in progress at the time of writing. They are not recorded here, and nothing in this
  section claims them green.

### CI times before and after (Package 1)

There is no before and after pair. No `Rust shell validation` run of this round finished. The superseded runs lasted 8 to 14
minutes before they were cancelled, which is not a run time and is not comparable. Trigger reduction is blocked (Stop 2).

### Validation of this round

Run on the tree of `0df77cb`, which the later commits do not change in code.

- `cargo fmt --all -- --check`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass.
- `cargo clippy -p noct-ui --target wasm32-unknown-unknown -- -D warnings`: pass.
- `cargo test --workspace --no-fail-fast`: 672 passed, 0 failed.
- `trunk build --release` in `crates/noct-ui`: pass.
- `node scripts/check-shell-ui.mjs`: 21 of 21 scenarios pass, including the four provider scenarios.
- `node scripts/check-shell-csp.mjs`: pass.
- `node --test scripts/run-rust-shell.test.mjs`: 7 of 7 pass.
- `scripts/verify-rust-written-gate.mjs` and `scripts/verify-row-gate.mjs`: both pass.
- `node scripts/verify-release-metadata.mjs`: pass, after `9e64a73`.
- `node scripts/ipc-inventory.mjs` produces `docs/ipc-inventory.md` byte for byte, after `0a34a65`.
- `npm test` (TypeScript suite, run on this round's tree): 98 test files passed, 634 tests passed, 12 skipped, exit 0.
  No product TypeScript file was changed in this round.
- actionlint: not run. No workflow file changed in this round.

### Not verified

- No window was opened on any system. The browser checks run Chromium with a mocked `invoke`, not the Tauri webview.
- No request was sent to a provider, and no network path is used. The address policy was tested with a fake resolver only.
- The operating system keyring: the `Keyring round trip` workflow succeeded on `2cd2350` and `0df77cb`. The vault tests of this
  round use an in-memory fake.
- Orphan secrets after a crash (D9) are not pruned.
- The timeouts and the size limit are constants that no client applies yet.
- The CI results of `9e64a73` and `0a34a65` are not recorded here.
- The commit message of `0a34a65` says "five Rust provider commands". There are six. The message was pushed as it is, and it
  is not rewritten, because the branch is not force-pushed.

### Dependencies on the owner

1. Choose the native build path for the HTTP client (Stop 1). Until it is chosen, 3B and 3C stay blocked.
2. Decide the model selector (Stop 3): add a read command for the bindings first, or accept that a save replaces the row. Also
   decide whether `:free` is checked in the page only or in the core as well.
3. Review decisions D1 to D9, the network answers in `docs/provider-allowlist-design.md` section 13, and the context divergence.
4. Decide Stop 2, the trigger reduction for Windows and macOS.
5. Investigate the intermittent Windows persistence failure on a Windows machine (Stop 5).

### Next session, first task

Decide Stop 1. Then the pinned connection (3B) and the SSE adapter with streaming (3C) can start, and the real chat switch, off by
default, is added with the adapter.

## Security review of the shell: report of the round

The round covers the `[security-review]` commits after `67c2aa4`. The provider chat ships switched off
(`PROVIDER_CHAT_ENABLED = false`). No provider network call is made by the product.

### Commits of the round

| Commit | Subject |
| --- | --- |
| `35cb93d` | docs(security): review the beta 1 commits and their tests |
| `89b81bc` | build(deps): reqwest and rustls on ring for the provider client |
| `bd94555` | fix(scripts): drop the unused result parameter of the provider check |
| `f1a386e` | feat(core): strict SSE reader and OpenRouter chat protocol |
| `5907f25` | feat(app): provider transport, pinned and capped |
| `fa86fef` | fix(app): consent needs an authorized project |
| `628e799` | fix(app): one provider key operation at a time |
| `3c74b97` | fix(app): remove leftover provider secrets |
| `bf7deaa` | feat(core): default OpenRouter model binding and its check |
| `44c4f1b` | feat(db): set the default model without losing the other bindings |
| `6a3aba1` | feat(app): read and set the default model of a project |
| `994e9e9` | feat(shell): default model field in the provider settings |
| `8a90b82` | test(app): share the loopback provider between the transport and chat tests |
| `384fc2b` | feat(app): provider chat backend, read-only and switched off |
| `7a802f1` | feat(shell): chat section in Revisar, off by default |
| `32f18d6` | docs(provider): document the switched-off chat and close the review status |
| `2ef1ec5` | fix(app): keep the transport test off a freed port |
| `0b4c529` | ci(release): wait for the tag's shell CI and require all three systems |
| `b49da1f` | ci(rust): run Windows and macOS on tags, pull requests and the nightly schedule |
| `d33dfcf` | ci(rust): add the opt-in Windows persistence check |
| `10d6cf2` | fix(release): pick the gate's shell run by commit, not by branch |

Commit `0a34a65` says "five Rust provider commands". `docs/ipc-inventory.md` at that commit lists six Rust-only commands
(line 16). The message is wrong; the history is not rewritten.

### Windows transport test

Runs `44359783014` (3c74b97), `44362522292` (994e9e9) and `44365881615` (32f18d6) failed on `windows-latest` in
`a_connection_that_cannot_be_made_is_a_transport_failure_that_holds_no_key`, with `left: Timeout, right: Transport`. The
call failed after about 1.5 s, the stall limit, and not at the connect limit. That points to a connection that was
accepted, with no reply in time. The test released a loopback port and expected it to refuse connections. The most likely cause is that
another test of the same binary, one of the stall tests, took the freed port. This is not confirmed on Windows.

The test now holds its own server for its whole run. The server closes each connection without a reply, and the test
asserts the same transport failure with no key. Product code is unchanged. A refused connection is no longer covered
by this test.

Run `10d6cf2` passed the Windows job, `Shell on windows-latest`, on the pull request. That shows the test passes with the
change. It does not prove the cause.

### Release gate

The gate of `0b4c529` filtered by `head_branch`. `tests/shellRelease.test.ts` forbids that filter and requires
`head_sha="$RELEASE_SHA"`. Run `44367461607` failed on it in `Verify source` and in `Persistence reliability` on three
systems. `10d6cf2` selects the newest push run of the tagged commit by `head_sha`, waits for it, and requires the Linux,
Windows and macOS jobs to succeed. A branch run has the Linux job alone, so it is refused. The test was not changed.

Not verified: the gate has not run on a real `rust-v*` tag. No such tag exists yet.

### Windows persistence check

The job `windows-persistence` runs only on a manual dispatch with the input set. Run `44367522495` ran the four files
30 times with no retry. The result was `Failed runs: 0 of 30`. The logs of cycles 1 and 30 show four files and 10 tests passing; the other
logs were not read.
Those are the four files alone; the recorded failure came from the full suite. The cause of the intermittent failure is
still not established. See [`windows-persistence-flake.md`](windows-persistence-flake.md).

### CI per system, last commit with code (`10d6cf2`)

| Check | Event | Result |
| --- | --- | --- |
| Rust shell validation: Linux, Windows, macOS | pull_request (PR #23) | success, success, success (run `44372059884`) |
| Rust shell validation: Linux | push | success (run `44372042486`, cold cache) |
| CI · source: Validate GitHub Actions workflows (actionlint) | pull_request | success |
| CI · source: Verify source, Node 24 and 24.18.0 (typecheck, lint, test, build) | pull_request | success |
| CI · source: Persistence reliability, Linux, Windows, macOS | pull_request | success |
| CI · source: Renderer interaction and visual regression | pull_request | success |
| CI · source: Package and smoke, Linux, Windows, macOS | pull_request | success |
| Keyring round trip, Linux, Windows, macOS | pull_request | success |
| Rust dependency audit (cargo deny and cargo audit) | pull_request | success |
| Row gate | push and pull_request | success |

The previous head `d33dfcf` failed `CI · source` on the release gate test, as described above. The `push` run of
`d33dfcf` was cancelled by the manual dispatch on the same ref, because the workflow's concurrency group cancels the
run already in progress.

### CI times, before and after (minutes, job duration)

| Run | Event | Linux | macOS | Windows | Wall (max) | Sum | Conclusion |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `44357708861` | push, `bd94555` | 10.1 | 8.6 | 10.7 | 10.7 | 29.4 | success |
| `44359783014` | push, `3c74b97` | 12.2 | 9.3 | 5.0 | 12.2 | 26.5 | Windows failure |
| `44362522292` | push, `994e9e9` | 15.6 | 10.8 | 7.3 | 15.6 | 33.7 | Windows failure |
| `44372042486` | push, `10d6cf2` (Linux only) | 24.7 | not run | not run | 24.7 | 24.7 | success |
| `44372059884` | pull_request, `10d6cf2` | 9.6 | 9.2 | 11.6 | 11.6 | 30.4 | success |

These are not a controlled comparison. The code differs between runs. The push run `44372042486` had a cold cache:
installing trunk took 7.5 min, against 2.1 min to restore the cache in the pull request run. No performance gain is
claimed. The push-side saving is that Windows and macOS no longer run on each push to `migration/rust`.

### Decisions taken by me

- **D18.** The transport test holds its own server instead of assuming a freed port refuses. The product code was not
  changed. The refused-connection case is no longer covered by any test.
- **D19.** A push to `migration/rust` runs the Linux shell only. Windows and macOS run on `rust-v*` tags, on pull
  requests into `main`, on the nightly schedule, and on a manual dispatch. Consequence: push feedback for Windows and
  macOS now comes from the pull request.
- **D20.** The gate selects the newest push run of the tagged commit by `head_sha`, waits up to 110 minutes, and requires
  all three systems. Its timeout is 120 minutes. The build, sign and publish jobs and their secrets are unchanged.
- **D21.** The Windows persistence check is opt-in, by manual dispatch only. It runs 30 times with no retry, and keeps
  each log as an artifact for 30 days.
- **D22.** The shell workflow change is split into two commits. The intermediate commit `b49da1f` has no persistence job.
  It was checked only by parsing its YAML, not by CI.
- **D23.** Actionlint was not downloaded or run locally. The download was denied by the permission classifier, and no
  other route was tried. CI's actionlint job reported success on `10d6cf2`.

### Stops

- `GET /models` is not implemented.
- The chat stays off. `PROVIDER_CHAT_ENABLED` is `false`, and the compile-time guard in `provider_chat_flow.rs` still
  fails the build if it is turned on alone.
- No real provider call was made. Every provider test uses a loopback server.
- No tag, release, signing or publish was started.
- PR #23 was not edited. Its checks ran because of the pushes.

### Not verified

- Actionlint locally. CI's actionlint job passed.
- The release gate on a real `rust-v*` tag, and the gate's wait on a real run.
- The nightly schedule. It runs only from the default branch, so it starts once this workflow reaches `main`.
- The cause of the Windows transport failure, and the cause of the intermittent persistence failure.
- Windows and macOS windows. No window was opened.
- A real call to OpenRouter.

### Dependencies on the owner

- Review of the provider chat: the switch, the consent text, and what is sent.
- The decision on `GET /models`.
- Manual verification on Windows and macOS.
- Whether Windows and macOS should run on every pull request, or only on tags and nightly (D19).
- The composer stub was replaced by the chat section (`crates/noct-ui/src/chat_panel.rs`). The design owner should check it.
- The commit message of `0a34a65` ("five" for six commands) is left as it is.

## Removal of the TypeScript and Electron side from `migration/rust`

The owner decided that `migration/rust` keeps only the Rust product. `main` is not changed. The previous product is kept
at the annotated tag `electron-final`, on commit `c8c378b` (tag object `e5b49be`), published to `origin`. The inventory and
the decision for each group were in `docs/ts-removal-plan.md`, removed on 2026-11-26 (last change `d76762a`).

### Commits

| Commit | Step | What it does | On `origin/migration/rust` |
| --- | --- | --- | --- |
| `d76762a` | 1 | Inventory, with APAGAR, MANTER and PORTAR for each group | yes |
| `02125b3` | 2 | Freeze: the four TypeScript reference files and the observer names are copied as fixtures | yes |
| `04533c6` | 3a | Renderer `src/`, `electron/`, root `index.html`, `public/`, Vite and electron-builder config (229 files) | yes |
| `7b3da42` | 3b | `shared/`, `tests/` (104 TypeScript files and 10 screenshots), the vitest, playwright, tsconfig and eslint config. The three release-workflow checks are ported to `node:test` | yes |
| `dace06d` | 3c | Six Electron workflows, 27 scripts and the release-line manifest, three ignored TypeScript gates | no |
| `867f47c` | 3d | `package.json` and lock reduced to `playwright` 1.61.1 | no |
| `0318ecb` | 3e | READMEs, CONTRIBUTING files and the pull request template describe the Rust-only branch | no |

The owner asked for two or three commits. The work was split into seven because each step was validated on its own.

### What was kept, and why

- **Rust test data:** the four TypeScript reference files (`constants`, `providerConfiguration`, `model`, `channels`) are copied
  byte for byte to `crates/noct-core/tests/fixtures/ts-reference/`. The observer names are in
  `crates/noct-db/tests/fixtures/typescript-observer-names.json`. The three Rust tests that read them now read the copies.
- **Rust-side files:** `crates/*`, `native/` (the C++ worker source, not compiled by CI), `docs/`, `assets/`, the UI sources
  and `crates/noct-ui/assets` (the vendored Geist fonts).
- **Node scripts of the Rust flow:** `run-rust-shell.mjs` (its `--from-fixture` option is removed, because it compiled a
  release tag's TypeScript), `set-glass.mjs`, `check-shell-ui.mjs`, `check-shell-csp.mjs`, `shell-latest-json.mjs`, and
  their tests. They run on Node, as the owner allowed.
- **Electron tag patterns:** `scripts/fixtures/electron-tag-patterns.json`, frozen from `electron-final`. The release test
  checks that no shell tag matches them, as it did when it read the deleted workflows.

### Deleted, and restorable from the tag

Everything listed above as removed is in `git show electron-final:<path>`. Nothing that Rust reads was deleted by mistake.
The checks at each step found no such case: `cargo test --workspace` passed each time with the same 759 tests.

### Validation at `0318ecb`

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | passed |
| `cargo test --workspace --no-fail-fast` | 759 passed, 0 failed, 6 ignored (13 ignored tests were the TypeScript gates and are gone) |
| `trunk build --release` in `crates/noct-ui` | passed |
| `node scripts/check-shell-ui.mjs` | passed, 27 scenarios |
| `npm test` (`node --test scripts/*.test.mjs`) | 37 passed, 0 failed |
| `npm ci --ignore-scripts` | installs `playwright` and `playwright-core` only |

`npm run typecheck` and `npm run lint` no longer exist, because there is no TypeScript and no ESLint on this branch. The
sync rule that asked for them before each change to `.mjs`, `.ts` or `.js` files cannot run on this branch as written.
`npm run typecheck` and `npm run lint` ran with exit 0 before the first deletion. Their output is in the scratch folder of
the session, not in the repository.

### What is lost, stated plainly

- The parity checks against the previous TypeScript product: the row gate (10 tests), the Rust-written gate (2 tests) and
  the output differential over the nine release lines (1 test). They run at `electron-final` only.
- The migration fixtures of the nine release lines, which were built from the tags' sources.
- The Windows persistence check of four TypeScript files (`docs/windows-persistence-flake.md`). Its cause is not
  established, and it can no longer run on this branch.
- The Electron packaging, the packaged smoke, the updater rehearsal and the Electron stable release. They stay on `main`.
- The React renderer's visual baselines (ten screenshots) and the design-system CSS check (`check-design-system.mjs`).
- The C++ rollback worker is no longer compiled or tested in CI.
- The `--from-fixture` option of the launcher.

### Owner decisions and dependencies

1. **PR #23 must not be merged into `main` as it stands.** Merging `migration/rust` into `main` would remove the Electron
   product from `main`. No action on the PR was taken.
2. **CodeQL** has `javascript-typescript` in its language set. That is a repository setting, outside the files.
3. **`AGENTS.md`** still describes the Electron invariants. Not changed here.
4. **The sync rule** names `npm run typecheck` and `npm run lint`, which no longer exist on this branch.
5. **Whether the parity checks are recreated** as Rust-only tests against frozen data, on another branch or tag.
6. **Windows and macOS CI** are not awaited for each push, as the owner asked. The `rust-shell-validation.yml` run on a push
   to `migration/rust` is Linux only; Windows and macOS run on tags, on pull requests to `main`, nightly and on dispatch.

### Stops

None of the stop conditions was hit. The changes touched no secret, no updater key, no signing, no CI secret, no `main`
and no PR.


## Practical mode: report per item (owner's order)

The owner asked for a short report at the end of each item, here. The order and the rules of that message apply: no wait for
the Windows and macOS CI results, Linux checks and local `cargo test` are the validation, no flake hunt, and no review
document unless the code writes the person's files or touches the provider key. Small reversible choices are made here and
listed; irreversible ones are left to the owner.

### Item 1: removal of the TypeScript and Electron side

Done. Section "Removal of the TypeScript and Electron side" lists the commits, what was kept, what was deleted (restorable
from `electron-final`, commit `c8c378b`), and the checks. Pushed to `migration/rust` up to `d44bca7`.

### Item 2: the chat end to end with OpenRouter

What the code already does, checked by reading it and by the tests of this branch:

- The key is typed in the settings area, checked in format and saved in the operating system store. The interface never reads
  it back.
- The model is typed as an identifier. There is no list of models and no `GET /models`.
- The chat sends a question and the top-level text files that the context rules allow to `https://openrouter.ai/api/v1/chat/completions`,
  streams the answer, and saves it into a conversation on request.
- The switch `PROVIDER_CHAT_ENABLED` is `false`. While it is off, a question is refused before any file is read and before any
  connection is made. The owner turns it on after testing.

What was checked in this round: the chain from the question to the saved answer is covered by `provider_chat_flow.rs` and
`provider_transport.rs` against a local fake server; the interface check runs in Chromium with a mocked shell and the switch
off. Not checked: a real window, a real key, and a real OpenRouter reply. No test calls OpenRouter.

What is not done: a model list, other providers, and the writing of changes proposed by the assistant (item 3). The README
claim that no provider was called was wrong, and it was corrected in this round.

Decision taken here: the switch stays `false`. Turning it on needs the owner's test. The existing compile-time guard in
`provider_chat_flow.rs` keeps it that way until the owner changes both places.

### Item 3: changes proposed by the assistant, approved one by one, written inside the project, with undo

Design, written before the code.

A proposal is one text file of the project: a relative path, the complete new content (UTF-8, at most 32 KiB, the same cap as
the chat context), and the bytes that the file had when the proposal was made, or no bytes for a new file. Proposals and their
results are kept in a journal in the data folder, never in the project. The journal is one file, written atomically with mode
0600. The person approves or rejects each proposal on its own; there is no apply-all.

Approval checks, in order, and writes nothing if any check fails:

1. the workspace is authorized in the store and available, as for the chat;
2. the path is relative, has no `..` component, no root or drive prefix, and no NUL byte;
3. no component of the path is a symbolic link, each checked with `symlink_metadata`, and the parent resolved in the project
   root stays inside it;
4. the name is not sensitive: `.env` and `.env.*`, `*.pem`, `*.key`, `*.p12`, `*.pfx`, `id_rsa*`, `.npmrc`, `.netrc`, names that
   contain `credential` or `secret`, and anything under `.git`;
5. the file on disk holds exactly the base bytes of the proposal, compared byte by byte (no hash), so a write happens only when
   nothing changed since the proposal.

Only then the new content is written with `write_atomic_file` (temporary file in the same folder, sync, rename). The base is in
the journal, so the previous bytes are never lost.

Undo checks that the file still holds the content that the approval wrote. If it changed since, undo refuses and says so. A new
file is undone by removing it, under the same check. Nothing outside the workspace is read or written by this path, and the
data folder receives only the journal.

Known limit, stated now: another program that changes the file between check 5 and the rename is not excluded. The window is
one comparison and one rename.

Not in this round: the assistant producing proposals (the chat returns text only, so the proposal is created by a command with
the same fields), the interface that approves each proposal, and hunk-level review (`editHunk`, which the owner had placed in
beta 2; see `docs/rust-edit-hunk-design.md`).

#### Report of item 3 (backend)

Built: the approval path of a change to a project file, without interface and without the assistant's proposals.

- `noct-core::file_change`: the path rules (relative, no `..`, no link-like names, no sensitive names), the states of a change
  (pending, applied, rejected, undone), and the journal's limits.
- `noct-app::file_changes`: the disk checks and the write. Every write goes through `write_atomic_file`.
- Commands `file_changes_list`, `file_change_propose`, `file_change_approve`, `file_change_reject` and `file_change_undo`,
  granted to the `main` window only.

Tests: 14 rule tests in `crates/noct-core/tests/file_change.rs` and 18 disk tests in `crates/noct-app/tests/file_changes.rs`.
They cover: nothing is written before approval; traversal and absolute paths refused; sensitive names refused; a folder or file
that is a symbolic link is refused, also when it is swapped in after the proposal; a conflict on approve and on undo keeps the
other edit; reject writes nothing; undo restores the text or removes the created file; the journal survives a restart; a damaged
journal is refused and not overwritten; the open limit; the text and size limits; the journal is mode 0600; an approved write
keeps the mode of the file. The symbolic link tests run on Unix only.

Decisions taken here (small, reversible):

- The base of a change must be UTF-8 text. A binary file is refused. The design said bytes; text is the smaller honest scope.
- Limits: 32 KiB per file (the chat's per-file cap), 50 open changes across all projects, 50 closed changes kept.
- The journal is `file-changes.json` in the data folder, mode 0600. It is not a SQLite table, so no migration was added.
- A new file needs an existing folder. Folders are not created.
- Reject touches no file, so it does not require the project to be authorized.
- Approve marks the change applied in the journal before it writes, and sets it back to pending if the write fails.
- A sensitive word anywhere in a name (`credential`, `secret`, `private-key`, `private_key`) refuses the path. The rule is broad on purpose.

Not done in this round:

- No interface panel for the changes. The commands exist and are tested; nothing in the app calls them yet.
- The assistant does not produce proposals. The chat still answers with text only. The format of a proposal from the assistant is
  a decision for the owner: it changes what the chat is asked to return, and the chat is still switched off.
- No review by hunk (`editHunk`, item 5).

Known limits, stated:

- Between the check and the rename, another program that writes the same file is not excluded. The window is one comparison
  and one rename.
- Without `O_NOFOLLOW`, a link swapped in between the check and the open is not excluded. The check runs on each name.
- A crash after the journal is saved as applied and before the write leaves the change marked applied with the file unchanged.
  Undo then refuses, because the file does not hold the content. Recovery is manual.
- Not run on Windows or macOS.

Validation at this commit: `cargo fmt --all -- --check` clean, `cargo clippy --workspace --all-targets -- -D warnings` clean,
`cargo test --workspace --no-fail-fast` exit 0, `trunk build --release` exit 0, `scripts/check-shell-ui.mjs` exit 0. A
formatting-only change to the disk test file came after the workspace run; that file was then rerun (18 passed) and clippy of
`noct-app` was rerun clean.

## Real window: three blockers reported by the owner (2026-11-18)

The owner saved an OpenRouter key in the real window and could not choose a model, type in the chat, or choose a project.
Diagnosis, run with `node scripts/run-rust-shell.mjs dev --data-dir /tmp/noct-bug` and read in the code:

| Symptom | Cause |
| --- | --- |
| No project to choose | No command saved a project. `Workspaces::touch` was only reached through `conversations_create`, which needs a saved project. A new data folder has 0 rows in `workspaces` (read with `sqlite3` from `/tmp/noct-bug/nocturne.db`), and the "+" of the sidebar was `disabled` on purpose. |
| Model field disabled | The default model is saved per project: `provider_panel.rs` disabled the field while `shared.current` was `None`, which is always the case with no project. |
| Cannot type in the chat | The text box had `prop:disabled=!state.enabled()`, and `PROVIDER_CHAT_ENABLED` was `false`, so `provider_chat_status` always said off. With no project the empty state also hid the suggestions. |

No ACL or IPC error was involved: every command the window called was granted. `--from-fixture v1.0.2` could not be used:
the launcher refuses it since the TypeScript side was removed (`scripts/run-rust-shell.test.mjs` checks the refusal).

Fix: `workspace_add` opens the system folder picker from the shell (`tauri-plugin-dialog` 2.8.3, called only from Rust; the
window has no dialog permission), checks the folder (absolute, exists, is a folder, readable, outside the blocked roots, not
saved yet) and saves it authorized. The window opens on the project chosen last (kept per viewer), or the first one. The chat
switch is a setting (`providerChatEnabled`, on by default) with `provider_chat_set_enabled`. The text box accepts text
whenever there is a project, and a line over it names what is missing (project, chat off, key, model, permission) with the
button that resolves it.

## Model list, assistant proposals and Construir (2026-11-19)

**Model list.** `models_list` reads OpenRouter's public list with no credential, under the same network policy as the
chat; the policy additions are in `docs/provider-allowlist-design.md`, section 13.1. Ajustes and the composer pill choose the
model from it; the typed id remains when the list cannot be read.

**Proposals from the assistant (C).** The prompt asks the model to put the whole new text of a file in a block that opens
with a line of four backticks and `noct-change path=<relative path>`, and closes with the same four backticks. When an answer ends complete, the shell reads the blocks of
the text it received itself (never text from the window) with `noct_core::proposal_block::read_blocks` and records each
accepted block as a **pending** change through the same `file_changes::propose` as a proposal from the window. Nothing is
written to the project; the window is told the recorded changes with a `proposed` event before `done`.

Security review of C:

- The answer is untrusted. A block is read only in its exact shape (a fence of four or more backticks on its own line, the
  word, `path=` with no space, the same fence to close it); an open block is ignored.
- The path passes `check_relative_path` (relative, no `..`, no root, drive, backslash, colon or NUL, no sensitive name such
  as `.env`, keys, certificates, `.git`), then the disk checks of `propose` (every folder real and inside the project, no
  symbolic link). The text fits 32 KiB. A path appears once; at most 5 blocks per answer; the journal keeps at most 50 open
  changes. A refused block is counted and shown as a number, never applied.
- A failed, truncated or cancelled answer proposes nothing: blocks are read only after the provider's `[DONE]`.
- Nothing is approved automatically. Approval happens only in Construir, one change at a time, and re-checks that the file
  still holds the text it had when the change was proposed.
- A project file that tries to steer the model (prompt injection) can at most produce pending proposals, which the person
  sees as a diff before any write.
- The window shows a block as one line ("[alteração proposta: path]"), so the file text does not fill the conversation; all
  text is drawn as text.

Tests: `crates/noct-core/src/proposal_block.rs` (shape, traversal, sensitive names, size, repeats, cap, open blocks, line
ends) and `crates/noct-app/tests/provider_chat_flow.rs` (pending changes recorded and no file written; refusals; a symbolic
link out of the project; an answer without blocks; an unsaved project; the prompt).

**Construir (B).** The panel lists the project's changes: waiting for approval (one card per file with a line diff, the
counts, "Arquivo novo" for a new file, Aprovar and Recusar), applied (Desfazer) and closed. A refused approval (the file
changed since the proposal) leaves the change waiting and shows the shell's message. The diff is computed in the window and
bounded; a text too long to compare is shown whole. Review stays read-only: the conversation only records proposals.

## Owner's review of C, safe answer rendering, and Documentar (2026-11-19)

CI: "Rust shell validation" on `0efa624` (run 44484850830) finished **success** on Linux.

### Paths an assistant proposal may never name (rule of `check_relative_path`)

A block whose path matches any rule below is refused and counted; nothing is recorded and nothing is written. The same
check runs again when a change is approved.

- **Git:** anything under a `.git` folder at any depth (`.git/config`, `.git/hooks/*`, `sub/.git/...`), and the Git files
  that make Git run or fetch something: `.gitattributes`, `.gitmodules`, `.gitconfig`.
- **CI:** `.github/workflows/**`, `.github/actions/**`, `.gitlab-ci.yml`, `.gitlab/**`, `.circleci/**`, `.buildkite/**`,
  `.travis.yml`, `azure-pipelines.yml`, `bitbucket-pipelines.yml`, `.drone.yml`, `Jenkinsfile`. Other files under
  `.github` (issue templates, `CODEOWNERS`) stay allowed.
- **Files that run on their own when the project is opened or committed:** hook managers (`.husky/**`, `.githooks/**`,
  `.pre-commit-config.yaml`, `.pre-commit-hooks.yaml`, `lefthook.yml`), editor and container settings that can start tasks
  (`.vscode/**`, `.idea/**`, `.devcontainer/**`), and files a shell or an editor reads on entering the folder (`.envrc`,
  `.exrc`, `.nvimrc`, `.nvim.lua`, `.lvimrc`).
- **Windows device names**, with or without an extension and in any case: `CON`, `PRN`, `AUX`, `NUL`, `COM0`–`COM9`,
  `LPT0`–`LPT9` (also the superscript digits Windows accepts).
- **Encoded or disguised paths:** any percent escape (`%2e%2e`, `%2F`, `%5c`), a backslash, a colon, NUL or another control
  character, and characters that look like a dot or a slash (fullwidth dot and slash, one-dot leader, division slash).
- The rules already in place: absolute paths, `..`, empty or `.` names, a trailing dot or space, sensitive names (`.env`,
  keys, certificates, credentials), symbolic links, folders outside the project.

Not on the list: build files that run only when the person runs a build or an install (`build.rs`, `package.json`
scripts, `Makefile`). They are shown as a diff and wait for approval like any other file.

Tests: `crates/noct-core/tests/file_change.rs` (each rule, and names close to them that stay allowed) and
`crates/noct-app/tests/provider_chat_flow.rs`
(`blocks_for_git_ci_self_running_files_device_names_and_encoded_paths_are_refused`: one answer per path, each counted as
refused, the journal stays empty, `.git/config` is unchanged and no hook or workflow file is created).

### Approval card

The card shows the whole path in a larger mono font, never cut. A change to a file that exists has the tag "Sobrescreve"
and a line saying that approving replaces the whole content of the file; a new file has the tag "Arquivo novo".
A file that runs code when the project is built or installed (`build.rs`, Makefile, justfile, `package.json`, Dockerfile,
`*.sh`, `*.ps1`, and a `Cargo.toml` with a build script or build dependencies before or after the change) is allowed and
carries a visible warning: "Este arquivo roda código quando você compila ou instala. Leia o diff com atenção."
(`noct_core::file_change::runs_on_build`).

### Known limitation

A proposal only creates a file or rewrites a whole file, up to 32 KiB of text. Editing part of a file (a hunk or a range
of lines) is not supported yet.

### Answers drawn as Markdown, without HTML

The answer is read in the window with `pulldown-cmark` 0.13 (no default features, so no HTML writer) into a small tree
(`crates/noct-ui/src/markdown.rs`) whose leaves are only text, and drawn with Leptos elements whose text is set as text.
Supported: headings, paragraphs, bold, italic, strikethrough, inline code, code blocks (mono font, own background, a Copiar
button), lists, task markers, quotes, rules and tables (in a box that scrolls sideways). Elements carry classes only.

Security review:

- Raw HTML, block or inline, is kept as text: `<script>`, `<img onerror>`, `<a href=javascript:>`, `<iframe>` are shown as
  the characters the model wrote. The tree has no element kind for HTML, so there is no path from the answer to markup.
- Links are not clickable: the text is shown, then the address in parentheses, as text. Images are not loaded: only
  "[imagem: description]" is shown. So an answer makes no request and opens nothing.
- No inline style and no `innerHTML`; the page policy (no inline style) is unchanged.
- Nesting is cut at 16 levels: deeper content stays as text, which bounds the recursion of drawing, hashing and dropping.
  The answer itself is capped by the shell at 64 KiB; the parser was tested with four times that.
- While an answer streams, it is parsed again at each piece and each top-level block is keyed by position and content, so
  finished blocks keep their elements; an open code fence is drawn as code, and a half table as a paragraph until its
  delimiter line arrives.

Tests: unit tests in `markdown.rs` (subset, raw HTML, `javascript:` link, remote image, malformed table, cut answer, depth,
huge answer, stable keys) and the Chromium scenario `chat-renders-markdown-without-html` (no `script`, `img`, `a`,
`iframe`, `[style]`, `[src]` or `[href]` inside the answer, nothing ran, no request to the image or link hosts, wide table
scrolls inside the answer, the open fence while streaming).

### Window fixes

- Sidebar: the project path is one line, the home folder is shown as `~`, a long path ends in an ellipsis, and the full
  path is the element's `title`.
- Conversation: room under the last line (padding of the conversation, always applied) and a short fade under it, applied
  only under `@supports` for `mask-image` and through a class on the area, not `:has()`; the view follows a growing answer only while the
  person is at its end, and an answer that ends does not move a person who scrolled up (checked in Chromium).

### Documentar

Documentar uses the same approval flow as Construir and shows only the changes to Markdown files of the project (`.md`,
`.markdown`); Construir shows the others. Under an answer, "Revisar em Documentar" appears when it proposed Markdown files.
The split is a view of the same journal: the shell's path rules and the approval re-check are the same for both.

## Finishing touches from the real window (2026-11-20)

### The app's select

Ajustes no longer uses `<select>`, whose open list GTK draws. `crates/noct-ui/src/select.rs` is a select-only combobox:
a button with the value, and a `listbox` popup in the app's style (glass, light and dark, the selected option marked).
Keyboard: arrows, Home/End, Enter and Space choose, Esc closes and keeps the focus, a letter jumps to the next option that
starts with it; a click outside closes. The list is mounted in `<body>` (a pane with `overflow: hidden` or a backdrop
filter would cut a fixed list inside it) and placed by custom properties `--select-*` that `dom::set_root_property` sets
on `<html>` through the CSS object model; it ignores any name that is not a custom property, and the policy test allows
that one call. It opens upward when there is no room below. Checked in Chromium: mouse, keyboard, Esc, click outside,
both themes, position under the button and upward at a short window.

### Jagged edges on the cards

- **Chromium:** not reproduced. Card corners zoomed 10x, glass on and off, light and dark, are antialiased.
- **Real window:** not captured. The installed `import` cannot read the X display here, and Playwright's WebKit needs
  system libraries that need sudo. The screen scale is 1 and the window is opaque, so fractional scaling and a
  transparent window are ruled out; no card has a transform or `will-change`.
- **Probable cause:** WebKitGTK composites with TextureMapper, which clips a rounded corner of a composited layer with a
  stencil, without antialiasing. Every glass pane is a composited layer (`backdrop-filter`), and `.content`, `.list` and
  `.card--flush` rounded their content with `overflow: hidden`.
- **Change:** those three no longer clip; their first and last children take the inner radius (`.diff` and `.main`
  scroll on their own). The glass is unchanged.
- **For the owner to test:** Ajustes › Aparência › "Bordas sem camadas (teste)" keeps the glass on the sidebar and the
  window pane but draws the cards, lists, composer and popups without their own blur. If the edges are smooth only with it
  on, the cause is the blur clip of the panes themselves, and the next step is to move that blur to an inset pseudo-element.

### The webview does not act as a browser in a release build

`crates/noct-ui/src/browser_guard.rs`, from WebAssembly, in a release build only (`debug_assertions` off): the right
click shows no page menu, except in a text box, an editable element or over a selection (copy and paste stay); F5,
Ctrl/Cmd+R, Ctrl/Cmd+Shift+R, Alt+Left/Right, Cmd+[ and ], Ctrl/Cmd+P and the browser keys are cancelled; `dragover` and
`drop` are cancelled. The window keeps Tauri's drag and drop handler (a dropped file is a shell event, not a navigation),
and the navigation policy refuses `file:` pages. The window sets `.devtools(DEVTOOLS)`, true only in a debug build, and
Tauri's `devtools` feature is not enabled (`cargo tree -e features -i tauri` shows none). Tests: unit test of the
shortcut list, Chromium checks of the menu, the shortcuts, the allowed copy and paste and the drop, and
`crates/noct-app/tests/window.rs` for the inspector and the drop handler. A real reload or navigation is not exercised.

## Pending: merge of `migration/rust` into `main`

> **Resolved.** `migration/rust` was merged into `main` by PR #23 (2026-11-18), #26 and #27 (2026-11-20). The
> Electron checks named below no longer exist. The note is kept as history.

`main` holds two merges of this branch: `eaff240` (PR #19) and `f8d2801` (PR #22). At `6fd041e`, `migration/rust` has 73
commits that `main` does not have, and `main` has the two merge commits that `migration/rust` does not have. Neither merge changed
`README.md`, so `main` keeps the product README and this branch keeps its own.

**Blocker for a pull request from this branch into `main`:** `npm run verify:release-metadata` fails on this branch:

```text
Error: README.md diverge da fonte canônica; valor esperado: v1.0.5
```

The check requires `README.md` to contain the version. The branch README is the migration's own document, which starts with
`# NOCT: state of the Rust migration`, and it does not contain `v1.0.5`. The README of `main` contains it. The check runs in
`package-validation.yml` on pull requests into `main`, and in `stable-release.yml` for `v` tags.

Nothing is changed by this note. Before the merge, choose one:

1. Keep the product README on `main`, and move the branch README to a document under `docs/` (for example
   `docs/rust-migration.md`). The check stays as it is. This is the smallest change.
2. Keep the branch README and change the check, so that it does not read the product's README.
3. Take the `main` README, and move the migration status to another file.

Only `verify:release-metadata` was run for this note. The other Electron checks were not run for it.

The blockers before a merge, the checks on PR #23, and the rules for both products to coexist until the TypeScript cleanup are in
`rust-merge-plan.md`, removed on 2026-11-26 (the tag `electron-final` has its last version).

## First release and repository cleanup (2026-11-23)

### Installers (`[security-review]`: a new workflow)

- `.github/workflows/build-installers.yml`, started by hand only (`workflow_dispatch`). Jobs: Linux on Ubuntu 22.04 (.deb,
  AppImage), Windows (NSIS .exe), macOS aarch64 and x86_64 (.dmg, both on the Apple silicon runner). A last job writes
  `SHA256SUMS` and keeps everything as the `noct-installers` artifact for 14 days.
- **Review of the trust boundary.** Token: `contents: read` at the top and in each job; no `write` permission, no
  `environment`, no `secrets.` reference, `persist-credentials: false`, every action pinned by commit, `--no-sign` (no
  updater key, no code signing). It pushes nothing and publishes nothing. `crates/noct-app/tests/installers.rs` locks
  these properties, the bundle targets and icons, the release interface build and the identifier.
- **Defect found and fixed.** The Tauri CLI runs `beforeBuildCommand` and `beforeDevCommand` from `crates/` (the folder
  above `crates/noct-app`), so `cd ../noct-ui` failed: `cargo tauri build` had never worked, and `shell-release.yml`
  would have failed at its build step. Both commands are now `cd noct-ui && ...`; a local `cargo tauri build --bundles
  deb,appimage` was run to confirm (result in the commit message).
- **Config.** `bundle.active` stays `false`; the workflow turns it on for its build. `bundle.targets` is deb, appimage,
  nsis, dmg; `bundle.icon` is `icons/icon.png` (512 px, used for Linux and for the macOS .icns the bundler makes) and
  `icons/icon.ico` (16 and 32 px only: the Windows installer icon is small; a larger .ico is a follow-up).
- **Identifier.** `com.nocturne.codex` is kept: Tauri names the data folder after it, and the credential store's service
  starts with it. Changing it would orphan installed data and keys.
- **Version.** The configuration says `0.1.0` (Cargo.toml, tauri.conf.json, package.json; changed 2026-11-24), so the
  installers are named `NOCT_0.1.0_*`, matching `docs/release-notes-0.1.0.md`. The Electron history keeps `1.0.5`.
- **Not verified:** no run of the workflow (a `workflow_dispatch` workflow can be started only once it is on the default
  branch); the Windows and macOS bundles; the AppImage on a hosted runner; an install on any system.

### What `shell-release.yml` needs from the owner (nothing here was changed)

1. **The `release` environment.** The `sign` job uses `environment: release`. The repository has `copilot`, `realese`
   (misspelled) and `stable-release`; there is no `release`. Create `release` (or rename `realese`), with deployments
   limited to tags `rust-v*`, and preferably a required reviewer.
2. **Two secrets in that environment:** `TAURI_SIGNING_PRIVATE_KEY` (the minisign private key whose public key is in
   `plugins.updater.pubkey`, `40503B624D87D646`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Who holds the key is ADR 0001,
   open decision 1.
3. **The tag rule.** The tag is `rust-vX.Y.Z` and must equal `rust-v` + the `version` of `tauri.conf.json`, which must
   equal the workspace `Cargo.toml` version; the tag must point at the commit that runs; for `rust-v0.1.0` the version
   must first be set to `0.1.0` in `Cargo.toml`, `crates/noct-app/tauri.conf.json` (and `package.json`, for consistency).
4. **Release notes at `docs/releases/<tag>.md`** (for example `docs/releases/rust-v0.1.0.md`); the gate fails without
   it. `docs/release-notes-0.1.0.md` can be copied there.
5. **The shell CI of the tag** must pass on Linux, Windows and macOS (the tag starts it; the gate waits up to 110 min).
6. **No release with that tag may exist**; the workflow never replaces one. It publishes a pre-release, never "latest",
   and moves the `updater-feed` pre-release to it.
7. What it does not do: it uploads only the updater artifacts (AppImage, NSIS .exe, `.app.tar.gz`) and `latest.json`,
   not the .deb or the .dmg; the installers for people come from `build-installers.yml`. The window still has no updater
   permission (`docs/shell-updater.md`, "Activation"), so no installed copy checks for updates yet.

### Repository cleanup

- **Templates.** The pull request checklist is Rust-only (fmt, clippy, test, trunk, the two Chromium checks) and accepts
  "não se aplica"; its security lines name the key vault, writes inside the project after approval, the command boundary
  and the CSP. The issue templates ask for the installer and the system, and list the current areas.
- **Dependabot.** Cargo (workspace) and GitHub Actions, weekly, minor and patch grouped, majors left out. npm is kept for
  `playwright` alone, since it runs the Chromium checks.
- **CodeQL** is the repository's default setup, not a file here: languages actions, c-cpp, javascript-typescript, rust.
  Each still has code: `.github/workflows`, the C++ worker in `native/`, `scripts/*.mjs`, the crates. Nothing to remove;
  `javascript` and `typescript` in the list are aliases of `javascript-typescript`.
- **Security · dependencies** (`dependency-security.yml`) now audits only the Playwright tree (there is no production npm
  tree) and pins its actions by commit. The Rust tree is audited by `rust-audit.yml`.
- **Workflow with no remaining function, proposed for removal (not removed):** `build-provenance-dry-run.yml`. Its dry
  run verified the attestation action that `shell-release.yml` now uses at the same pin.
- **Removed as dead:** `build/icon.png` (a copy of the app icon for electron-builder), `.impeccable/surfaces/src-app-tsx.md`
  (the React surface), the Electron/Vite/TS lines of `.gitignore` and `.editorconfig`. Updated: `assets/README.md`,
  `PRODUCT.md`, `IDENTITY-MIGRATION.md`, `CONTRIBUTING*`, `SECURITY*`, `CHANGELOG.md`.
- **Removed in the cleanup of 2026-11-26:** the Electron-line release notes in `docs/releases/` (eleven files, up to `v1.0.5`; the folder keeps a `README.md`, because `shell-release.yml` requires `docs/releases/<tag>.md` for each `rust-v*` tag), `docs/Docs-roadmap/` (two PDFs of the product named Studio), `docs/New_Design/mockup-vidro.html`, `docs/release-readiness-1.0.md`, `docs/security-review-beta1.md`, `docs/rust-merge-plan.md`, `docs/rust-row-gate-workflow.md` and `docs/ts-removal-plan.md` (this last one is not in `electron-final`; its last change is `d76762a`). The Electron product up to `1.0.5` stays in the tag `electron-final`. Also removed: the Geist fonts under `crates/noct-ui/assets` (no build copied them; the interface names Inter), `.impeccable/` (design-tool state, now ignored), and the `.gitignore` entries of tools the project does not use (`.npm`, `dist-native`, `release`, `coverage`, `.nyc_output`, `*.ntvs*`, `*.njsproj`, `*.sln`). Comments that cited removed generator scripts now name the tag `electron-final` as the source of the fixtures. Kept: `docs/npm-audit-triage.md`, because the npm tree still has `playwright`. Open: `DESIGN.md` still specifies Geist as the interface font.

### Leftovers of the previous product that stay (not clearly dead)

- **Documents that describe the Electron product:** `docs/architecture*.md`, `docs/installation*.md`,
  `docs/development*.md`, `docs/updates*.md`, `docs/providers*.md`, `docs/privacy*.md`, `docs/security*.md`,
  `docs/code-intelligence*.md`, `docs/compatibility*.md` (the platform floors are current), `docs/github-actions.md`,
  `docs/ipc-inventory.md`, `docs/release-readiness-1.0.2.md`, `docs/security-audit-1.0.md`,
  `docs/New_Design/*.html`. They are history or need a rewrite, not a deletion; `docs/README.md` already says some
  describe the previous product.
- **Stale references:** `docs/shell-updater.md` named `tests/shellRelease.test.ts` and `tests/shellLatestJson.test.ts`;
  they now name `scripts/shell-release.test.mjs` and `scripts/shell-latest-json.test.mjs`. `shell-release.yml` comments name
  `stable-release.yml`, which no longer exists (the rule they explain, never marking a shell release "latest", still
  holds for Electron installs that read it).
- **Fixtures kept on purpose:** `scripts/fixtures/electron-tag-patterns.json` (the release test reads it),
  `crates/noct-db/tests/fixtures/typescript-observer-names.json` and the schema hashes (parity data).
- **Code comments:** about 100 files under `crates/` name the TypeScript module they were ported from. They are
  provenance, not dead code.
- **Owner:** private vulnerability reporting is off on the repository (`SECURITY.md` points to it).

## Suggestions restored: analysis and generated change (2026-11-24) [security-review]

- `suggestions_analyze` passes the chat's checks (switch, authorized project, consent, OpenRouter model, key), sends the
  chat's context (sensitive and ignored files excluded, caps kept) and runs on the chat's `ChatControl`
  (`provider_chat_cancel` stops it). The reply never reaches the window; only a character count does.
- The reply is untrusted: `noct_core::suggestion_analysis::read_reply` refuses a missing, invalid or truncated snapshot (nothing
  stored), keeps at most 10 suggestions, discards one over a bound or with a repeated identity, and drops any affected file
  that fails `check_relative_path`, is sensitive or is ignored. Storage is the existing reconciliation (schema 32 unchanged).
- `suggestions_status` only changes the status. `suggestion_generate_change` requires an accepted suggestion, reads its
  affected files through `resolve_target`/`read_base`, and records only blocks for those files as pending changes through
  the existing journal; nothing is written to the project before approval in Construir.
- `suggestions_list` reads a project's suggestions (every status, at most 200) after the authorization check.
- Interface: Revisar has the tabs Conversa and Sugestões. Model text is shown as text (affected files never as links;
  reasoning and proposed changes through `markdown.rs`). Checked in Chromium with a mocked shell, light and dark: no page
  error, no link, image or script in the tab, filter and failure states shown. Not checked in a real window.

## Chat findings saved in Sugestões (2026-11-24) [security-review]

- While "Salvar achados do chat em Sugestões" is on (default; `chatFindingsEnabled`, `provider_chat_set_findings`), the chat's
  instructions ask for a `nocturne-suggestions` block only for a concrete problem. The shell reads it from the complete
  answer only (a failed or cancelled answer saves nothing) with `read_findings`: the analysis validator, at most 5, bounds,
  affected files confined to the project, duplicates by identity; an invalid block saves nothing. Storage is the existing
  reconciliation, so a new finding is "Nova" and a known one keeps its status. The window only gets the count.

## Context, evidence and memory of Sugestões (2026-11-24) [security-review]

- A call carries the project tree (names and sizes, at most 2,000, no sensitive or ignored file, no link followed) and the
  chosen files (`context_tree`, `context_selection_set`; the choice is kept per project under the hidden `maintenance.`
  prefix, default = the top level). The caps stay 32 KiB per file and 128 KiB in all. Consent is now `openrouter-v2`, so every
  project is asked again; the card names the tree and the earlier suggestions.
- Evidence: the model gives `path:start-end`; the shell keeps it only when the file's text was sent and the lines exist, reads
  at most 12 lines from the disk with the change path rules (no link, no sensitive file), and stores that excerpt with the
  source `noct:verificada` in the existing `evidence` column. Older rows show "evidência não verificada".
- Memory: an analysis carries up to 30 earlier suggestions (title, status, last reason; at most 3 KiB) as data. A refusal may
  carry a one-line reason in `result` through `suggestions_status`. Schema 32 is unchanged.
- `check:ui` knew neither `suggestions_list` (the lamp's count, since `d5e1e01`) nor `context_tree`; its mock now answers both.

## Portable Linux package (2026-11-25)

- `build-installers.yml` (Linux job) also writes `NOCT_<version>_linux-x86_64.tar.gz` with `scripts/package-linux-portable.sh`:
  the release binary as `noct`, `icon.png`, an example `noct.desktop` and a `LEIA-ME.txt` with the system packages (Arch:
  `webkit2gtk-4.1 gtk3`; Debian and Ubuntu: `libwebkit2gtk-4.1-0 libgtk-3-0`). It is in the run's artifacts and in
  `SHA256SUMS`. Nothing is signed, and no updater artifact changes. It is the Linux build for systems where the AppImage's
  embedded WebKit runs without hardware acceleration.
- The Linux job checks the three files. Its checks were joined with `&&` on one line, which `bash -e` does not stop on, so a
  missing file did not fail the step; each check is now a line of its own.
- Checked here: the script ran on the release binary of this machine. The archive has the executable mode, the root owner and
  the three files; the binary links `webkit2gtk-4.1`, `gtk-3`, `soup-3` and `javascriptcoregtk-4.1`, which the list covers.
  Not checked: a run of the workflow on GitHub, and the package on another distribution.

## Chat findings say what became of them (2026-11-25)

- Cause of the silence: `store_findings` returned 0 for every case that stored nothing (no block, a block that does not read,
  every finding refused by a bound or repeated, a store or project error), and the shell sent an event only when it saved
  at least one. Nothing told the person why.
- Every complete answer now sends one `findings` event before `done`, with a status and two counts: `off` (the switch in
  Ajustes is off), `absent` (no block), `invalid` (the block does not read), `empty` (`[]`), `discarded` (every finding refused
  by a bound or repeated), `saved` (`saved` stored, `discarded` refused) and `failed` (the project or the store refused). Only
  the status and the counts travel; the words under the answer are the interface's.
- The line under the answer (`chat-findings-status`) always says which one it was. "Abrir Sugestões" appears only when findings
  were saved, and only then the list and the lamp are read again (`suggestions_version`).
- The instruction asks for the block whenever the answer names a concrete problem, even when the text already describes it,
  and shows a minimal finding. A test reads that example with the validator of a reply, so the example is a stored finding.
- Checked here: `noct-core` tests (the example reads), `noct-app/tests/findings_report.rs` (each status, and a project that is
  not saved fails), `noct-ui` tests (the frame, the words of each status, the run's lifecycle) and the Chromium check (two
  scenarios: no block, and saved with one refused; 34 of 34 passed). Not checked: a real provider reply, and how often models
  follow the stronger instruction; that needs a call with a real key.
- Not marked `[security-review]`: no validation, storage or capability changed. The window receives a status and two counts,
  never the model's text.

## Agents as profiles of the modes (2026-11-26) [security-review]

- Each mode has one agent in Ajustes › Agentes. The name follows the mode. The instruction is text of up to 4 KiB, with line
  endings normalized and control characters refused. The model is the project's when empty, or an identifier that passes
  `validated_model_id`. The three agents are one settings value, `agentProfiles`, a JSON list, written with the defaults on the
  first run. A stored value that does not pass the checks reads as the default of its mode, and the next save writes a valid list.
- The Review agent's default instruction is the sentence the chat already had. The rules the shell adds after it are fixed in
  `provider_chat::RULES`. `the_default_agent_sends_the_prompt_the_chat_always_sent` compares the prompt with the former text.
- The chat sends the instruction and the model of the Review agent. The answer names the agent and its model through an `agent`
  frame, sent once before the first delta. Analysis and change generation keep their prompts and the project's model.
- Commands `agents_list`, `agents_set` (mode, instruction, model) and `agents_restore` (mode) are in `command_names`, the main
  window's capability, and the contract test. A refused value is `InvalidInput` and saves nothing.
- Trust boundary: the instruction goes into the provider's system prompt, so the window now chooses part of what the provider is
  asked. What the shell permits does not depend on it. An agent has no permission field, and a stored profile with an unknown
  field is ignored. The proposals of a complete answer still pass the same checks (blocks, paths, sensitive names) before they
  are recorded as pending.
- Known gaps, not changed in this round:
  - The Review chat records a complete answer's `noct-change` blocks as pending changes, as before. The task asked that Review
    only read and produce findings. `AGENTS.md` allows Review to record proposed changes as pending, so the two disagree; this
    round keeps the behavior and leaves the decision to the owner.
  - In Documentar only Markdown changes are listed. That is a filter of the interface. The shell does not refuse a
    non-Markdown change for the Docs mode when it is approved.
  - The Build and Docs agents are stored and editable, but nothing calls them yet.
- Checked: `noct-core/tests/agents.rs` (19 tests), `noct-app/tests/agents_flow.rs` (6), the chat flow tests (the default prompt,
  an instruction changing only the start, the agent's model), `noct-ui` native tests (the agent frame, the panel's rules),
  `capabilities.rs` (the contract list), `cargo test --workspace` (906 passed), wasm clippy, `trunk build --release`,
  `npm test` (37/37), `check:csp`, and `check:ui` (34 of 34, with the agents flow on the ready scenarios). Not checked: a real
  OpenRouter call with an agent's instruction or model, and the agent's name in the answer's header in a real window. The
  Chromium mock does not send the `agent` frame; the session test covers it.
