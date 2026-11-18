# Windows persistence check: the intermittent failure

## What failed

On commit `f2b8240` (the head of PR #23), the first attempt of the `Persistence reliability (windows-latest)` job failed four
TypeScript test files on 5 second timeouts, and once on an `ENOTEMPTY` while removing a SQLite temporary folder:

- `tests/buildRollbackBoundary.test.ts`
- `tests/checkpointService.test.ts`
- `tests/databaseDurability.test.ts`
- `tests/startupRecovery.test.ts`

The same commit passed when the job ran again. The same files had passed on `e8e5c23`. The failure is recorded in
[`rust-merge-plan.md`](rust-merge-plan.md). Its cause was not established then, and it is not established now.

These files test the Electron product. They are not part of the Rust shell.

## What is known and what is not

Known:

- The failure is intermittent. One attempt failed and the re-run of the same commit passed.
- It appeared on a timeout and a folder-removal error, not on an assertion about the data.

Not known:

- The mechanism. No failing log has been read against the code, and no run has reproduced the failure on demand.
- Whether the four files share a cause, or failed for four separate reasons under the same load.

Hypotheses to test. They are not conclusions:

1. A SQLite database file is still open when its temporary folder is removed. Windows refuses to delete a folder with an open
   file, which fits `ENOTEMPTY`.
2. Another process (an antivirus scan or the indexer) holds a file while a test waits for it, which fits the 5 second
   timeouts.

A failing run must confirm or reject each hypothesis. A passing batch lowers the estimate of the failure rate, but it does not
prove that the cause is gone.

## How the check runs

The job `windows-persistence` in [`rust-shell-validation.yml`](../.github/workflows/rust-shell-validation.yml) runs only when
someone starts the workflow by hand with `windows-persistence` set to `true`. It never runs on a push or a pull request.

- It runs `npm ci`, the same install as the package validation, and then `node scripts/run-tests.mjs` (the runner of `npm test`)
  on the four files above, 30 times in a row.
- It does not retry. A failing run is recorded with its exit code and its log, and the job fails if any run fails.
- Each run writes its log to `persistence-logs/`. The job uploads that folder as the artifact `windows-persistence-logs`, kept
  for 30 days. `persistence-logs/summary.txt` lists the exit code of each run.

To start it: GitHub, Actions, Rust shell validation, Run workflow, branch `migration/rust`, and `windows-persistence` set to
`true`.

## Status

- The check was added in this round. It has not been run at the time of writing.
- The cause is not established.
- No timeout or test was changed to make the failure go away. The four files are the same as in the failing run.

The check is done when a run of it shows a failure with a log that names the mechanism, and that mechanism is either fixed
in the product code or accepted with a written reason in this document.
