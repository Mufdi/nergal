# Slice 1 build log — WritebackRegistry

## Files changed

- `src-tauri/src/tracker_shared/mod.rs` (new) — `pub mod writeback_registry;`.
- `src-tauri/src/tracker_shared/writeback_registry.rs` (new, 391 lines) — `WritebackRegistry<F: WriteFieldClass>`, `WriteFieldClass` trait, `FieldClass{Scalar,Additive}`, `WriteEntry<F>`, `EchoConflict` (neutral echo-conflict payload), `EchoCheckResult` (4 variants), `check_echo<F>`, plus a consolidated test module (11 tests).
- `src-tauri/src/lib.rs` — one line: `mod tracker_shared;` (alphabetical slot, between `mod terminal;` and `mod updater;`).
- `src-tauri/src/clickup/writeback.rs` — module doc rewritten; registry/`check_echo`/`EchoCheckResult` body removed, replaced by a `pub type WritebackRegistry = GenericWritebackRegistry<WriteField>` alias + `impl WriteFieldClass for WriteField` (real Scalar/Additive split preserved) + `impl Default` (constructs with `WRITE_TTL`). `WriteConflict` struct kept verbatim (per-tracker serde wire). Comment post-once model (`CommentOutcome`/`post_comment`/`verify_comment_landed`/`classify_comment_error`) untouched. Test module trimmed from 11 registry/echo tests to 4 ClickUp-specific ones (see below); all 9 comment-model tests untouched.
- `src-tauri/src/linear/writeback.rs` — same shape: module doc rewritten, registry/`check_echo` body removed, type alias + `impl WriteFieldClass` (all-`Scalar`) + `impl Default`. `WriteConflict` kept verbatim. Comment post-once model untouched. Test module trimmed from 11 registry/echo tests to 3 Linear-specific ones; 3 comment-model tests untouched.
- `src-tauri/src/clickup/poller.rs` — `run_echo_check`: `registry.entries_for_task(&task.id)` → `registry.entries_for(&task.id)`; `ScalarConflict(conflict)` arm now maps the neutral `EchoConflict` (`conflict.id/.field/.your_value/.remote_value`) into ClickUp's own `WriteConflict { task_id, ... }` before pushing. Two test call sites (`reg.entries_for_task(TASK_PARENT)` → `reg.entries_for(TASK_PARENT)`).
- `src-tauri/src/linear/mod.rs` — echo/conflict hook in the run loop: `reg.tracked_issue_ids()` → `reg.tracked_ids()`, `reg.entries_for_issue(issue_id)` → `reg.entries_for(issue_id)`; `ScalarConflict(c)` arm now maps `c` into `writeback::WriteConflict { issue_id: c.id, ... }` before emitting; added an `AdditiveDivergence => unreachable!(...)` arm (Linear's `WriteFieldClass` is all-`Scalar`, so `check_echo` can never construct it — exhaustiveness requirement of the shared 4-variant enum).

## Net LOC (`git diff --stat`, this slice's files only)

```
 src-tauri/src/clickup/poller.rs    |  16 +-
 src-tauri/src/clickup/writeback.rs | 292 +++-------------------------
 src-tauri/src/lib.rs               |   1 +
 src-tauri/src/linear/mod.rs        |  22 +-
 src-tauri/src/linear/writeback.rs  | 295 +++-------------------------
 5 files changed, 101 insertions(+), 525 deletions(-)
```
New files (untracked, not in the diff stat above): `tracker_shared/mod.rs` (7 lines) + `tracker_shared/writeback_registry.rs` (391 lines) = 398 lines added.
Net for slice 1: 101 + 398 insertions − 525 deletions = **−26 lines**. (Slice 1 is ~1/3 of the change's total ~600-800 LOC target across all 3 slices; the credential-store and closed-out slices carry more of the reduction.)

## WriteConflict-mapping approach (per implementation.md's iprev decision)

`check_echo` returns a neutral `EchoCheckResult::ScalarConflict(EchoConflict { id, field, your_value, remote_value })`. Each tracker's own emit site maps this to its own `Serialize`-derived `WriteConflict` struct (`task_id` for ClickUp, `issue_id` for Linear) immediately before pushing/emitting — zero wire-format change, `WriteConflict` itself untouched.

- ClickUp: `clickup/poller.rs` `run_echo_check`'s `ScalarConflict(conflict)` arm.
- Linear: `linear/mod.rs`'s inline echo loop (inside the poll-cycle closure) `ScalarConflict(c)` arm.

## Test consolidation

Baseline: 816 tests. Final: **812 tests** (5 suites, 1 ignored — matches expected math: −15 per-tracker registry/`check_echo` tests deleted [7 from ClickUp, 8 from Linear] + 11 new consolidated tests in `tracker_shared::writeback_registry::tests` = net −4).

Kept per-tracker (not mechanically duplicate — exercise the tracker's real `WriteFieldClass` impl or its own `WRITE_TTL` derivation):
- ClickUp: `ttl_is_at_least_two_poll_intervals`, `checklist_item_is_additive_class`, `additive_divergence_never_scalar_conflict`, `own_assignment_write_is_own_echo_not_conflict` (REGRESSION, real `Assignees` field).
- Linear: `ttl_is_at_least_two_poll_intervals`, `own_assignment_write_returns_own_echo_not_conflict` (REGRESSION, real `Assignee` field), `assignee_conflict_when_remote_supersedes` (adapted to assert on the neutral `EchoConflict`'s `.id`/`.field`/`.your_value`/`.remote_value` instead of the old `WriteConflict.issue_id`).

Consolidated into `tracker_shared::writeback_registry::tests` (11 tests, using a test-only `TestField{Scalar,Additive}` enum): `expired_entries_not_returned`, `fresh_entry_returned_and_clearable`, `api_failure_clears_provisional_record`, `record_without_clear_leaves_entry_present`, `tracked_ids_returns_live_entries`, `ttl_is_a_construction_parameter` (new — proves TTL is per-instance, not a shared const), `own_echo_when_server_matches_written`, `scalar_conflict_when_remote_supersedes`, `unrelated_when_server_matches_pre_write`, `additive_divergence_never_scalar_conflict`, `own_additive_write_is_own_echo_not_divergence`.

## Decisions / deviations from the literal task wording

- **Visibility bumped `pub(crate)` → `pub`** for `WritebackRegistry`, `WriteEntry`, `WriteFieldClass`, `FieldClass`, `EchoConflict`, `EchoCheckResult`, `check_echo` in `tracker_shared::writeback_registry`. The brief said "Shared items `pub(crate)`"; first clippy run failed with 12 `private_interfaces` errors because these types are threaded through pre-existing `pub async fn` Tauri commands (`clickup_set_custom_field`, `linear_set_issue_state`, `linear_execute_gated_write`, etc.) — those functions' `pub`-ness predates this slice and is out of scope to change. `mod tracker_shared;` itself stays a plain (non-`pub`) module in `lib.rs`, consistent with other internal modules (`mod commands;`, `mod db;`).
- **No per-tracker method-name wrappers.** Rather than keeping `entries_for_task`/`entries_for_issue`/`tracked_issue_ids` as thin forwarding wrappers, call sites (`clickup/poller.rs`, `linear/mod.rs`) now call the shared generic names directly (`entries_for`, `tracked_ids`) via the type alias. This avoids reintroducing per-tracker duplication for what is now literally the same struct. `record`/`clear_entry`/`purge_expired` needed no renaming (already generic-friendly names) — zero diff in the writeback *command* functions in `clickup/mod.rs`/`linear/mod.rs` (only the reconcile/run-loop echo-check sites changed).
- **`WriteEntry.task_id`/`.issue_id` → `.id`.** Confirmed via grep that no code outside each tracker's own `writeback.rs` read those fields directly (all other `.task_id`/`.issue_id` hits were on the unrelated `ClosureToken` struct in `closure.rs`), so the rename was safe.
- Ran both tracker migrations' edits before the first gate pass (rather than gating strictly after Linear alone, then again after ClickUp alone) since the shared module was written comprehensively for both shapes up front; a single combined gate pass turned out sufficient — no issue attributable to either tracker individually.

## Gate outputs

**clippy** (`cd src-tauri && rtk proxy cargo clippy --all-targets -- -D warnings`):
```
    Checking nergal v0.4.1 (/home/felipe/Projects/cluihud/src-tauri)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.84s
```
EXIT=0. (First run failed with 12 `private_interfaces` errors — see "Decisions" above; fixed by bumping visibility, re-run clean.)

**test** (`cd src-tauri && cargo test`):
```
     Running unittests src/lib.rs (target/debug/deps/nergal-d9e74f6277acab17)
     Running unittests src/main.rs (target/debug/deps/nergal-1550b9654c910575)
     Running tests/agent_foundation_cost.rs (target/debug/deps/agent_foundation_cost-0f5dd531c37fc6e1)
     Running tests/agent_foundation_migration.rs (target/debug/deps/agent_foundation_migration-9f277a01597c431b)
   Doc-tests nergal
cargo test: 812 passed, 1 ignored (5 suites, 1.75s)
```

**fmt** (`cd src-tauri && cargo fmt --check`): failed on first run (import-group ordering in `clickup/writeback.rs` + `linear/writeback.rs`); fixed with `cargo fmt`; re-check EXIT=0, clean diff.

**tsc** (`npx tsc --noEmit` from repo root): `TypeScript: No errors found`, EXIT=0 — unaffected, as expected (pure backend slice).

## Not cleanly preserved / flagged for reviewer

Nothing — all `implementation.md` facts for Slice 1 held exactly as documented (TTL construction-param, 4-variant `EchoCheckResult`, `WriteConflict` staying per-tracker, real Scalar/Additive split preserved for ClickUp only). No blocker hit.
