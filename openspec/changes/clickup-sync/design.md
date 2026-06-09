# Design — clickup-sync

## Context

ClickUp's domain model is `Workspace (team) → Space → Folder → List → Task → Subtask`. Tasks live in Lists; Lists may sit in a Folder or be folderless (ClickUp models folderless Lists under a synthetic `hidden:true` Folder in the payload). Statuses are **per-List** custom workflows (e.g. `"en revisión - dev (pr)"`), not a global enum. Custom fields are defined at any scope and carry arbitrary types. All of this is validated against the live workspace but **must be treated as runtime data**, never hardcoded (see proposal § Invariante).

This change is the data + read layer. Writes (`clickup-writeback`) and agent-loop integration (`clickup-task-integration`) build on the mirror this change establishes.

## Decision 1 — Local SQLite mirror vs live API calls per render

**Decision**: Maintain a local SQLite **mirror** of the ClickUp hierarchy + tasks; the panel reads exclusively from the mirror. A background poller refreshes it.

**Alternatives considered**:
- *Thin client (live call per render)*: simplest, no schema. Rejected — every render hits the network (slow), burns the rate limit, breaks offline, and makes the future bidirectional sync racy.
- *In-memory cache (no persistence)*: loses state across restarts, can't drive a "what changed while closed" diff, re-fetches the whole workspace each launch. Rejected.
- *SQLite mirror (chosen)*: instant render, offline-tolerant, gives the poller a base to diff, and is the substrate the optimistic-write reconcile in `clickup-writeback` needs. Matches the project's SQLite-first posture and the Linear local-first model.

## Decision 2 — Structure-agnostic schema (the generic tree, not the snapshot)

**Decision**: Model the full generic ClickUp tree with real FKs; vocabularies (statuses, custom-field defs) as **rows not enums**, so structural changes are absorbed as data. Schema (column types/FKs in tasks.md):
- `clickup_spaces`, `clickup_folders(…, hidden)`, `clickup_lists`, `clickup_statuses(list_id, …)`, `clickup_tasks(list_id, parent_id, …, date_updated, archived, stale)`, `clickup_custom_field_defs`, `clickup_task_custom_values`, `clickup_checklists`, `clickup_checklist_items`, `clickup_comments`, `clickup_attachments`.
- Folderless Lists reference a folder row flagged `hidden=1`; real Folders land in the same table `hidden=0`. Subtasks are tasks with non-null `parent_id`.

**Alternatives considered**:
- *Flat tables keyed to the current snapshot*: rejected — adding a Folder would need a migration (violates the invariante).
- *Statuses/custom-fields as enums*: rejected — per-List and user-editable.
- *Single JSON blob per task*: rejected for queryable fields (status/assignee/list/due drive SQL group-by/filter); JSON kept only for list-shaped sub-data, following the `tasks.blocked_by` precedent.

**Custom-field definitions sourcing** (review #12): task payloads carry `custom_fields[]` with `id`, `name`, `type`, `type_config`, `value` — but **not** the def's scope. Therefore `clickup_custom_field_defs` is **derived from task payloads** (id/name/type/type_config); `scope_level`/`scope_id` are **nullable best-effort**. The dedicated `GET /list/{id}/field` (accessible custom fields) is an optional later enhancement to populate scope precisely; not required for render-by-type. Documented as an accepted limitation.

## Decision 3 — Token storage: keyring with atomic config-file fallback

**Decision**: Store the Personal API token in the OS keyring via the `keyring` crate (secret-service on Linux). If unavailable, fall back to `~/.config/cluihud/clickup.toml`, **created atomically with mode `0600`** (`OpenOptions::new().mode(0o600).create_new(true)` — no write-then-chmod TOCTOU window, review #11), and surface to the user that the token is on disk.

**Alternatives considered**:
- *Plaintext config only*: a personal token grants full account access, readable by any same-uid process. Rejected as default.
- *zbus direct to secret-service*: more surface than the maintained `keyring` crate. Rejected.
- *keyring + atomic fallback (chosen)*: token never on disk in plaintext on the happy path; airtight `0600` on fallback.

**Token never leaks** (review #16): read into memory only inside the client; `Authorization: <token>` header is the v2 scheme (no `Bearer`). A guard/test SHALL assert no error string or log line contains the token, and reqwest header tracing is off.

## Decision 4 — Poll scope: ALL tasks per Space (not assignee-filtered)

**Decision** (resolves review #3): the poller fetches **all** tasks per Space (`GET /team/{team_id}/task?space_ids[]=…`, no assignee filter), not an assigned-to-me subset. Assigned-to-me is a **local panel filter** over the mirror.

**Rationale**: an assignee-filtered poll makes un-assignment undetectable (an un-assigned task simply vanishes from the fetch, indistinguishable from closed/deleted) and limits the panel to one view. An all-tasks fetch makes the complete-Space-fetch authoritative, which is what absent-task reconciliation (Decision 6) needs.

**Rate-limit reality** (corrects review #8 — the earlier "1-2 requests/cycle" claim was wrong): per cycle ≈ hierarchy (spaces + folders + folderless lists per Space) + paginated all-tasks pages, × N Spaces. **Statuses ride inline** with the List objects in the hierarchy responses (ClickUp returns `statuses[]` on each List), so there is **no separate `GET /list/{id}` per List** — statuses are fresh every cycle for free, and the freshness guarantee (a new status appears next sync) holds with no caching tension. Realistic budget is therefore a handful of requests per Space (hierarchy + task pages), well under the ceiling at the default cadence; still, the cadence (default 45s, configurable) must be validated against the 100 req/min ceiling for the actual workspace size at build, honoring `429` + `Retry-After`, with heavy sub-data fetched lazily (Decision 5). The `GET /list/{id}` and `get_task` detail calls exist only for on-demand detail, not the poll loop.

## Decision 5 — Lazy heavy sub-data, keyed on `date_updated`

Comments and checklists are fetched on detail-open or when a task's `date_updated` advanced since last stored — never every poll. The list/group/filter view needs only the light task fields, which the all-tasks fetch already returns.

## Decision 6 — Atomic reconcile: fetch-all-then-commit, ordered, FK-safe

**Decision** (resolves review #4, #5, #2): each poll cycle fetches everything it needs **first**, then commits the mirror in **one SQLite transaction** with a fixed upsert order — `spaces → folders → lists → statuses → tasks → subdata`. The panel's `read_tasks` never observes a torn mid-reconcile state. A poll that fails a network call mid-way commits nothing (the prior mirror stays intact).

- **FK ordering** (review #4): because `clickup_tasks.list_id` is `NOT NULL REFERENCES clickup_lists`, lists are upserted before tasks within the transaction. A task referencing a `list_id` absent from the just-fetched hierarchy (paginated past / created mid-fetch) is **not** an FK panic: synthesize a placeholder list row (id from the task's `list` field, `name` from payload, `hidden` folder) and log it; the next hierarchy fetch corrects it.
- **Absent-task semantics** (review #2): a **complete** per-Space all-tasks fetch (paginated to exhaustion) is authoritative for that Space. Tasks present in the mirror for that Space but absent from the fetch are tombstoned (`stale=1`) — this is how **closed** (`include_closed=false`), **deleted**, and **moved-to-another-Space** tasks stop showing. **Un-assignment is NOT a tombstone case** (round-2 #1): under all-tasks scope an un-assigned task stays present in the fetch with an updated `assignees` array; it is simply hidden by the local assigned-to-me filter, never tombstoned. A task reappearing in a later fetch is **un-tombstoned** (`stale=0`) on upsert (review #14).
- **Placeholder-list lifecycle** (round-2 #4): a synthesized placeholder list (FK-safety, below) is **exempt** from the hierarchy absent-row tombstone rule — it was never in a hierarchy fetch by definition. Its tasks stay visible under the placeholder until a real hierarchy fetch supplies the real List (which then takes over). If the real List never appears, the placeholder persists: better to show a task the user can see than hide it because the hierarchy API didn't return its container.
- **Hierarchy absent rows** likewise tombstone, not hard-delete mid-iteration; reappearance resets `stale=0`.
- **Pagination termination** (review #6): the filtered-tasks endpoint filters *after* the page slice, so a <100-row page does **not** mean the last page. Terminate on the response's `last_page` flag, never on row count. Fixture test: `<100 rows but last_page=false` must keep paging.
- **Stale GC** (review #14): tombstoned rows are retained for the panel's "show closed" affordance but GC'd past a retention window so the diff/queries don't scan unbounded dead rows.

## Decision 7 — First sync is silent (baseline before arming notifications)

**Decision** (resolves review #1): assignment notifications are armed only **after** a baseline exists. The first successful full sync (empty mirror → populated) seeds tasks **silently** — no `notify-send` for already-assigned tasks. A `clickup_sync_state` marker (or per-task baseline) records that the baseline is established; only assignments observed *after* the baseline fire notifications. Same gate after a DB reset. A bulk re-assignment is **coalesced** into one "N new tasks assigned" notification rather than N pings (review #18). Test: first sync of a workspace with pre-assigned tasks emits zero notifications.

## Decision 8 — Subtask tree from `parent_id`, single source of truth

**Decision** (resolves review #7): the all-tasks fetch runs with `subtasks=true`, which returns subtasks **flat**, each carrying `parent`. The mirror's subtask tree is built **solely** from `clickup_tasks.parent_id` populated from that flat `parent`. The nested `subtasks` array (returned by the per-task `get_task` detail call) is used only for the floating detail view's on-demand refresh, never as the reconcile's tree source. One source of truth for the tree; tested.

## Decision 9 — Multi-team handling

**Decision** (resolves review #13): `get_teams` may return several. The system resolves the team explicitly — if one team, use it; if more than one, let the user pick which team(s) to sync (a setting), defaulting to none-selected with a prompt rather than silently syncing `teams[0]`.

## Decision 10 — Read UI: list-in-panel + floating detail, sanitized rendering

**Decision**: the panel hosts the grouped, keyboard-navigable **list** (rich rows with inline metadata); opening a task raises a **floating module** with the full detail. One detail renderer.

**Alternatives considered**: in-panel master-detail (cramped in the narrow panel — rejected); both compact-peek + floating (two renderers, drift — rejected); floating full detail + inline row metadata (chosen, reuses `src/components/floating/`).

**Rendering security** (review #9, #10): a ClickUp task is multi-writer — description and comments are untrusted input rendered in a WebKitGTK webview. The renderer SHALL use a **sanitizing markdown pipeline that strips raw HTML/script** (reuse the project's existing markdown render path which already handles untrusted note bodies; never raw-HTML passthrough). Attachment **thumbnails** SHALL load lazily only on detail-open (not in list rows), gated so a non-image `thumbnail_url` can't auto-load; the chip click-through opens the external URL in the browser (an explicit user action, not an auto-fetch). No attachment binary is stored.

**Interaction** (per `docs/patterns.md`): focus zone `data-focus-zone="clickup"`; `Alt+↑/↓` list nav; groups collapse via `data-nav-expanded` + arrow-left/right; `Shift+←/→` cycles group-by; `Enter` opens detail; persistent header Space selector ("Todos" default) — not a transient modal. Bindings verified against `src/stores/shortcuts.ts` (`event.code`). Visual styling per `docs/design.md`; the Linear reference informs density, not a literal copy.

## Decision 11 — Greenfield Rust module layout

New `src-tauri/src/clickup/`: `mod.rs` (commands + wiring), `auth.rs` (keyring + atomic fallback), `client.rs` (reqwest REST, rate-limit, pagination), `mirror.rs` (mirror read/write, atomic reconcile), `poller.rs` (interval refresh + diff + baseline + events), `model.rs` (serde payload types). Mirrors the existing per-domain organization (`agents/`, `worktree.rs`).

## Risks

- **[Risk] Status denormalization drift** (review #15) — `clickup_tasks.status_name` can briefly lag a status rename until the task re-fetches. Accepted as a brief window; noted. Group-by reads the denormalized name for speed.
- **[Risk] Rate limit under large workspaces** — Decision 4 mitigations; cadence validated against the ceiling at build, status caching reduces the dominant cost.
- **[Risk] Keyring absent (headless/CI)** — atomic `0600` fallback with explicit disclosure; never silent.
- **[Risk] Down-migration / uninstall** (review #17) — migrations are forward-only (project convention; accepted). Uninstall does not call `clear_token`; documented so the user can clear the keyring entry manually.
- **[Risk] Migration number collision** with the parallel context-bridge track (which reserved 014/015, now stale since `014_env_shell_suggestions` exists) → do not pin absolute numbers; the implementer assigns the next free number at build time. In implementation.md.
