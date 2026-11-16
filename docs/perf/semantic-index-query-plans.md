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
