# Implementation Plan: split-god-modules

> Grounded in current codebase, symbols verified 2026-07-02. Behaviour (not just symbol existence) verified for the load-bearing claims below.

## Verified codebase facts (do not re-assume)

- `src-tauri/src/commands.rs` is 3979 lines with exactly **120** `#[tauri::command]` attributes; `src-tauri/src/db.rs` is 2298 lines with a single `impl Database` beginning at `db.rs:186`.
- `lib.rs:367` is the `generate_handler![` list; it references commands as `commands::<name>` — a `commands/mod.rs` with `pub use` re-exports keeps every path valid with **zero lib.rs edits** (directory module replaces file module transparently).
- `db.rs:206` `pub fn open()` owns connection setup (`PRAGMA foreign_keys=ON` at 199/216) and the migration runner; these stay in `db/mod.rs`.
- Cross-domain private helpers exist in commands.rs (verified: `resolve_session_cwd` — used by `create_pr` at commands.rs:2606 and `git_ship` at 2884 among others; `resolve_session_base` at 2624) — they need `pub(crate)` homes in `commands/shared.rs`.
- 98 `db.lock()` call sites in commands.rs — untouched by this change (three of them are being restructured by the pending `release-db-lock-during-io` change; see sequencing).
- The full 120-command inventory and its domain clustering are recorded in design.md D1 (derived from the live file, not assumed).
- Rust permits multiple inherent `impl Database` blocks across files of the same crate — no trait needed for the db split.

## Execution order

1. **Gate**: confirm `release-db-lock-during-io`, `remove-dead-askuser-slice`, and `session-child-fk-cascade` have landed or are explicitly rebased-after (design D5).
2. `db.rs` → `db/` (smaller, macro-free): create `db/mod.rs` (struct/open/migrations/shared row mappers), then one commit per domain file extraction; `cargo check && cargo test` between commits.
3. `commands.rs` → `commands/`: `commands/shared.rs` first (cross-domain helpers), then one domain file per commit in design D1's table order; `cargo check` between commits.
4. Create `.git-blame-ignore-revs` listing every split commit; note it in README or CONTRIBUTING blurb (one line).
5. Full check + manual smoke.

## Plan

- **`db/mod.rs`**: `pub struct Database`, `open()`, migration list + runner, `conn()` accessor, shared mapping helpers. Re-export nothing (methods are inherent — callers keep `db.method()`).
- **`db/<domain>.rs`** (9 files): `use super::*;` + one `impl Database { … }` block each; move method bodies verbatim.
- **`commands/mod.rs`**: `mod` decls + `pub use self::<file>::*;` per domain file.
- **`commands/<domain>.rs`** (11 files + `shared.rs`): move command fns verbatim, hoist each file's `use` list minimally (compiler-guided).
- **Membership judgment calls**: `merge_session`/`cleanup_merged_session` → `workspaces_sessions` (session lifecycle, not git plumbing); `get_transcript` → `tasks_costs` (session data reads); deviations documented in PR.

## Per-phase risk

- [db split: a method relies on a private helper staying in mod.rs] → compiler error; keep helpers `pub(super)` in `db/mod.rs`.
- [commands split: attribute macros (`#[tauri::command]`) interact with module path in `generate_handler!`] → the re-export pattern is the documented Tauri idiom; verify with `cargo check` after the FIRST domain extraction before proceeding to the rest.
- [merge conflicts with concurrent work] → execute in a quiet window (D5), whole split in one focused PR of move-only commits.
- [accidental logic edits smuggled into moves] → review each commit with `git diff --color-moved=dimmed-zebra`; CI Full check (if `ci-quality-gates` has landed) or local Full check otherwise.

## Verification

- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit` (should be trivially green — no frontend change)
- Manual smoke: launch dev app; exercise one command per new domain file (open a session, stage a file, view a PR, open a plan, annotate, open obsidian note, browse files, save config).
