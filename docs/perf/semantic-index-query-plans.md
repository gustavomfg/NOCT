# Semantic index: query plans and statistics

This note records the measurements behind the query-plan section of
[ADR 0002](../adr/0002-envelope-provider-credentials.md). It covers the keyset listings of
`semantic_units` in the Rust migration (`crates/noct-db`): the embeddings listing and the file-unit listing.
The decision it supports is in the ADR.

## Method

- Engine: SQLite 3.54.1, the version the `noct-db` build reports (`BENCH sqlite` line of the benchmark).
- Databases are in memory. Release build, one run per configuration, on a Linux development workstation.
- Synthetic rows: 8-dimensional embeddings, random identifiers, 200 units per file.
- Page size 500, the listing's maximum. A **page walk** is one full walk of the selected workspace, page by page.
  A **single pass** is one query that reads the same rows without paging.
- Two states of the same data: **without statistics** (never analyzed) and **with statistics** (exact `ANALYZE`).
- Numbers are relative to this machine and this build. They are not budgets.

Reproduce, from the repository root:

```bash
cargo test --release -p noct-db --lib embedding_listings_at_fifty_and_two_hundred_thousand_rows -- --ignored --nocapture
cargo test --release -p noct-db --lib a_workspace_listing_alone_and_among_ten_workspaces -- --ignored --nocapture
cargo test --release -p noct-db --lib pragma_optimize -- --ignored --nocapture --test-threads=1
cargo test --release -p noct-db --test semantic_statistics measure_statistics_by_size -- --ignored --nocapture
```

Each line of output is prefixed with `BENCH`. The tests are in `crates/noct-db/src/repositories/semantic_index.rs`
(`benchmark_tests`).

## Query plans

| Listing | State | Plan |
|---|---|---|
| Embeddings, page and single pass | Without statistics | `SEARCH semantic_units USING INDEX idx_semantic_units_workspace_status (workspace=? AND status=?)`, then `USE TEMP B-TREE FOR ORDER BY` |
| Embeddings, one workspace of the whole table | With statistics | `SCAN semantic_units USING INDEX sqlite_autoindex_semantic_units_1`. The identifier cursor is a seek. |
| Embeddings, one workspace of ten | With statistics | `SEARCH semantic_units USING INDEX idx_semantic_units_workspace_status (workspace=? AND status=?)`, then `USE TEMP B-TREE FOR ORDER BY` |
| File units | Both states, one workspace of 20,000 rows | `SEARCH semantic_units USING INDEX idx_semantic_units_workspace_path (workspace=? AND relative_path=? AND start_line>?)`, then `USE TEMP B-TREE FOR LAST 2 TERMS OF ORDER BY` |

Without statistics, the identifier cursor `id > ?` is only a filter on the workspace/status range. Every page
sorts that range again, so the work grows faster than the page size.

## Results

### One workspace, 50,000 and 200,000 rows

| Measurement | Without statistics | With statistics |
|---|---|---|
| Page walk, 50,000 rows | 2,065 ms | 164 ms |
| Page walk, 200,000 rows | 27,766 ms | 662 ms |
| Growth, 50,000 to 200,000 rows (page walk) | 13.5× | 4.0× |
| Single pass, 50,000 rows | 147 ms | 113 ms |
| Single pass, 200,000 rows | 666 ms | 496 ms |
| Growth, single pass | 4.5× | 4.4× |

### Several workspaces: one selected, 20,000 rows

| Layout | Without statistics | With statistics |
|---|---|---|
| Alone, 20,000-row table, page walk | 430 ms | 57 ms |
| Among ten workspaces, 200,000-row table, page walk | 435 ms | 298 ms |
| Among ten, single pass | 60 ms | 59 ms |
| Ratio, among ten to alone (page walk) | 1.01× | 5.2× |

Without statistics the other workspaces cost the listing nothing measurable. With statistics the planner changes
plan for a workspace that is a tenth of the table: it uses the workspace/status index and a temporary sort. The
single pass is almost the same in both layouts (1.4× with statistics), so most of the difference is in the page
walk. This is a plan switch, not a scan of the other workspaces. Even so, the listing among ten workspaces is
faster with statistics (298 ms) than without them (435 ms).

### `ANALYZE` and `PRAGMA optimize`

| Run | Result |
|---|---|
| `ANALYZE`, 200,000 rows, ten workspaces | 161 ms |
| `ANALYZE`, 400,000 rows, ten workspaces | 324 ms |
| `PRAGMA optimize` on a 200,000-row table without statistics | 5–6 ms. Stores estimates: 1,409 rows per key. The exact value is 20,000 for ten workspaces. |
| Page walk after that `PRAGMA optimize`, ten workspaces | 431 ms |
| `PRAGMA optimize` on a 200,000-row table, one workspace | 5 ms. Stores 1,409 rows per key. The exact value is 200,000. |
| Page walk after that `PRAGMA optimize`, one workspace | 27,645 ms (single pass 695 ms) |
| Table doubled to 400,000 rows, `PRAGMA optimize` again | 0 ms. The statistics still count 200,000 rows. |
| Page walk after the table doubled, with the stale statistics | 439 ms |
| Page walk after `ANALYZE` at 400,000 rows | 429 ms |

Two more facts, from the `sqlite3` command-line tool (3.54.0), on the same kind of data:

- `PRAGMA optimize` does not overwrite exact statistics written by a recent `ANALYZE`.
- The estimates written by `PRAGMA optimize` were not refreshed when the table doubled (table above).

So `PRAGMA optimize` does not replace `ANALYZE` after a large indexing round. On a database that has never been
analyzed, it stores the estimates that keep the slow plan for one large workspace.

### Checking the statistics, and refreshing them

The rule in `crates/noct-db/src/statistics.rs` runs at open and at close. It refreshes the statistics only when the
table is large and the stored figures do not describe it. Measured with one workspace, release build, one run (the
`BENCH` lines below):

| Rows | `PRAGMA optimize` alone: stored rows per workspace | `COUNT(*)` | `COUNT(DISTINCT workspace)` | `ANALYZE`, exact |
|---|---|---|---|---|
| 10,000 | 1,409 (exact: 10,000) | 18 µs | 17 µs | 11.8 ms |
| 50,000 | 1,409 (exact: 50,000) | 36 µs | 17 µs | 45.5 ms |
| 200,000 | 1,409 (exact: 200,000) | 1.6 ms | 27 µs | 169.4 ms |

```
BENCH rows=10000 count=10000 workspaces=1 optimize_ms=7 sampled=[10000 1409 1409 1409] count_us=18 distinct_us=17 analyze_us=11789 exact=[10000 10000 10000 10000]
BENCH rows=50000 count=50000 workspaces=1 optimize_ms=8 sampled=[50000 1409 1409 1409] count_us=36 distinct_us=17 analyze_us=45499 exact=[50000 50000 50000 50000]
BENCH rows=200000 count=200000 workspaces=1 optimize_ms=20 sampled=[200000 1409 1409 1409] count_us=1563 distinct_us=27 analyze_us=169431 exact=[200000 200000 200000 200000]
```

Reproduce: `cargo test --release -p noct-db --test semantic_statistics measure_statistics_by_size -- --ignored --nocapture`.
The `sampled` and `exact` columns are the four figures of `sqlite_stat1` for `idx_semantic_units_workspace_status`,
which is on `(workspace, status, updated_at DESC)`: the table's rows, then the rows per prefix of one, two and three
columns. The rule reads the first two, the table's rows and the rows per workspace. The `optimize_ms` figure
is the `PRAGMA optimize` alone, on a table of that size without statistics. Figures change by a few percent between
runs; the table above is this run.

The table total stored by `PRAGMA optimize` is right at every size. The per-workspace figure is not. A check on the
total alone would accept these estimates, so the rule compares both figures with the table, within 10%. An
analysis at every open would cost 12 to 170 ms and write to the database each time. The check reads only, and it
costs 17 µs to 1.6 ms. That is why it is a check and not an analysis.

Detection, one test per state (`crates/noct-db/tests/semantic_statistics.rs`):

| State of the database | Result | Test |
|---|---|---|
| No statistics, 10,000 rows | Analyzed in the background | `opening_a_database_without_statistics_analyzes_it_in_the_background` |
| No statistics, 9,999 rows | Not analyzed (below the threshold) | `opening_a_small_database_without_statistics_starts_no_analysis` |
| Exact, fresh | Not analyzed | `opening_a_database_with_exact_statistics_starts_no_analysis` |
| Sampled by `PRAGMA optimize`, one workspace, 10,000 rows | Analyzed | `opening_a_database_with_sampled_statistics_analyzes_it` |
| Sampled by `PRAGMA optimize`, ten workspaces, 10,000 rows | Analyzed | `opening_a_database_sampled_across_ten_workspaces_analyzes_it` |
| Exact, uneven workspaces (9,000 and 3,000 rows) | Not analyzed | `opening_a_database_with_exact_uneven_workspaces_starts_no_analysis` |
| Exact, then the table doubled without close | Analyzed | `opening_a_database_whose_statistics_predate_a_doubling_analyzes_it` |

Failure handling, with the `ANALYZE` check denied by the authorizer hook (`rusqlite`, dev-only feature):

| State | Result | Test |
|---|---|---|
| A round of 10,000 units ends, analysis denied | Round recorded, one entry logged, no statistics stored | `a_failed_analysis_after_a_large_round_is_logged_and_the_round_still_updates` |
| Close of 10,000 rows, first analysis denied | Close succeeds, one entry logged, `PRAGMA optimize` stores estimates | `a_failed_statistics_step_at_close_still_runs_the_optimize_and_the_close_succeeds` |

When the rule is replaced by a total-only check (a temporary mutation, reverted), the two sampled rows fail and the
others pass. So the per-workspace comparison is what detects the sampled estimates.

The thresholds are judgements: 10,000 rows, and 10%. Neither was measured against a real workload.

## Criteria

These thresholds were set in the benchmark code (`PROBLEM_RATIO` and `NEIGHBOUR_RATIO` in `benchmark_tests`):

- **One workspace growing with the table:** a problem when the page walk at 200,000 rows is at least 8 times the
  page walk at 50,000 rows. Linear growth gives 4 and quadratic growth gives 16. Without statistics the growth is
  13.5 times (a problem). With statistics it is 4.0 times (not a problem).
- **Several workspaces:** a problem when one workspace's listing is at least 2 times slower in a ten-workspace
  table than alone. With statistics the ratio is 5.2 times, so the benchmark reports a problem. Without statistics
  it is 1.01 times.

## Index on `(workspace, status, id)`: evaluated, not in the schema

An experiment added this index to the benchmark, measured it, and then removed it. The code is not in the tree.
These numbers are from that run and were not measured again after the removal:

- With the index, the identifier cursor becomes a seek in both states (`SEARCH ... (workspace=? AND status=? AND id>?)`).
- Ten workspaces, 200,000 rows: the page walk took 69 to 74 ms in both states and both table sizes. Without the
  index it took 298 ms with statistics and 435 ms without.
- One workspace, 200,000 rows: the page walk took 483 ms, against about 24 s without the index and without statistics.

The index is deferred. It needs a versioned migration, and that migration must wait until the Electron build no
longer reads the database. See the ADR.

## Limits

- Synthetic data. One layout per table size. Other workspace counts, status mixes and table shapes can choose
  other plans, and they are not measured.
- One run per configuration. No repetitions and no variance figures.
- No real user database was measured.
- The `ANALYZE` cost is measured on this machine only.
- The 10,000-unit threshold for a large round is a judgement. It was not measured, and it is not a budget.
- A database closed by the TypeScript build should hold the sampled estimates of `PRAGMA optimize`, as the Rust
  tests reproduce on a Rust-written database. That was not measured on a database the TypeScript build closed.
- The check reads the workspace figure only. The figure of the (workspace, status) prefix is not checked.
- The 10% tolerance is a judgement. The check costs were measured once, on this machine.
- The failure tests inject the failure with an authorizer hook that denies `ANALYZE` checks. A lock held by another
  connection fails the statistics step and `PRAGMA optimize` together, so no test shows the optimize running after a
  lock. A checkpoint blocked by another connection is not tested either.
