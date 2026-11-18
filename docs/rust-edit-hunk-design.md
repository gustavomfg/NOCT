# changeControl.editHunk: design before the Rust port

Status: design only. No code is written for this command. It is the blocked item of P4 in [the status record](status.md),
and the first part of the owner's decision (3) of the second review round. The code waits for the owner's answers in
section 13.

Scope: `changeControl.editHunk` stores a reviewer's edited patch for one text hunk and marks the hunk `edited` or
`conflicted`. It is a review annotation. It changes no file in the workspace.

## 1. The command today

- Request: `{ conversationId, hunkId, finalPatch }`, strict. `finalPatch` is at most 200,000 characters
  (`shared/ipc/schemas.ts`, `changeControlHunkEditSchema`).
- Handler (`electron/ipc/registerChangeControlIpc.ts`): authorizes the conversation, loads the hunk, its change and its
  execution, and refuses unless the execution belongs to that conversation and workspace. It calls
  `ChangeHunkService.edit`, sends `changeControl.changed` with the execution and change set ids, and returns the hunk.
  The event is sent only after a successful edit.
- Service (`electron/change-control/ChangeHunkService.ts`, `edit`):
  1. the length is checked against 200,000 characters;
  2. the hunk exists and is `pending` or `edited`;
  3. the change exists in the execution and is `pending` or `edited`;
  4. `validatePatch` (section 4);
  5. the hunk and the change are read again; the edit is refused if the hunk's status, the hunk's `finalPatch` or the
     change's status changed meanwhile ("A revisão mudou durante a edição.");
  6. `finalPatch` is stored, with status `edited` when valid or `conflicted` when not, and `decisionAt` set to now.

## 2. What it writes

- Nothing in the workspace. The only write is the `change_hunks` row: `final_patch`, `status` and `decision_at`.
- The contract says so. `shared/changeControl.ts` states "Review annotation only: finalPatch and its decision never mutate
  workspace bytes". `docs/change-control.md` states that hunk edit, accept and reject "do not apply patches and do not
  decide the file", and that "There is no effective file-edit operation in this contract."
- The commands that write the user's files are the file decisions. Accept (`ChangeDecisionService`) verifies the AFTER
  bytes and keeps them. Reject restores the BEFORE bytes through `SnapshotRollbackService.rollbackPaths` and the native
  worker. The port of `editHunk` must not call the decision, the rollback or any other writer.

## 3. Path scope

- The request carries no path. The path is `change.relativePath`, stored when the change was captured.
- The checkpoint content is read only for that record, and only under the checkpoint store root (section 5).
- The command never reads the live file in the workspace. Validation uses the stored BEFORE bytes, so an external edit does
  not change a hunk's status here. The file decision makes the AFTER comparison later, as `docs/change-control.md` requires.
- Authorization order in the port follows the handler: conversation, then hunk, then change, then execution, then
  workspace. A refusal keeps the same text as the TypeScript.

## 4. Validation (`validatePatch`)

The TypeScript checks these, in order. Any failure gives status `conflicted`, not an error:

1. the hunk's diff exists and its `kind` is `text`;
2. the change set exists for the execution;
3. the BEFORE checkpoint has a file entry for `relativePath`, and its kind is `file`;
4. the BEFORE bytes are read;
5. when `change.beforeHash` is set, the SHA-256 of the bytes must equal it. When it is not set, no check is made;
6. `applyPatch` on the bytes, read as UTF-8, must not throw. Its result is discarded; only success or refusal matters.

## 5. Readers the port needs

### Checkpoint bytes

- TypeScript: `CheckpointService.readContent` calls `WorkspaceCheckpointStore.readContent`. An empty `contentPath` is an error
  ("O checkpoint não contém o conteúdo de <path>."). Otherwise the whole file is read, with no size check and no check that
  the path is inside the store.
- Rust today: `checkpoint_files.content_path` is read into the records (`crates/noct-db/src/repositories/checkpoints.rs`,
  `crates/noct-core/src/change_control.rs`), and `list_files` exists. There is no reader for the bytes and no store root.
- Port: a reader that resolves the content path under the checkpoint store root only, refuses a missing or escaping path with
  a fixed message, checks the size before reading, and returns the bytes. It runs asynchronously or in the blocking pool.

### Diff

- TypeScript: `ChangeDiffService.get` builds a `FileDiff` from the BEFORE and AFTER checkpoints. Its bounds are 1 MiB of
  text, 2,000 text lines, 4,000 diff lines and 200,000 diff characters.
- Rust today: there is no diff module in `crates/`.
- Port: needed for the `kind` check and for the `unifiedDiff` that a hunk's original patch is taken from. Without it,
  `changeControl.hunks` cannot produce the rows that `editHunk` reads.

### Hunk rows and capture

- Rust has the tables `change_sets`, `changes`, `change_hunks` and `change_decision_operations`, and a repository
  (`crates/noct-db/src/repositories/change_sets.rs`) with `get_hunk`, `list_hunks`, `create_hunk`, `update_hunk`,
  `get_change` and `update_change`.
- The capture that creates change sets, changes and hunks (`ChangeCaptureService` in TypeScript) is not ported. No Rust code
  calls `create_hunk`, so the Rust shell creates no hunks today. A port of `editHunk` alone could only be tested with rows
  that the test writes itself. See question (g).

## 6. applyPatch

The TypeScript function (`ChangeHunkService.ts`) does this:

1. Normalizes CRLF and lone CR in the BEFORE text to LF, splits on LF, and drops one trailing empty element.
2. Takes the patch lines after the first three (`slice(3)`). That assumes the `---`, `+++` and `@@` header of one hunk.
   Those three lines are not checked.
3. Skips empty lines and every line that starts with `@@`. The ranges in `@@` are not checked against the file or against
   `startLine` and `endLine`.
4. For a context line (` `) or a removal (`-`), the next source line must equal the rest of the line exactly. Otherwise it
   refuses: "O contexto do hunk não corresponde ao BEFORE." or "A remoção do hunk não corresponde ao BEFORE.". Both advance the
   source position. A removal appends nothing.
5. An addition (`+`) appends the rest of the line.
6. Any other prefix refuses with "Formato de patch não suportado.". That includes `\` ("\ No newline at end of file").
7. Appends the source lines not yet consumed. The joined result is discarded.

The port would inherit these behaviours, which the owner should decide on before code. None is changed silently:

- (i) The first three patch lines are skipped without a check. A patch without a header skips its first three lines in the
  check, so a wrong line there can still give `edited`.
- (ii) The `@@` ranges are ignored. A patch whose hunks sit elsewhere in the file is accepted when the sequence of context and
  removal lines matches.
- (iii) `\ No newline at end of file` is refused, so a genuine diff that contains that line becomes `conflicted`.
- (iv) CRLF is normalized to LF before matching, so a CRLF file is compared as LF.
- (v) The 200,000 limit is counted in JavaScript UTF-16 units. A port that counts Unicode scalar values accepts a different set
  of patches near the limit (question (d)).

Recommendation: port the function with exactly this behaviour and prove it with the same inputs in a differential. Change the
behaviour, if wanted, in a separate commit with its own tests and review.

## 7. Locking and order

- The Rust commands run inside `blocking` while the store lock is held. Reading up to 1 MiB of checkpoint text under that lock
  is bounded, but it blocks other store calls for the length of the read.
- Recommendation: two phases, as the TypeScript does with its re-read. Phase 1 holds no store lock: it reads the hunk, the
  change, the change set and the BEFORE bytes, and validates. Phase 2 takes the store lock: it reads the hunk and the change
  again, refuses if the hunk's status, its `finalPatch` or the change's status differ from phase 1, and then updates the row.
  See question (a).

## 8. Undo

- The TypeScript has no undo for a hunk edit and no reset.
- While a hunk is `pending` or `edited`, another edit replaces `finalPatch`. `originalPatch` keeps the original text, so a
  reviewer can restore it by sending the same text again, but the status then stays `edited` (or becomes `conflicted`), not
  `pending`.
- Once a hunk is accepted or rejected, `edit` is refused with "A anotação já possui uma decisão ou conflito.". A conflicted
  hunk is refused the same way.
- A file decision is final, except the build-revert path in `ChangeDecisionService`. That path rejects an accepted change
  when `revertingBuild` is set, and it writes the file. It is not part of this command.
- The port has no reset in this command. A reset, if the owner wants one, is a separate command with its own review, and it
  must not write files.

## 9. Refusals

The port keeps the TypeScript text for each refusal that returns a message:

| Condition | TypeScript message | Outcome |
| --- | --- | --- |
| `finalPatch` above the limit | O patch excede o limite seguro de edição. | error |
| hunk not found | Hunk não encontrado. | error |
| hunk decided, or conflicted | A anotação já possui uma decisão ou conflito. | error |
| change not in the execution | A mudança do hunk não pertence à execução. | error |
| change decided | A decisão do arquivo já foi encerrada. | error |
| hunk or change changed during the edit | A revisão mudou durante a edição. | error |
| patch does not match, or bytes do not match | none | status `conflicted` |

The Rust shell has no hunk error codes yet. They would follow the pattern of `UnknownArtifact` in
`crates/noct-app/src/error.rs`, with their own tests.

## 10. Event

The TypeScript sends `changeControl.changed` after a successful edit. The Rust shell has no change-control event yet, and the
P7 UI does not show change control. See question (e).

## 11. Tests before code

The TypeScript test (`tests/changeHunkService.test.ts`) covers one path: a hunk is created `pending`, a valid edit becomes
`edited`, and an incompatible patch becomes `conflicted`. It does not cover:

- a hash mismatch, and a change with no `beforeHash`;
- the refusals in section 9;
- the race check in section 1, step 5;
- the 200,000 limit;
- a non-text diff, and a checkpoint without content;
- a content path outside the store root;
- the user's file: its bytes must be the same before and after an edit (hash them both times);
- the event.

The Rust port needs a test for each of these, and a differential: the same inputs through the TypeScript and the Rust must give
the same status and the same refusal.

## 12. Preconditions

1. A checkpoint byte reader with the store root constraint (section 5).
2. A diff with the same `kind` and `unifiedDiff` as `ChangeDiffService`.
3. An `applyPatch` port with the behaviour decided in section 6.
4. The capture, or a decision on where hunk rows come from (question (g)).
5. Hunk error codes, with their tests.
6. Limit constants in `noct-core`: 200,000 characters for the patch, 1 MiB for checkpoint text, 2,000 lines. None of these
   values exists in the Rust code today.

## 13. Questions for the owner

- (a) Two-phase validation with a compare-and-set (section 7), or validation under the store lock?
- (b) A change with no `beforeHash` skips the hash check. Keep that, or refuse such changes?
- (c) `applyPatch`: port the current behaviour (recommended), or fix the divergences in section 6 now?
- (d) The patch limit: UTF-16 units as in TypeScript, or Unicode scalar values?
- (e) Send `changeControl.changed` now, or with the UI slice that shows change control?
- (f) Is a reset of a hunk edit wanted at all?
- (g) Until capture is ported, where do the Rust hunk rows come from?

## 14. Next step

No code until the owner answers (a) to (g) and the preconditions are scheduled. The first code change would be the checkpoint
byte reader, with its own tests, in its own commit.
