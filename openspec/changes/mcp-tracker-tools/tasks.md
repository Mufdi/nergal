## 1. Mirror read mappers

- [x] 1.1 Thin read fns over the ClickUp + Linear mirrors returning the unified row shape (source + shared columns + `fields` bag), reusing the `clickup::mirror::read_tasks` query shape + the Linear equivalent. Include `mirror_updated_at` per tracker.
- [x] 1.2 Read fns for PR status rollup + git status from the ship-flow backend the status bar uses (live local `git`/`gh`, no tracker API call; no persisted cache exists).

## 2. Tool defs + dispatch

- [x] 2.1 `list_tracker_tasks(filter?)` — unified, summary rows, `limit` (default 50, hard-clamped max), `date_updated DESC`; global mirror (no per-workspace binding exists in schema — see proposal Revision 1).
- [x] 2.2 `get_tracker_task(id)` — detail + subdata counts + budget-trimmed comments; mirror-only.
- [x] 2.3 `get_pr_status(session_id?)` — rollup; default session = caller.
- [x] 2.4 `get_git_status(session_id)` — branch/dirty/ahead.
- [x] 2.5 Register the 4 tools in the MCP tool list + dispatch arms; each carries a read-only contract in its description (tracker reads = global mirror; pr/git = identity-gated to caller).

## 3. Tests

- [x] 3.1 Dispatch/mapper unit tests where seam-able (the mirror read mappers over a seeded in-memory DB; the unified-row shape; pr/git identity-gate rejects cross-session); tracker reads mirror-only (no live tracker-call path reachable).

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`
- [x] 4.2 `npx tsc --noEmit` (unaffected).
- [ ] 4.3 Manual: an MCP client calls each tool; workspace-scoped rows returned; no write path; mirror_updated_at present.
