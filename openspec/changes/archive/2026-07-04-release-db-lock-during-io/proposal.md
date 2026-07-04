## Why

Several Tauri commands hold the global `Mutex<Database>` guard across seconds-long git/network work, freezing every other command that touches the DB (session list, panels, hooks persistence) for the duration. Verified instances:

- `delete_session` (`src-tauri/src/commands.rs:986-1048`, async): takes the guard at 1007, then runs the Obsidian MOC build (`MocBuilder::build` — git diff over the worktree) and `worktree::remove_worktree` (shells out to git) before the final `db.delete_session`.
- `create_pr` (`commands.rs:2586-2620`, sync): guard at 2592, then `list_branches` and `worktree::create_pr` (network push + `gh pr create`) with the guard held.
- `git_ship` (`commands.rs:2872-2920+`, sync): guard at 2883, then the full `worktree::ship` pipeline (commit, push, PR create, optional auto-merge) with the guard held.

On a slow disk or network these hold the data layer hostage for seconds; every `db.lock()` caller blocks (and async callers block a runtime thread).

## What Changes

- **Restructure the three commands to a read → drop → work → re-acquire shape**: extract everything the git/network phase needs from the DB up front (session row, cwd, branch, base, repo path, obsidian config) into owned values, drop the guard (scope block or explicit `drop`), run the blocking git/network work, then re-acquire briefly only for the final writes (`db.delete_session`, none for `create_pr`/`git_ship`).
- **Wrap the blocking phases of the two sync commands in `spawn_blocking`** where they become async (or leave them sync — Tauri already runs sync commands off the main thread; the essential fix is dropping the guard, and design.md keeps the async/sync choice per command).
- **Guard-scope note for `delete_session`'s Obsidian block**: `write_session_log_footer(&db, …)` and `MocBuilder::build(…, &db)` take `&Database` — they need the guard, but the git-diff part inside `MocBuilder::build` is the slow piece; design.md decides between (a) accepting a short DB read inside and hoisting only `remove_worktree`, or (b) splitting `MocBuilder::build`'s DB reads from its git work (bigger, follow-up-worthy).

## Capabilities

### New Capabilities

- `db-lock-discipline`: the global database mutex is never held across git or network I/O; commands extract what they need, release, then perform the slow work.

### Modified Capabilities

_None._

## Impact

- **`src-tauri/src/commands.rs`**: `delete_session`, `create_pr`, `git_ship` restructured; no signature or behavior changes visible to the frontend.
- **Risk**: MEDIUM — reordering around the guard can introduce TOCTOU (session renamed/deleted between read and write phases); mitigations in design.md (re-validate on re-acquire where a write follows).
- **Out of scope**: auditing every other `db.lock()` caller (a follow-up sweep task lists candidates but only these three verified offenders change now); replacing the `Mutex<Database>` with a pool/async DB layer (architectural, separate discussion).
