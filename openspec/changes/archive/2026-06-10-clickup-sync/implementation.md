# Implementation — clickup-sync

Detailed plan mapped against the current codebase. No code here — this guides Mode B.

## Codebase anchors (validated)

- **HTTP client**: `reqwest = "0.12"` (rustls-tls, json, stream) already in `Cargo.toml:74`. Reuse — no new HTTP dep. One `reqwest::Client` for the module.
- **Migrations**: `include_str!` array in `db.rs:132`, current max `014_env_shell_suggestions`. DB at `~/.config/cluihud/cluihud.db`, `PRAGMA foreign_keys=ON` set (`db.rs:108`) — FKs enforced (drives the ordered-upsert requirement).
- **Config dir**: `dirs::config_dir().join("cluihud")` (`db.rs:100-101`) — the token fallback file goes here.
- **Keyring**: NOT present — `keyring = "3"` is a new dep (deps-review). Linux → secret-service.
- **Notifications**: `tauri-plugin-notification` (`Cargo.toml:27`) — reuse for the assignment notification.
- **Right panel**: `src/stores/rightPanel.ts` — `TabType`, `PANEL_CATEGORY_MAP`, `SINGLETON_TYPES`, `activePanelViewMapAtom`. Add `clickup`.
- **Floating modules**: `src/components/floating/` (archived `2026-05-02-scratchpad-floating-panel`).
- **Sanitizing markdown**: reuse the project's existing untrusted-note markdown render path (the Obsidian note pipeline already renders untrusted bodies) — do NOT introduce raw-HTML passthrough for ClickUp content.
- **TopBar**: `src/components/layout/TopBar.tsx`.

## Migrations

Assign the **next free number** at build time (≥ `015`; `014` is taken). **Do not** reuse the `014`/`015` numbers the context-bridge specs mention — stale, parallel track. Append `0NN_clickup_mirror.sql` to the `db.rs:132` array **in order**. All tables new — no `ALTER TABLE sessions` here (that's `clickup-task-integration`). Includes `clickup_sync_state` (baseline gate). Note: migrations are forward-only (project convention) — accepted; no down-migration.

## Execution order

1. **Auth first** (`clickup/auth.rs` + commands). Add `keyring`; store/load/clear with the **atomic** `0600` fallback (`OpenOptions::new().mode(0o600).create_new(true)`); `token_on_disk` flag. `clickup_validate_token` → `GET /user`. Token-leak guard test.
2. **model.rs + client.rs**. Serde tolerant of unknown fields. Validate against fixtures (Space `901312445262`, task `86ahwtc67` with subtasks). Rate-limit (`429`+`Retry-After`) + **pagination by `last_page`** (not row count) here, once.
3. **Migration + mirror.rs**. Tables incl. `clickup_sync_state`; typed upsert/read; `read_tasks(filter)` joins status/assignee. Upsert resets `stale=0` on reappearance. Custom-field defs derived from task payloads (`scope_*` nullable).
4. **poller.rs** — the subtle core:
   - **All-tasks scope** per Space (no assignee filter); assigned-to-me is a panel filter.
   - **Fetch-all-then-commit-atomically**: gather hierarchy + cached statuses + paginated all-tasks, then one transaction, ordered `spaces→folders→lists→statuses→tasks→subdata`.
   - **FK-safe**: unknown `list_id` → synthesize a placeholder list + log, never panic.
   - **Absent-task tombstone**: complete Space fetch authoritative; absent mirror tasks → `stale=1`; reappearance → `stale=0`. Covers closed/deleted/un-assigned.
   - **Silent first sync**: gate notifications on `clickup_sync_state.baseline_done`; seed silently, then arm; coalesce bulk assignments.
   - **Statuses inline**: List objects in the hierarchy fetch carry `statuses[]` — no separate per-List status call; statuses are fresh every cycle for free.
   - **Lazy heavy sub-data** keyed on `date_updated`. Emit `clickup:changed`. Stale GC past a retention window.
5. **Frontend**. Register `clickup` view; atoms from `clickup:changed`; TopBar entry gated on token; panel (Space selector + group-by + assigned-to-me + show/hide-closed + focus zone + nav rows, no thumbnails in rows); floating detail (sanitized markdown + comments, lazy gated thumbnails, browser-open chips); token settings + team picker (>1 team).

## Reuse, don't reinvent

- Notification path: reuse `tauri-plugin-notification` (the unfocused-session notification call site), not a raw `notify-send` shell-out.
- Panel registration: follow the `git`/`browser` singleton precedent + the focus-zone/`data-nav-item` conventions in `docs/patterns.md` §5.
- Markdown: reuse the Obsidian-note sanitizing render path for untrusted ClickUp text.
- Keyboard: grep `src/stores/shortcuts.ts` for collisions before the open-panel shortcut + group-by `Shift+←/→`; `event.code`.

## Rate-limit budget (corrected)

Per cycle ≈ hierarchy + paginated all-tasks pages, × N Spaces. Statuses ride inline on the List objects (no separate per-List call), so the budget is a handful of requests per Space — well under the ceiling at the default cadence. Mitigate further with default cadence 45s, `Retry-After`, lazy sub-data. Validate the cadence against the 100 req/min ceiling for the actual workspace size at build before shipping the default.

## Edge cases

- **Empty mirror, populated workspace** (the real first-launch case): handled by the silent-baseline gate, NOT by "empty workspace".
- **Token user vs assignee ids**: resolve the token user's id once (`GET /user`); cache it for the assigned-to-me panel filter + post-baseline assignment detection.
- **Folderless lists**: payload carries a `folder` with `hidden:true` — store `hidden=1`; every list has a folder in the payload, never null.
- **`automatic_progress` computed field**: read-only display only; never a write target (matters at writeback).
- **Archived/closed tasks**: the default poll uses `include_closed=false`; a closed task leaves the fetch and tombstones via the absent-task rule. The panel's show-closed toggle does an **on-demand** `include_closed=true` fetch so it renders the actual closed status, not the tombstoned last-open snapshot.
- **Un-assignment is not absence**: under all-tasks scope an un-assigned task stays in the fetch with an updated `assignees` array — update the mirror and let the assigned-to-me filter hide it; do NOT tombstone it (tombstone is for closed/deleted/moved-to-another-Space only).
- **Multi-team**: `get_teams` may return several → user selection; never silently sync `teams[0]`.
- **Status rename drift**: denormalized `status_name` on tasks lags a rename until the task re-fetches — accepted brief window, noted.
- **Uninstall**: does not call `clear_token`; document so the user can clear the keyring entry manually.

## Out of scope (later changes)

- Any write to ClickUp → `clickup-writeback`.
- `sessions` columns + the 3 task→agent verbs + binding → `clickup-task-integration`.
- Exposing the bound task to the agent via MCP → folded into `cluihud-mcp-server` (see memory `project_clickup_integration`).
