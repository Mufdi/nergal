# Design — db-lock-io-sweep

## Context

The global data layer is `SharedDb = Arc<Mutex<Database>>`. `Database` wraps a single `rusqlite::Connection`. Every consumer — Tauri commands, the ClickUp/Linear pollers (background threads), the hook socket server, the MCP server, startup recovery and app-close sweeps — serializes through this one mutex. Holding the guard across a blocking git/`gh`/network/fs operation therefore stalls *all* of them, not just the caller. The archived `release-db-lock-during-io` established the read → drop → work → re-acquire discipline for three commands; this change makes it universal across the 53 remaining offenders.

## Goals / Non-goals

- **Goal**: no `MutexGuard<Database>` alive across git/`gh` subprocess, network, or filesystem I/O, anywhere.
- **Goal**: behavior-identical — no frontend-visible signature/behavior change; same success/error outcomes.
- **Non-goal**: replacing the mutex with a pool or async DB. Separate architectural discussion.
- **Non-goal**: parallelizing the now-guard-free I/O. We drop the guard; we do not add concurrency.

## Three fix patterns

### Pattern A — Simple-drop (39 sites)

The function acquires the guard, reads owned values (cwd, branch, base) via `resolve_session_*`, then calls a `worktree::*` wrapper or a direct `Command::new(...)`. No DB write follows. Fix: scope the guard to a block that returns the owned values; the guard drops at the block's end, before the I/O.

```rust
// before
let db = db.lock().map_err(|e| e.to_string())?;
let cwd = resolve_session_cwd(&db, &session_id)?;
let branch = resolve_session_branch(&db, &session_id)?;
crate::worktree::push(&cwd, &branch).map_err(|e| e.to_string()) // guard still held

// after
let (cwd, branch) = {
    let db = db.lock().map_err(|e| e.to_string())?;
    (resolve_session_cwd(&db, &session_id)?, resolve_session_branch(&db, &session_id)?)
}; // guard dropped here
crate::worktree::push(&cwd, &branch).map_err(|e| e.to_string())
```

Applies to the read-only/idempotent git and gh wrappers. For fns that resolve several values (`get_session_git_info` reads only cwd then makes 5 git calls), the block returns just the cwd.

### Pattern B — Read → drop → I/O → re-acquire (interleaved, trailing DB write)

The function reads inputs, does git/network work, then writes back to the DB under the *same* guard. Fix: block-extract inputs, drop, do I/O, re-acquire a fresh guard for the write. The re-acquired write MUST re-validate row existence and no-op gracefully if the row vanished (TOCTOU: another actor could delete the session while the guard was released).

Sites: `git_rename_branch` (git.rs:309 — rename branch, then `db.update_worktree_branch`), `create_session` (workspaces_sessions.rs:240 — `create_worktree`, then `db.create_session`), `cleanup_merged_session` (:502 — remove_worktree + delete_branch, then `db.delete_session`), and the three spawn-worktree fns: `clickup_spawn_worktree_with_task` (clickup/mod.rs:880), `linear_spawn_worktree_with_issue` (linear/mod.rs:501), `build_worktree_session` (mcp/worktree_sessions.rs:867) — each does `create_worktree` then `create_session`, with a rollback path that calls `remove_worktree`.

TOCTOU note for the create/spawn trio: the write is a *create*, not an update — the risk is not a missing row but a duplicate/orphan if the worktree succeeded and the create fails. Preserve the existing rollback (`remove_worktree` on create-session error) but run it guard-free too.

### Pattern C — Gather → drop → loop-I/O (interleaved per-N loops with MOC/log I/O)

The function loops over N sessions, each iteration mixing DB reads (`MocBuilder::build(&db)`, `write_session_log_footer(&db)`) with filesystem writes (`atomic_write`, `BacklinkUpdater::propagate`). A single drop-before-loop is impossible while `MocBuilder::build` borrows `&Database` inside the loop. Fix depends on the **MocBuilder split** (below): gather every session's `MocInputs` (+ log-footer inputs) under one short guard, drop, then loop the render-and-write guard-free.

Sites: `delete_workspace` (workspaces_sessions.rs:187), `delete_session` (:345), `finalize_session_obsidian` (hooks/server.rs:499 — note one branch already `drop(guard)`s; unify both), `lib.rs` crash-recovery drain (:293) and `queue_close_markers` (:1026).

## The MocBuilder split (shared enabler)

`MocBuilder::build(session_id, cfg, &Database)` currently: (1) 3 quick DB reads — `find_session`, `get_visible_tasks`, `get_annotations` (lines 32-42); (2) pure file I/O — reads the session log file, `create_dir_all`, `atomic_write` (44-94). It does **not** shell out to git (the :230 comment is aspirational). The DB reads are already all at the top, so the split is clean:

```rust
// DB-read phase — borrows &Database, returns owned inputs (None if no MOC earned or no moc_dir)
pub fn gather(session_id: &str, cfg: &ResolvedObsidianConfig, db: &Database) -> Result<Option<MocInputs>>;
// write phase — no &Database; does the log-file reads + atomic_write
impl MocInputs { pub fn render_and_write(&self, cfg: &ResolvedObsidianConfig) -> Result<Option<PathBuf>>; }
```

`MocInputs` owns: `session` (id, agent_id, name, created_at), `tasks_done`, `decisions: Vec<String>`. The "nothing to show" early-return (line 62) stays — but note it currently also depends on `activity`/`files` extracted from the log file (file I/O). Decision: move the log-file extraction (`extract_session_activity/model/files`) into `render_and_write` (it needs no `&db`), and let the "earned a MOC" check run there — a session that gathers but renders nothing returns `Ok(None)` from `render_and_write`, same observable outcome as today. Keep `MocBuilder::build` as a thin `gather()?.map(render_and_write)` wrapper so any caller that is NOT in a hot loop (and already drops the guard correctly) is untouched. Only the Pattern-C callers switch to the two-phase form.

`write_session_log_footer(&db, …)`: verify whether it does file I/O under `&db`. If it only reads the DB then writes a file, give it the same gather/write split or hoist its file write; if it is pure DB, it can stay under the guard. Builder confirms by reading the fn before deciding.

## TOCTOU / correctness invariants

- Pattern B/C re-acquired writes re-validate existence and no-op if gone (spec scenario "row deleted between phases").
- Pattern C gather must snapshot everything the write phase needs — after the drop there is no `&db`. Missed field = compile error (the write phase has no DB handle), which is the desired failure mode.
- No behavior may become order-dependent on the drop: the git/fs work already ran after the reads in the old code; we only move *when the guard releases*, not the operation order.
- Async commands (`delete_session` is `async`): dropping the guard before `.await`-free blocking work is still the fix; do not hold the guard across any blocking call regardless of async-ness.

## Risks

- **MEDIUM**: the `MocBuilder` split touches a shared helper with 5 callers. Mitigation: keep `build()` as a wrapper so non-loop callers are behavior-identical; only migrate the Pattern-C loops.
- **LOW-MEDIUM**: Pattern B re-acquire introduces a window where the row could change. Mitigation: re-validate + no-op; the create/spawn trio keeps its existing rollback.
- **LOW**: 39 simple-drop sites are near-mechanical but voluminous; a missed site is a silent perf regression, not a correctness bug. Mitigation: the reviewer re-runs the enumeration grep to confirm zero `worktree::`/`Command::new`/`atomic_write` calls remain under a live guard.

## Revision 1 — iprev round 1 resolutions

Round 1 (opus) verified the plan against the code and surfaced 4 findings. Resolutions:

- **[BLOCKER] `lib.rs:293` marker drain does not fit single-gather Pattern C.** Confirmed: `lib.rs:293` passes a **locked guard** `&g` to `startup_recover(db: &Database)` → `drain_inline(&Database)` → `process_marker(path, &Database)` (post_session.rs:323/234/289). The loop is *driven by reading marker files* (`read_to_string` per marker) to discover the `session_id`, so DB inputs cannot be gathered before the fs I/O, and there is no `&SharedDb` in scope to re-acquire after a drop. This is a distinct **Pattern C′ (per-iteration lock)**: thread `&SharedDb` (the `Arc`, not a guard) through `startup_recover`/`drain_inline`/`process_marker`; per marker do `read_to_string` guard-free → short `lock()` for `find_session` + `MocBuilder::gather` → drop → `render_and_write` + `propagate` guard-free. `lib.rs:293` passes `&db` (the Arc), not `db.lock()`. **The detached `run()` path (post_session.rs:221) must also be updated** — see Revision 2 below; the round-1 "unaffected" claim was wrong.
- **[MAJOR] `create_session` `is_first` is a read-derived predicate consumed post-drop.** Confirmed at workspaces_sessions.rs:247-269: `is_first = session_count == 0` decides whether `create_worktree` runs; the slow work is **only** on the `!is_first && is_git_repo` branch. Resolution: **keep the whole `is_first`/no-worktree path atomic** (it has no slow work — no reason to drop), and only apply Pattern B (drop → `create_worktree` → re-acquire → `create_session`) on the worktree branch, where `is_first` is already `false` so re-deriving cannot flip it. This eliminates the duplicate-cwd race without a re-validate dance. General rule tightened: Pattern B re-validation covers **read-derived predicates**, not just row existence — but the cleanest fix is to not drop when the deciding read gates no slow work.
- **[MINOR] create/spawn trio loses the `worktree_dir.exists()` pre-check mutual exclusion.** Resolution refined in Revision 2 (the round-1 "git worktree add fails on the loser" premise was wrong — see below).
- **[MINOR] `finalize_session_obsidian` footer write is guarded in BOTH branches.** hooks/server.rs:513 `write_session_log_footer(&guard, …)` runs under the guard regardless of `runner_available` (not part of the MOC-block drop asymmetry). Resolution: finalize's gather must also capture footer inputs (`tasks_done` via `get_visible_tasks`; `cached_model` needs no `&db`) and run the footer write guard-free — task 2.3 spells this out.

Round-1 non-issues (confirmed, no change): MocBuilder split is clean (moc.rs:44-94 borrow no `&db`); `delete_session` async has no `.await` between drop and re-acquire; `delete_workspace`/`queue_close_markers` gather is complete; `cleanup_merged_session` Pattern B has no stale-value hazard; `merge_session` is correctly Pattern A.

## Revision 2 — iprev round 2 resolutions

Round 2 (opus) confirmed findings 2 (create_session) and 4 (finalize footer) RESOLVED, and surfaced one new MAJOR + one premise correction:

- **[MAJOR NEW-1] Pattern C′ signature change breaks the detached `run()` path.** Confirmed at post_session.rs:220-224: `run()` opens its own `Database::open()` → `&Database` and calls `process_marker(path, &db)`. Flipping `process_marker`/`drain_inline`/`startup_recover` to `&SharedDb` makes `run()` fail to compile (`expected &Arc<Mutex<Database>>, found &Database`). The round-1 "unaffected / do not touch" claim was false. **Resolution**: `run()` wraps its fresh connection — `let db = Arc::new(Mutex::new(Database::open()?));` — and passes `&db` (a `SharedDb`) to `drain`. The detached runner is single-threaded, so the extra `Mutex` is inert; the per-marker `lock()` inside `process_marker` still works. `run()` is the only other caller (grep-confirmed: callers are `lib.rs:294` startup_recover, `startup_recover`→`drain_inline`:328, `run`:223, `drain_inline`:236). Task 2b.4 added.
- **[MINOR finding-3 premise correction] `create_worktree` reuses on exists, it does not error.** Confirmed at worktree.rs:351-354: `if worktree_path.exists() { return Ok(worktree_path); }`. So the round-1 "re-check on error" fix never fires — the loser reuses the winner's worktree and both `create_session` calls could bind distinct `session_id`s to one worktree_dir. **Resolution**: for the spawn trio, after re-acquiring the guard and *before* `create_session`, check whether an existing session already targets this `worktree_path`; if so, return the existing-worktree error and do **not** `remove_worktree` (it belongs to the winner). Reachable only by same-task/same-slug double-invoke within one wall-clock second (slug embeds `ts`-seconds) on a human-gated path — MINOR, but the check is cheap and preserves today's guarantee. Task 3.4-3.6 note added.
- **[non-issue] Task 2b.3** (drain test): round 2 confirms `drain`'s signature is unchanged, so `drain_count_excludes_failed_markers` (post_session.rs:472) stays green with zero edits — the test uses a DB-free closure. 2b.3 is a no-op safety check, not real work.

## Verification

`cargo clippy --all-targets -- -D warnings` + `cargo test` + `cargo fmt --check`; `tsc --noEmit` (no frontend change expected). Plus a manual re-audit: re-run the offender enumeration and confirm the table is empty (every listed site now scopes its guard).
