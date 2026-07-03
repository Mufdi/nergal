## Why

The two largest backend files span every domain in the app: `src-tauri/src/commands.rs` (3979 lines, 120 `#[tauri::command]` fns, no section boundaries) and `src-tauri/src/db.rs` (2298 lines, a single `impl Database` starting at line 186 with ~85 public methods). Every feature touches them, so every PR collides in them; navigation, review, and per-domain testing all pay the monolith tax. This is the audit's highest-leverage structural finding, and it pairs with the pending `critical-path-test-coverage` change (a domain split makes per-module tests natural).

## What Changes

- **Split `commands.rs` into `commands/` by domain**, re-exported from `commands/mod.rs` so `lib.rs`'s `generate_handler![...]` list (`lib.rs:367`) and all public command names/signatures are byte-identical. Domains derived from the actual 120 command names (design.md D1): `workspaces_sessions`, `git`, `ship_pr`, `conflicts`, `plans`, `annotations`, `tasks_costs`, `obsidian`, `openspec`, `files`, `config_misc`.
- **Split `db.rs` into `db/` by domain** — `db/mod.rs` keeps `Database` (struct, `open`, migrations, connection); domain files add methods via multiple `impl Database` blocks (`db/workspaces.rs`, `db/sessions.rs`, `db/transcripts.rs`, `db/cross_session.rs`, `db/tasks_costs.rs`, `db/annotations.rs`, `db/scratchpad.rs`, `db/obsidian.rs`, `db/panels_misc.rs`).
- **Purely mechanical**: no logic edits, no signature changes, no visibility widening beyond what re-exports need. `cargo check` + the untouched test suite are the proof of equivalence.
- Shared private helpers currently local to each file (e.g. `resolve_session_cwd`, `resolve_session_base` in commands.rs) move to a `commands/shared.rs` (`pub(crate)`), keeping call sites unchanged.

## Capabilities

### New Capabilities

- `backend-module-structure`: backend command handlers and DB methods are organized by domain in `commands/` and `db/` module trees, with the Tauri command surface unchanged.

### Modified Capabilities

_None._

## Impact

- **Files**: `commands.rs` → ~12 files; `db.rs` → ~10 files (~22 new files, 2 deleted); `lib.rs` untouched except possibly the `mod` declarations (a directory module keeps `mod commands;` working as-is).
- **Git history**: file-level `git blame` breaks across the split — mitigated by a `.git-blame-ignore-revs` entry (design.md D4) and doing the split in dedicated commits with no logic changes mixed in.
- **Risk**: MEDIUM — big diff, zero intended behavior change; the compiler catches structural mistakes, tests catch the rest. The one operational risk is conflicting with in-flight branches (rebase pain) — coordinate timing (design.md D5, sequencing note re: pending changes touching commands.rs).
- **Out of scope**: fixing the `db.lock()`-across-I/O issues (separate pending change `release-db-lock-during-io` — land it FIRST to avoid rebasing its surgical edits over the split); adding tests (pending `critical-path-test-coverage`); any command rename or API change.
