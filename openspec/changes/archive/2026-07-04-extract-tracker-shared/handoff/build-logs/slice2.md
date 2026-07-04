# Slice 2 build log — closed-out marker trio

## Task 3.1 — caller grep (gate for the `ORDER BY` addition)

```
git grep -n "read_closed_out\|mark_closed_out\|unmark_closed_out" -- src-tauri src
```

Callers of `read_closed_out` on each side:

- ClickUp: `clickup/mod.rs:361` (`clickup_read_closed_out` Tauri command, thin passthrough) → `src/stores/clickup.ts:637` `invoke<string[]>("clickup_read_closed_out")` → `src/stores/clickup.ts:641` `store.set(clickupClosedOutAtom, closedOut)`. Consumer: `src/components/clickup/ClickUpPanel.tsx:1096` `useAtomValue(clickupClosedOutAtom).includes(task.id)` — `.includes()` membership check only.
- Linear: `linear/mod.rs:1145` (`linear_read_closed_out` command) → `src/stores/linear.ts:567` `invoke<string[]>("linear_read_closed_out")` → `src/stores/linear.ts:571` `store.set(linearClosedOutAtom, new Set(closedOutIds))` — wrapped in a `Set` immediately, order already discarded before any consumer sees it.

No consumer on either side iterates the array in-order or renders it as a list; both reduce it to a membership check (`Array.includes` / `Set`). The pre-existing ClickUp test (`closed_out_marker_round_trips_and_is_idempotent`) already `.sort()`ed the result before asserting, confirming the ClickUp side itself never depended on insertion order. **Confirmed order-insensitive on both trackers — adding `ORDER BY closed_at` to ClickUp's query is behavior-preserving.**

## Files changed

- `src-tauri/src/tracker_shared/closed_out.rs` (new, 110 lines) — `mark_closed_out`/`read_closed_out`/`unmark_closed_out` free fns parametrized by `table: &str, id_col: &str`; `table`/`id_col` interpolated into the SQL text via `format!` (WHY comment: sqlite can't bind identifiers as `?`-params, and every call site passes a compile-time literal, never user input). `read_closed_out` now does `ORDER BY closed_at` unconditionally (previously only Linear had it). Consolidated 2 tests (`closed_out_marker_round_trips_and_is_idempotent`, `unmark_closed_out_removes_marker`) against a throwaway in-memory test table, superseding the 4 near-duplicate tests previously split across `clickup/mirror.rs` and `linear/mirror.rs`.
- `src-tauri/src/tracker_shared/mod.rs` — added `pub mod closed_out;`.
- `src-tauri/src/clickup/mirror.rs` — `mark_closed_out`/`read_closed_out`/`unmark_closed_out` bodies replaced with one-line forwarders to `crate::tracker_shared::closed_out::*` (table `"clickup_closed_out"`, id-col `"task_id"`); public signatures unchanged (all 3 callers in `clickup/mod.rs` untouched). Test module: the 2 closed-out tests replaced with a one-line pointer comment to the shared test module.
- `src-tauri/src/linear/mirror.rs` — same shape: forwarders to the shared fns (table `"linear_closed_out"`, id-col `"issue_id"`); signatures unchanged (callers in `linear/mod.rs` + `linear/closure.rs` untouched). Test module: the 2 closed-out tests replaced with a pointer comment.

Only `clickup/mirror.rs`, `linear/mirror.rs`, and `tracker_shared/{mod,closed_out}.rs` were touched — `auth.rs`, `writeback.rs`, `integration.rs` untouched per scope.

## Shared fn signatures

```rust
pub fn mark_closed_out(conn: &Connection, table: &str, id_col: &str, id: &str, closed_at: i64) -> Result<()>
pub fn read_closed_out(conn: &Connection, table: &str, id_col: &str) -> Result<Vec<String>>
pub fn unmark_closed_out(conn: &Connection, table: &str, id_col: &str, id: &str) -> Result<()>
```

## Forwarders vs rewritten call sites

Used **thin forwarders** in both `clickup/mirror.rs` and `linear/mirror.rs` — the mirror-module public fn names/signatures (`mark_closed_out(conn, task_id, closed_at)` etc., no table param) are unchanged for their existing callers (`clickup/mod.rs`, `linear/mod.rs`, `linear/closure.rs`); only the fn bodies now delegate to `tracker_shared::closed_out::*` with the table/id-col literals baked in.

## Test consolidation

Removed 2 tests from `clickup/mirror.rs` (`closed_out_marker_round_trips_and_is_idempotent`, `unmark_closed_out_removes_marker`) and 2 from `linear/mirror.rs` (`closed_out_round_trip`, `unmark_closed_out_removes_marker`) — replaced by 2 consolidated tests in `tracker_shared::closed_out::tests` run against a generic test table (`closed_out_test`/`item_id`), asserting the same round-trip/idempotent-remark/unmark/no-op-on-absent-marker behavior both per-tracker suites asserted. No tracker-specific behavior remained to keep (unlike slice 1's `writeback.rs`, where ClickUp's real Additive split and each tracker's own `WRITE_TTL` derivation justified keeping some tests local) — the trio is byte-identical modulo table/column name, so nothing was kept in the mirror modules.

Net: −4 removed, +2 added = **−2 tests**.

## Gate outputs

**clippy** (`cd src-tauri && rtk proxy cargo clippy --all-targets -- -D warnings`):
```
    Checking nergal v0.4.1 (/home/felipe/Projects/cluihud/src-tauri)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.60s
```
EXIT=0, clean on the first run.

**test** (`cd src-tauri && cargo test`):
```
     Running unittests src/lib.rs (target/debug/deps/nergal-d9e74f6277acab17)
     Running unittests src/main.rs (target/debug/deps/nergal-1550b9654c910575)
     Running tests/agent_foundation_cost.rs (target/debug/deps/agent_foundation_cost-0f5dd531c37fc6e1)
     Running tests/agent_foundation_migration.rs (target/debug/deps/agent_foundation_migration-9f277a01597c431b)
   Doc-tests nergal
cargo test: 810 passed, 1 ignored (5 suites, 1.79s)
```
Baseline (post-slice-1) was 812; 812 − 4 + 2 = 810. Matches.

**fmt** (`cd src-tauri && cargo fmt --check`): failed on first run — rustfmt wrapped the `unmark_closed_out` forwarder call onto multiple lines in both `clickup/mirror.rs` and `linear/mirror.rs` (line length). Fixed with `cargo fmt`; re-check EXIT=0, clean diff.

**tsc** (`npx tsc --noEmit` from repo root): `TypeScript: No errors found`, EXIT=0 — unaffected, as expected (no frontend surface touched; the Tauri command signatures/JSON shapes are unchanged).

## Net LOC (`git diff --stat`, tracked files only)

```
 src-tauri/src/clickup/mirror.rs     | 65 +++++++++++----------------------
 src-tauri/src/linear/mirror.rs      | 72 +++++++++++--------------------------
 src-tauri/src/tracker_shared/mod.rs |  1 +
 3 files changed, 41 insertions(+), 97 deletions(-)
```
New file (untracked, not in the diff stat above): `tracker_shared/closed_out.rs` = 110 lines added.
Net for slice 2: 41 + 110 insertions − 97 deletions = **+54 lines**. (This slice's trio was already tiny per-tracker — ~30 lines each — so consolidating it doesn't shrink the repo the way slice 1's 292/295-line `writeback.rs` bodies did; the reduction target for the change as a whole rests mainly on slices 1 and 3.)

## Not cleanly preserved / flagged for reviewer

Nothing — the `implementation.md` Slice 3 facts held exactly as documented: the only real divergence (`ORDER BY` present on Linear, absent on ClickUp) was confirmed order-insensitive by the caller grep before unifying it, and the manual for-loop vs `.collect::<Result<_,_>>()` style difference collapsed naturally into one shared body. No blocker hit.
