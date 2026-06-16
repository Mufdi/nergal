# Implementation — linear-mirror

Plan mapped to the real codebase. No code here — files to touch, symbols to
reuse, execution order, edge cases. Read before executing tasks.md.

## Verified codebase facts (do not re-assume)

Anchored against files on disk 2026-06-16:

- **Migrations array**: `src-tauri/src/db.rs:189–211` is a `let migrations = [ include_str!(...), … ];` array; the last entry is `022_session_transcripts.sql` (db.rs:211), then `];` at db.rs:212. Versioning is positional — `version = (i + 1)` (db.rs:215). **Next free migration = `023`.** Append the `include_str!("../migrations/023_linear_mirror.sql")` line after line 211, inside the array.
- **`PRAGMA foreign_keys = ON`** is already established for the connection (per the summarizer FK work; db.rs sets it at connection open). FK cascades in 023 are live.
- **Config**: `src-tauri/src/config.rs:76` holds `clickup_poll_interval_secs: Option<u64>` with its default at config.rs:203 (`None`). Add `linear_poll_interval_secs: Option<u64>` and `linear_active_window_days: Option<u64>` as siblings (field + Default).
- **Backend-owned keys**: `src-tauri/src/commands.rs:103` — `const BACKEND_OWNED_CONFIG_KEYS: &[&str] = &["summary", "clickup_poll_interval_secs"];`. The merge at commands.rs:124 drops these from any frontend `save_config` payload. **Add `"linear_poll_interval_secs"` and `"linear_active_window_days"`** — without this they hit the same stale-config clobber the summarizer work fixed.
- **Tauri command registration**: `src-tauri/src/lib.rs:294` `tauri::generate_handler![…]`; ClickUp commands are registered starting `lib.rs:459` (`clickup_set_token`, …, `clickup_sync_status` at :462). Register the `linear_*` commands in the same block.
- **Right-panel tab registry**: `src/stores/rightPanel.ts:8` `TabType` union (currently ends `… | "clickup" | "clickup-task"`); `viewPanelLabel` map at rightPanel.ts:15–27; `PANEL_CATEGORY_MAP` at rightPanel.ts:30–43; `SINGLETON_TYPES` at rightPanel.ts:62 (`["tasks","git","browser","clickup"]`). Add `"linear"` (category `tool`, singleton) and `"linear-task"` (category `document`, not singleton) to **all four** spots.
- **Sanitizing markdown pipeline**: `MarkdownView` from `@/components/plan/MarkdownView` — `react-markdown` + `remark-gfm`, **no `rehype-raw`** (raw HTML escaped) with a `urlTransform` (link schemes filtered). Used by `ClickUpTaskView.tsx:26,1057,1146`. **Gap (iprev #7, anchored):** `MarkdownView` overrides `h1/h2/h3/p/ul/ol/li/code/pre/a` but **not `img`** → inline `![](url)` auto-loads. The Linear surface needs an `img`-override variant (click-to-load) — do NOT change ClickUp's usage. Verify `obsidianUrlTransform` keeps react-markdown's safe-protocol default.
- **URL-open guard**: `validate_url` in `src-tauri/src/browser.rs:64` allow-lists `http`/`https`/`about:blank` (rejects `file:`/`javascript:`/`data:`). Reuse it before opening any untrusted Linear attachment/link URL (iprev #16).
- **StatusIcon / PriorityIcon**: `src/components/clickup/StatusIcon.tsx` and `PriorityIcon.tsx`. They are Linear-style glyphs already (hollow ring / pie / filled disc for status; ascending bars for priority). **Reuse by import.** If importing from `components/clickup/` into `components/linear/` reads as wrong coupling, relocate both to `src/components/shared/` in a prep step and update the two ClickUp import sites — a mechanical move, no behavior change. Prefer the move (cleaner) unless it balloons the diff.
- **Dual-shell detail/tab controller**: ClickUp's floating `ClickUpTaskDetail.tsx` + document `ClickUpTaskTab.tsx` share a controller hook over a parametrized body (`docs/patterns.md` §13). Mirror the structure for `LinearTaskDetail` / `LinearTaskTab` (the latter only matters once `linear-task` tabs open — for read-only mirror, the floating detail is the primary surface; the tab shell can be a thin follow-on within this change or deferred to agent-integration. Build the floating detail first).
- **`keyring` crate**: already a dependency (added by ClickUp). No `Cargo.toml` change.
- **Notifications**: ClickUp uses the project's `notify-send` path for `clickup:assigned`. Reuse the same notification helper for `linear:assigned`.

## File plan

**Rust (new module `src-tauri/src/linear/`):**
- `mod.rs` — submodule decls (`auth`, `client`, `model`, `mirror`, `poller`) + the Tauri command surface (token verbs, sync status, read commands) + poller restart hooks. Mirror `clickup/mod.rs` structure.
- `auth.rs` — keyring `cluihud`/`linear-token` + atomic-0600 fallback + `AuthMode`/`authorization_header`. Port `clickup/auth.rs` almost verbatim; swap account name + add the `AuthMode` builder.
- `client.rs` — `LinearClient` (GraphQL POST, const query docs, `RATELIMITED` backoff, cursor pagination).
- `model.rs` — serde structs + `Connection<T>` generic.
- `mirror.rs` — upsert/read helpers over the 023 tables.
- `poller.rs` — interval reconcile, bounded scope, baseline, events.
- Register `mod linear;` in `lib.rs`.

**Migration:** `src-tauri/migrations/023_linear_mirror.sql` (schema in tasks.md §3.1).

**Frontend (new `src/components/linear/` + `src/stores/linear.ts`):**
- `linear.ts` (atoms: sync-status, issues, teams, labels-cache, UI prefs, listeners `setupLinearListeners` + `refreshLinearMirror`).
- `LinearPanel.tsx` (list + chip-strip group-by + team selector + filters), `LinearTaskDetail.tsx` (floating), `LinearTaskView.tsx` (shared body), optionally `LinearTaskTab.tsx`.
- Reuse `StatusIcon`/`PriorityIcon` (relocated to `shared/` or imported).
- TopBar icon in `src/components/layout/TopBar.tsx` (gated on key configured).
- Settings section in `src/components/settings/SettingsPanel.tsx` (key set/clear/validate + team multi-picker + disclosure). ClickUp's section (SettingsPanel.tsx ~744–912) is the template.
- RightPanel render branch in `src/components/layout/RightPanel.tsx` for `linear` / `linear-task`.

## Execution order

1. **Migration 023 + `mirror.rs` + db registration** — schema first; unit-test the table round-trips + FK cascade before anything reads them.
2. **`model.rs` + `client.rs`** — GraphQL structs + client; test against captured fixtures (parse, pagination, `RATELIMITED`). No network in tests.
3. **`auth.rs` + token commands** — keyring + `AuthMode`; `linear_validate_key` against `viewer`.
4. **`poller.rs` + reconcile** — bounded scope, atomic transaction, baseline, events. The reconcile tests (§4.8) are the correctness core — write them alongside.
5. **Config wiring** — `linear_poll_interval_secs` + `linear_active_window_days` + `BACKEND_OWNED_CONFIG_KEYS`.
6. **Frontend** — atoms + listeners → panel → floating detail → settings → TopBar. Verify `shortcuts.ts` for the open-panel binding collision before adding it.
7. **Verification** — clippy/test/fmt/tsc + manual walk.

## Edge cases + risk (post-iprev round 1)

- **Completeness-gated tombstoning (D4, iprev #1) is the load-bearing one.** Tombstone ONLY when every paginated branch for every selected team reached `hasNextPage==false` (`complete` flag). A rate-limit give-up on page 3 of 8 must commit upserts only and tombstone nothing — otherwise pages 4–8's live issues get false-tombstoned. Candidate = selected-team AND `updated_at > cycle_now - window` AND absent from the cycle's **global** fetched-id set AND not `synthetic` AND not `was_viewer_assigned`.
- **Age-out eviction ≠ tombstone (iprev #4).** Issues with `updated_at <= cycle_now - window` and not viewer's are *out of scope* — absence ≠ deletion. A dedicated eviction pass hard-deletes them (childless first); without it the mirror grows unbounded and shows 30-day-stale rows. Distinct from the tombstone-GC retention window.
- **Un-assignment via set 3, not `updatedAt` (iprev #3).** Don't rely on an assignee change bumping `updatedAt`. Re-fetch every `was_viewer_assigned=1` issue by id each cycle; clear the flag when the server assignee is no longer the viewer. Correct regardless of API semantics.
- **`parent_id` is a plain column (iprev #2/#5).** No self-FK: a child can precede its parent in `updatedAt`-desc order, and a parent can be out of scope → a hard FK aborts the txn. Build the tree in app, dangling parent → root. This also removes the cascade that would wipe live subtrees.
- **Viewer change resets baseline (iprev #8).** `linear_validate_key` compares the resolved `viewer_id` to the stored one; on change → `wipe_mirror()` + `baseline_done=0`. And never set `baseline_done=1` after a zero-team sync (iprev #14).
- **Scoped viewer-assigned union (iprev #9).** The assigned query is constrained to selected teams — a workspace-wide query drags in issues whose team/states were never fetched → perpetual placeholder churn.
- **`RATELIMITED` on HTTP 400 (iprev, anchored).** Parse the GraphQL body even on a 4xx; `status.is_success()` alone kills the poll on every rate-limit. Wait on the **exhausted** bucket's reset, clamped `[1s,60s]`. Hard complexity rejection is a separate error, not retried.
- **Null relations** — null assignee/project/cycle/parent are legitimate (Option columns), never trigger placeholder synthesis (only a non-null unknown `state_id`/`team_id` does, flagged `synthetic` and excluded from tombstoning — iprev #17).
- **Workspace labels (null `team_id`)** — upsert-only, never tombstoned by absence; GC a label def only when unreferenced (iprev #19). The `issue_labels` join is reconciled per issue (delete-absent + insert-present).
- **Complexity quota (iprev #11)** — nested issues query is heavy per row; keep `issues first ≤ 25` and inner `labels first ≤ 10`, assert the arithmetic in a comment. A hard over-cap rejection is handled distinctly from `RATELIMITED`.
- **Inline image auto-load + URL schemes (iprev #7/#16)** — Linear `MarkdownView` variant overrides `img` (click-to-load); attachment/link opens pass `validate_url` first.
- **StatusIcon relocation** — if moved to `shared/`, update the two ClickUp import sites in the same commit so the build never breaks midway.
