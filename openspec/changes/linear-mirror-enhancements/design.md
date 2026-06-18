# Design — linear-mirror-enhancements

## Context

All findings verified against the live Linear API (Red Ribbon workspace, key from keyring `cluihud`/`linear-token`):
- Distinct `StateType`s present: `triage, backlog, unstarted, started, completed, canceled, duplicate`. `duplicate` is its own type (both teams). "Pending" is a custom `completed` state with `position=-899` (precedes "Done" `position=3`).
- 172 issues, 159 assigned to the viewer; completed=81/canceled=9/duplicate=6 of those — i.e. the data is mirrored, just hidden by the `showCompleted=false` default.
- `organization { name }` = "Red Ribbon"; a Personal API key is workspace-scoped — it cannot see the user's other workspace. `viewer` exposes no cross-org list.
- `issue.history` is available with: `createdAt`, `actor { displayName }`, `fromState/toState { name }`, `fromAssignee/toAssignee { displayName }`, plus label/cycle/priority deltas.

## Decision 1 — a Linear-specific status icon, not the shared ClickUp one

`StatusIcon` (shared with ClickUp) only knows open/custom/done/closed. Linear needs `duplicate` and a *dashed* backlog ring and an ✕ (not check) for canceled/duplicate. Add `LinearStatusIcon({ stateType, color, fraction })` rendering directly from Linear's `StateType`:
- `triage` → hollow ring + a small centered dot (intake marker).
- `backlog` → **dashed** hollow ring (`strokeDasharray`).
- `unstarted` → hollow ring.
- `started` → ring + proportional pie (reuse the existing pie math).
- `completed` → filled disc + ✓.
- `canceled`, `duplicate` → filled disc + ✕ (gray when no color).
ClickUp's `StatusIcon` is left untouched (surgical). The Linear components swap `<StatusIcon type={linearStateToIconType(...)}>` → `<LinearStatusIcon stateType={...}>`.

## Decision 2 — group order = type rank then workflow position

`STATE_TYPE_RANK` becomes `triage=0, started=1, unstarted=2, backlog=3, completed=4, canceled=5, duplicate=6` (the user's stated order; `duplicate` added). Within a rank, sort by `statePosition` ascending so "Pending" (−899) precedes "Done" (3). `isTerminal` gains `duplicate`. `linearStateToIconType` is retained only where the icon swap isn't done, but the new `LinearStatusIcon` is the source of truth.

## Decision 3 — show completed by default

`linearShowCompletedAtom` defaults to `true` (the user wants done/canceled/duplicate visible). The existing toggle still hides them on demand. `isTerminal` includes `duplicate` so the toggle governs it consistently.

## Decision 4 — default-view setting maps to the existing chip views

The panel already models the chip views as `LinearView = "mine" | "state" | "project" | "assignee" | "cycle"` (and ClickUp has group-by `status|list|assignee` + a "mine" filter). Add config `linear_default_view: Option<String>` and `clickup_default_view: Option<String>` (validated against the known view sets; unknown/None → "mine" to preserve today's behavior). On panel mount, if the user hasn't already changed the view this session, apply the default: `"mine"` → assigned-to-me on (group-by state); a group-by value → assigned-to-me off + that group-by. Reversible UI; persisted via the standard config command.

## Decision 5 — multi-workspace: N stored keys, ONE active, wipe-on-switch

The mirror stays single-tenant — adding true multi-org simultaneous mirroring would fan out every table by org. Instead:

**Storage.** Keys move from a single keyring account to one per workspace: account `linear-token::<org_id>` (the bare `linear-token` is migrated on first run — validate it, resolve its `org_id`, re-store under the namespaced account, mark active). A new table tracks the set:
```sql
-- migration 025
CREATE TABLE linear_workspaces (
  org_id   TEXT PRIMARY KEY,
  name     TEXT NOT NULL,
  url_key  TEXT,
  added_at INTEGER NOT NULL
);
ALTER TABLE linear_sync_state ADD COLUMN active_org_id TEXT;
```
The keyring holds the secrets (never the DB); `linear_workspaces` holds only non-secret metadata for the Settings list. The 0600 fallback file becomes `linear-<org_id>.toml` when the keyring is unavailable.

**Active workspace.** `linear_sync_state.active_org_id` names the workspace the mirror reflects. The poller loads that workspace's key (`auth::load_key_for(org_id)`); with no active workspace it idles (`no_key`).

**Switching = wipe + re-sync.** `linear_set_active_workspace(org_id)`:
1. bump `key_generation` (the existing account-swap epoch — already wipes viewer/teams/baseline and discards any in-flight poll commit),
2. delete all mirrored issue/team/state/label/cycle/project rows (a `wipe_mirror` that leaves `linear_workspaces` + `linear_sync_state` intact),
3. set `active_org_id`, clear `selected_team_ids` (teams are workspace-specific),
4. kick a fresh poll.
The epoch guard already prevents a late in-flight response from the previous workspace committing into the new one — switching reuses it, so cross-workspace data bleed is structurally prevented (the central security property here).

**Adding.** `linear_add_workspace(key)`: validate the key (`viewer` + `organization`), resolve `org_id`+`name`+`url_key`, store the key under `linear-token::<org_id>`, upsert `linear_workspaces`; if it is the first, set it active. **Removing** deletes the keyring entry + the row; removing the active one clears `active_org_id` and wipes the mirror.

**Commands**: `linear_list_workspaces`, `linear_add_workspace`, `linear_remove_workspace`, `linear_set_active_workspace`. The legacy `linear_set_key`/`linear_clear_key` become thin shims over add/remove on the active org (kept so existing callers don't break) or are replaced — whichever keeps the Settings flow coherent.

**Alternatives considered**: (a) simultaneous multi-org mirror — rejected, every `linear_*` table would need an `org_id` column + the panel a workspace dimension; far more surface for no stated need (the user wants to *switch*, not see both at once). (b) Encode org in `key_generation` only — rejected, doesn't persist the workspace list for the Settings UI.

## Decision 6 — Activity feed: live fetch, not mirrored

Mirror the attachments/relations precedent: `linear_issue_detail` already does a live fetch on detail open. Extend it to also pull `issue.history` (capped, most-recent N) and return an `activity: Vec<LinearActivityEntry>`. Render an **Activity** section under the description (above the existing sub-issues/attachments), each entry a one-liner: actor + verb + relative time, e.g. "mufdidev created the issue", "changed status Todo → Done", "added label Improvement", "removed from Cycle 19". No persistence, no schema. History entries are normalized in Rust into a small tagged enum so the frontend renders without re-deriving Linear's raw shape.

**Activity entry model** (serialized):
```
{ id, createdAt, actor: Option<String>,
  kind: "created" | "state" | "assignee" | "label" | "cycle" | "priority" | "other",
  detail: { from?, to?, added?: [..], removed?: [..] } }
```
Label/cycle ids are resolved to names from the history payload where Linear includes them; otherwise shown by the raw value Linear returns (no extra round-trips).

## Risks
- **[Risk] Secret leakage across workspaces** → wipe-on-switch + `key_generation` epoch guard (existing) + per-org keyring accounts. The epoch guard is the structural backstop; tests cover "switch discards an in-flight commit".
- **[Risk] Migration of the existing single key** → on first run after upgrade, if `linear-token` exists and no `linear_workspaces` row does, validate+resolve+re-store namespaced, mark active; idempotent (a second run finds the namespaced key and skips).
- **[Risk] Keyring unavailable** → per-org 0600 fallback files, same atomic-create pattern as today.
- **[Risk] Huge completed set in the panel** → groups render lazily/collapsible already; showing completed by default is the user's explicit ask. Sort caps the per-group cost.
- **[Risk] History volume** → cap to most-recent N (e.g. 30) with a "older activity omitted" note, mirroring the comment attrition stance.
