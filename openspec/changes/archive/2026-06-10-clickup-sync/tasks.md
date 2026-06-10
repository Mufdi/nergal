# Tasks — clickup-sync

> Migration numbers are **not pinned**: assign the next free number(s) after the
> current max (`014_env_shell_suggestions`) at implementation time, registering
> each in the `db.rs:132` array in order. See implementation.md § Migrations.

## 1. Auth + token storage

- [x] 1.1 Add `keyring = "3"` to `src-tauri/Cargo.toml` (deps-review: new dependency).
- [x] 1.2 `clickup/auth.rs`: `store_token` / `load_token` / `clear_token`. Primary = `keyring::Entry::new("cluihud", "clickup-token")`. Fallback = `~/.config/cluihud/clickup.toml`, created **atomically** with mode `0600` (`OpenOptions::mode(0o600).create_new(true)` — no write-then-chmod window) and a `token_on_disk` flag surfaced to the UI.
- [x] 1.3 Tauri commands `clickup_set_token`, `clickup_clear_token`, `clickup_validate_token`. Validate calls `GET /user`, returns `{ id, username, email }` (never the token). Header is `Authorization: <token>` (no `Bearer`).
- [x] 1.4 Guard/test: no error string or log line ever contains the token; reqwest header tracing off.

## 2. REST client

- [x] 2.1 `clickup/model.rs`: serde structs for `Space`, `Folder`, `List`, `Status`, `Task` (+ flat `parent`, `custom_fields`, `checklists`, `attachments`), `Comment`, `User`. Tolerant of unknown fields (`#[serde(default)]`).
- [x] 2.2 `clickup/client.rs`: `ClickUpClient` over the shared `reqwest::Client`. Read methods: `get_user`, `get_teams`, `get_spaces`, `get_folders`, `get_folderless_lists`, `get_lists` (List objects carry `statuses[]` inline — no separate per-List status call in the poll), `filter_team_tasks(team_id, {space_ids, page, subtasks:true, include_closed:false})` — **no assignee filter** (all-tasks scope, Decision 4), `get_task(task_id, include_subtasks)` (detail only), `get_task_comments(task_id)`, `get_list(list_id)` (on-demand detail / show-closed only).
- [x] 2.3 Rate-limit: on `429` honor `Retry-After` with bounded backoff. **Pagination terminates on the response `last_page` flag, never on row count** (the endpoint filters after the page slice).
- [x] 2.4 Tests over captured fixtures: parse without panic; `<100 rows but last_page=false` keeps paging; assignee/computed-field shapes.

## 3. Mirror schema (migration)

- [x] 3.1 New migration `0NN_clickup_mirror.sql` registered in `db.rs:132`. Tables:
  - `clickup_spaces(id TEXT PK, name TEXT NOT NULL, synced_at INTEGER NOT NULL)`
  - `clickup_folders(id TEXT PK, space_id TEXT NOT NULL REFERENCES clickup_spaces(id) ON DELETE CASCADE, name TEXT NOT NULL, hidden INTEGER NOT NULL DEFAULT 0, stale INTEGER NOT NULL DEFAULT 0)`
  - `clickup_lists(id TEXT PK, folder_id TEXT REFERENCES clickup_folders(id) ON DELETE CASCADE, space_id TEXT NOT NULL REFERENCES clickup_spaces(id) ON DELETE CASCADE, name TEXT NOT NULL, stale INTEGER NOT NULL DEFAULT 0)`
  - `clickup_statuses(id TEXT PK, list_id TEXT NOT NULL REFERENCES clickup_lists(id) ON DELETE CASCADE, status TEXT NOT NULL, color TEXT, orderindex INTEGER, type TEXT)`
  - `clickup_tasks(id TEXT PK, list_id TEXT NOT NULL REFERENCES clickup_lists(id) ON DELETE CASCADE, parent_id TEXT REFERENCES clickup_tasks(id) ON DELETE CASCADE, name TEXT NOT NULL, text_content TEXT, status_name TEXT, status_color TEXT, priority TEXT, assignees_json TEXT NOT NULL DEFAULT '[]', tags_json TEXT NOT NULL DEFAULT '[]', due_date INTEGER, start_date INTEGER, date_created INTEGER, date_updated INTEGER, url TEXT, archived INTEGER NOT NULL DEFAULT 0, stale INTEGER NOT NULL DEFAULT 0)` + indexes on `list_id`, `parent_id`, `stale`.
  - `clickup_custom_field_defs(id TEXT PK, scope_level TEXT, scope_id TEXT, name TEXT NOT NULL, type TEXT NOT NULL, type_config_json TEXT)` — `scope_*` nullable (derived from task payloads, Decision 2).
  - `clickup_task_custom_values(task_id TEXT NOT NULL REFERENCES clickup_tasks(id) ON DELETE CASCADE, field_id TEXT NOT NULL REFERENCES clickup_custom_field_defs(id) ON DELETE CASCADE, value_json TEXT, PRIMARY KEY(task_id, field_id))`
  - `clickup_checklists(id TEXT PK, task_id TEXT NOT NULL REFERENCES clickup_tasks(id) ON DELETE CASCADE, name TEXT, orderindex INTEGER)`
  - `clickup_checklist_items(id TEXT PK, checklist_id TEXT NOT NULL REFERENCES clickup_checklists(id) ON DELETE CASCADE, name TEXT, resolved INTEGER NOT NULL DEFAULT 0, orderindex INTEGER)`
  - `clickup_comments(id TEXT PK, task_id TEXT NOT NULL REFERENCES clickup_tasks(id) ON DELETE CASCADE, user_json TEXT, text TEXT, date INTEGER, resolved INTEGER NOT NULL DEFAULT 0, reply_count INTEGER NOT NULL DEFAULT 0)`
  - `clickup_attachments(id TEXT PK, task_id TEXT NOT NULL REFERENCES clickup_tasks(id) ON DELETE CASCADE, title TEXT, url TEXT, mimetype TEXT, size INTEGER, thumbnail_url TEXT)`
  - `clickup_sync_state(team_id TEXT PK, baseline_done INTEGER NOT NULL DEFAULT 0, last_full_sync INTEGER)` — drives the silent-first-sync gate.
- [x] 3.2 `clickup/mirror.rs`: typed upsert/read helpers; `read_tasks(filter)` returns the panel view-model (joined status/assignee) from the mirror only; upsert resets `stale=0` on reappearance.

## 4. Poller + atomic reconcile

- [x] 4.1 `clickup/poller.rs`: interval task (configurable cadence, default 45s). Per cycle: fetch hierarchy (statuses ride inline on each List object — no separate status call) + all-tasks per Space (paginated to exhaustion via `last_page`, `subtasks=true`), **then** commit in one SQLite transaction.
- [x] 4.2 Ordered upsert within the transaction: `spaces → folders → lists → statuses → tasks → subdata`. A task whose `list_id` is unknown to the fetched hierarchy synthesizes a placeholder list row (+ logs), never an FK panic.
- [x] 4.3 Absent-task reconcile: a complete per-Space fetch is authoritative — mirror tasks for that Space absent from the fetch are tombstoned (`stale=1`); covers closed/deleted/un-assigned. Hierarchy absent rows tombstone too. Reappearance resets `stale=0`.
- [x] 4.4 Subtask tree built solely from `parent_id` (from the flat `parent` in the all-tasks fetch). The nested `get_task` array is detail-only, never the reconcile tree source.
- [x] 4.5 **First-sync baseline**: while `clickup_sync_state.baseline_done=0`, seed silently — zero notifications. Set `baseline_done=1` after the first full sync. Only post-baseline new assignments notify. Coalesce a bulk re-assignment into one "N new tasks assigned" notification.
- [x] 4.6 Lazy heavy sub-data: comments/checklists fetched on detail-open or when `date_updated` advanced; not every poll.
- [x] 4.7 Emit `clickup:changed` after each committed reconcile. Stale GC past a retention window.
- [x] 4.8 Tests: first sync of a pre-assigned workspace → zero notifications; new task; moved task (list_id change); status change; **un-assignment → assignee array updated, task hidden by the assigned-to-me filter, NOT tombstoned**; deletion/close/moved-to-another-Space → tombstoned; reappearance → un-tombstoned; new post-baseline assignment → one notification; bulk assignment → coalesced; FK placeholder path; atomic-commit (no torn read).

## 5. Read-only panel (frontend)

- [x] 5.1 Register `clickup` in `src/stores/rightPanel.ts` (`TabType` + `PANEL_CATEGORY_MAP: "tool"` + singleton). Atoms fed by `clickup:changed`.
- [x] 5.2 TopBar icon + shortcut (verify `shortcuts.ts`; `event.code`). Entry shown only when a token is configured.
- [x] 5.3 Panel: persistent Space selector ("Todos" + each Space), group-by (status/list/assignee) via `Shift+←/→`, assigned-to-me filter (local, over the mirror). Show-closed toggle triggers an on-demand `include_closed=true` fetch for the current scope (closed tasks aren't in the default poll), rendering their actual closed status — not the tombstoned last-open snapshot. Focus zone `data-focus-zone="clickup"`, rows `data-nav-item` with inline metadata. No thumbnails in list rows.
- [x] 5.4 Floating detail module (`src/components/clickup/`): markdown description + comments rendered through the project's **sanitizing** markdown pipeline (raw HTML stripped); subtasks; checklists; attachment chips with **lazy, detail-open-only** image thumbnails (gated against non-image URLs); chip click opens the original in the browser. Read-only.
- [x] 5.5 Token settings UI: set/clear token, validation feedback (resolved user), `token_on_disk` disclosure, and the team picker when `get_teams` returns more than one.

## 6. Verification

- [x] 6.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [x] 6.2 `npx tsc --noEmit`
- [x] 6.3 Manual walk per proposal § Cómo verifico. (Round 1 2026-06-10 → 8 fixes; round 2 re-test passed all 7 checks.)
