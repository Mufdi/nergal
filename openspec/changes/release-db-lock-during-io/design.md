## Context

`SharedDb = Arc<Mutex<Database>>` serializes all DB access app-wide; rusqlite `Connection` is not `Sync`, so the mutex is load-bearing. Commands mixing DB reads with git/network work currently do both under one `db.lock()` guard. The three verified offenders: `delete_session` (`commands.rs:1007` guard; MOC git diff + `remove_worktree` inside), `create_pr` (`commands.rs:2592` guard; push + `gh pr create` inside), `git_ship` (`commands.rs:2883` guard; the whole ship pipeline inside).

## Goals / Non-Goals

**Goals:**
- No git or network I/O while the DB guard is held, in the three verified commands.
- Identical external behavior (same results, same error surfaces, same events).

**Non-Goals:**
- An async DB layer / connection pool; sweeping unverified commands (listed as a follow-up task); changing ship/PR semantics.

## Decisions

### D1: read → drop guard → slow work → (re-acquire for writes)

Each command becomes three phases:
1. **Extract** under a short guard: session row, `resolve_session_cwd`, branch/base, `workspace_repo_path`, obsidian config — all into owned locals (`String`/`PathBuf` clones; cheap).
2. **Work** with no guard: MOC build's git diff, `remove_worktree`, `worktree::create_pr`, `worktree::ship`.
3. **Finalize** under a fresh short guard where a write exists (`delete_session` only: `agents.forget_session` + `db.delete_session`; re-`find_session` first — if the row vanished meanwhile, return Ok idempotently).

**Alternatives considered:**
- *`spawn_blocking` around the slow work, keeping the guard held*: pointless — the guard is the contention, not the thread. Rejected.
- *Finer-grained locking inside `Database` (per-table mutexes)*: large blast radius for the same outcome on these paths. Rejected.
- *Async rewrite of `worktree::*`*: the functions are process-spawning wrappers; making them async doesn't release the std-mutex guard held across `.await` (and holding a std `MutexGuard` across `.await` is itself a bug pattern — `delete_session` today only avoids it because its guard-holding section is synchronous). Rejected.

### D2: `delete_session`'s Obsidian block — hoist the git work, keep DB reads short (option a)

`write_session_log_footer(&db, &cfg, &session_id)` is DB+file I/O (fast, keep under guard). `MocBuilder::build(&session_id, &cfg, &db)` interleaves DB reads with a worktree git diff — splitting it is the correct end state but touches the obsidian module's API. Phase this change: (a) now — hoist `remove_worktree` and re-scope the guard so it drops before worktree removal (the biggest, always-hit cost) and re-acquires for the final delete; (b) follow-up task (in this change's tasks.md, marked optional) — split `MocBuilder::build` into `collect(db) -> MocInputs` + `build(inputs)` so its git diff also runs guard-free. Rationale: MOC build only runs for obsidian-configured workspaces; worktree removal runs for every worktree session delete.

### D3: sync commands stay sync

`create_pr`/`git_ship` are sync `fn` commands — Tauri executes them on its blocking pool, so the *caller thread* is fine; the fix is purely guard scoping (phase 1/2 split). Converting to async + `spawn_blocking` adds nothing. `delete_session` stays async as-is.

### D4: TOCTOU on re-acquire

Between extract and finalize the world can change (user deletes the session mid-ship, renames branch). Rules: re-acquire reads re-validate existence and no-op gracefully (`delete_session`: missing row → Ok); pure-read commands (`create_pr`, `git_ship`) have no finalize phase — their extracted values describe the repo state the user acted on, same as today (today's guard doesn't prevent the repo itself changing under them anyway).

## Risks / Trade-offs

- [Concurrent command sees mid-teardown state (session row still present, worktree already gone)] → already possible today from external git actions; `remove_worktree` failure is logged-and-continue (commands.rs:1030-1044 precedent).
- [Double `delete_session` racing: both pass extract, both remove worktree] → `remove_worktree` on a missing path errors → logged, continues; final delete is idempotent SQL. Same outcome as today's re-invoke.
- [A future edit re-introduces slow work under a guard] → the new capability spec pins the rule; add a `## Follow-up` task to grep-audit `db.lock()` callers and list offenders in the PR for visibility.

## Open Questions

- None blocking. The optional `MocBuilder` split (D2b) can graduate to its own change if it grows.
