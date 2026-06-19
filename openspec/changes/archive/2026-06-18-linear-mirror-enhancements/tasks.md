# Tasks — linear-mirror-enhancements

> Built in phases, each gated (clippy/test/fmt/tsc/vite) + committed. Stays in dev.

## 1. Glyphs + ordering + show-completed (Phase A)

- [x] 1.1 New `src/components/linear/LinearStatusIcon.tsx` rendering by Linear `stateType`: triage (ring+dot), backlog (dashed ring), unstarted (hollow ring), started (ring+pie via the existing pie math), completed (filled+✓), canceled/duplicate (filled+✕). Color from the state color, gray fallback.
- [x] 1.2 Swap `<StatusIcon type={linearStateToIconType(...)}>` → `<LinearStatusIcon stateType={...}>` in `LinearPanel`, `LinearTaskView`, `LinearTaskDetail` (group headers, rows, detail title). Leave ClickUp's `StatusIcon` untouched.
- [x] 1.3 `STATE_TYPE_RANK` → `triage=0, started=1, unstarted=2, backlog=3, completed=4, canceled=5, duplicate=6`; `isTerminal` includes `duplicate`. Verify within-rank sort is `statePosition` asc (Pending before Done).
- [x] 1.4 `linearShowCompletedAtom` defaults to `true`.

## 2. Default-view setting (Phase B)

- [x] 2.1 `config.rs`: `linear_default_view: Option<String>` + `clickup_default_view: Option<String>` (None default). Surface via the existing `get_config`/`save_config` (frontend-owned config, not backend-only).
- [x] 2.2 `stores/linear.ts` + `stores/clickup.ts`: on first panel mount of a session, if the view hasn't been user-changed this session, apply the configured default (`"mine"` → assigned-on + group state; group-by value → assigned-off + that group-by). Validate against the known view set; unknown/None → "mine".
- [x] 2.3 `SettingsPanel`: a "Default view" select in the ClickUp section and the Linear section, options = that tracker's chip views. Persist via `save_config`.

## 3. Activity feed (Phase C)

- [x] 3.1 `linear/client.rs`: extend the detail query with `history(first: N)` (createdAt, actor, from/toState, from/toAssignee, label/cycle/priority deltas); add a `LinearActivityEntry` normalizer (tagged kind + detail) in `linear/model.rs` or `mod.rs`.
- [x] 3.2 `linear/mod.rs`: `linear_issue_detail` returns `activity: Vec<LinearActivityEntry>` (cap N, most-recent-first, "older omitted" note when truncated). No mirror, no schema.
- [x] 3.3 `stores/linear.ts`: extend `LinearIssueDetail` with `activity`. `LinearTaskView`: an **Activity** section under the description (one line per entry: actor + verb + relative time). Unit-test the normalizer (Rust) over a fixture history payload.

## 4. Multi-workspace (Phase D)

- [x] 4.1 Migration `025_linear_workspaces.sql`: `linear_workspaces(org_id PK, name, url_key, added_at)` + `ALTER TABLE linear_sync_state ADD COLUMN active_org_id TEXT`. Register in `db.rs`.
- [x] 4.2 `linear/auth.rs`: per-workspace keyring accounts `linear-token::<org_id>` + per-org 0600 fallback `linear-<org_id>.toml`; `load_key_for(org_id)`, `store_key_for(org_id, key)`, `remove_key_for(org_id)`. One-time migration of the legacy `linear-token` account (validate → resolve org → re-store namespaced → mark active; idempotent).
- [x] 4.3 `db.rs`: `linear_workspaces` CRUD + `set_active_org` / `get_active_org`; `wipe_linear_mirror()` (delete issue/team/state/label/cycle/project rows, keep workspaces + sync_state). Tests.
- [x] 4.4 `linear/client.rs`: `resolve_organization()` (`organization { id name urlKey }`) for add/validate.
- [x] 4.5 `linear/mod.rs`: `linear_list_workspaces`, `linear_add_workspace(key)`, `linear_remove_workspace(org_id)`, `linear_set_active_workspace(org_id)` (bump epoch → wipe → set active → clear teams → re-poll). Poller loads the active org's key; no active → `no_key`. Register in `lib.rs`.
- [x] 4.6 `SettingsPanel` Linear section: workspace list (name + active radio + remove) + "Add workspace" (paste key → validate → add). Replaces/wraps the single-key field.
- [x] 4.7 Tests: switch discards an in-flight commit (epoch guard); wipe leaves workspaces+sync_state; legacy-key migration idempotent.

## 5. Verification
- [x] 5.1 `cargo clippy -- -D warnings && cargo test && cargo fmt --check` · `npx tsc --noEmit` · `vite build`
- [x] 5.2 Manual walk (dev) per proposal § Cómo verifico. Walked OK 2026-06-18 (A+B+C+D).
