# linear-mirror-enhancements

## Why

The `linear-mirror` walk surfaced gaps once the panel was used against a real workspace (Red Ribbon, 172 issues): completed/canceled/duplicate issues were mirrored but hidden, the status glyphs didn't match Linear's, the group order wasn't Linear's, the default view was hardcoded to "my issues", the user couldn't reach their second Linear workspace (Personal API keys are workspace-scoped), and the issue detail lacked the Activity (history) feed Linear shows under the description. All of these were confirmed empirically against the live Linear API.

## What Changes

- **Faithful status glyphs** — a Linear-specific status icon: triage, backlog (dashed ring), unstarted (hollow), started (pie), completed (filled+check), canceled & **duplicate** (filled+✕). `duplicate` is a distinct Linear `StateType` (verified live), not a flavour of canceled.
- **Linear group order** — group state buckets in the order the user's workspace shows them: triage → started → unstarted → backlog → completed → canceled → duplicate, ties broken by workflow `position` (so "Pending" `position=-899` precedes "Done").
- **Completed/canceled/duplicate visible** — they are already mirrored; stop hiding them by default. "Show completed" defaults on; terminal classification includes `duplicate`.
- **Default view setting** — a per-tracker Settings select choosing the initial chip view among the views each panel already has: Linear = my issues / state / project / assignee / cycle; ClickUp = my tasks / status / list / assignee. No more hardcoded "my issues".
- **Multi-workspace (Linear)** — store one Personal API key per workspace, list them in Settings, pick the active one. The mirror is single-tenant: switching the active workspace wipes the Linear mirror and re-syncs. The existing single key migrates to become the first workspace.
- **Activity feed** — fetch an issue's `history` live on detail open (like attachments/relations, not mirrored) and render it under the description: created, state changes, assignee changes, label add/remove, cycle changes, priority changes.

## Impact

- **Affected capabilities**: `linear-mirror` (MODIFIED — glyphs, ordering, default view, multi-workspace, activity), `linear-agent-integration` (unaffected; activity is read-only detail).
- **Affected code**:
  - Rust: `linear/auth.rs` (per-workspace key storage), new `linear_workspaces` table + `linear_sync_state.active_org_id` (migration 025), `linear/mod.rs` (workspace add/list/activate/remove commands + history fetch in `linear_issue_detail`), `linear/client.rs` (history GraphQL + organization query), `linear/poller.rs` (use the active workspace's key; wipe-on-switch), `config.rs` (`linear_default_view`, `clickup_default_view`).
  - React: new `LinearStatusIcon`; `LinearPanel` (group order, show-completed default, default-view apply); `LinearTaskView` (Activity section); `stores/linear.ts` + `stores/clickup.ts` (default-view atoms, workspace atoms); `SettingsPanel` (default-view selects + Linear workspace manager).
- **Depends on**: `linear-mirror` (mirror + panel + auth). Independent of the agent-integration verbs.

## Build contract

### Cómo verifico
- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` · `npx tsc --noEmit` · `vite build`
- Walk (dev): glyphs match Linear per state; groups in the stated order; done/canceled/duplicate visible; default-view select changes the initial view; add the 2nd workspace key → switch → mirror re-syncs to it; open an issue → Activity feed under the description.

### Criterio de done
- All six items work against the real workspace; secrets never logged; switching workspaces never leaks the other's data (wipe-on-switch + epoch guard).
- No `unwrap()`/`expect()` outside tests; no TODO/FIXME; comments WHY-only.

### Estimated scope
- files_estimate: 16 · risk_tier: critical · tags: [migration, security, feature] · visibility: private
- spec_target: linear-mirror
