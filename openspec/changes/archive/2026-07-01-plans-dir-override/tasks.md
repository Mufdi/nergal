# Tasks — plans-dir-override

> Lean M (aditivo, mirror 1:1 del feature `openspec_dir`). Reuse anchors verified
> 2026-06-30. No specs/design/implementation.md (quick-fix ceremony). Read the
> `openspec_dir` precedent before each mirror step — same shape, same names.

## 1. DB — per-workspace `plans_dir` column

- [x] 1.1 `src-tauri/migrations/030_workspace_plans_dir.sql`: `ALTER TABLE workspace_config ADD COLUMN plans_dir TEXT;` (mirror `012_workspace_openspec_dir.sql`).
- [x] 1.2 `db.rs`: register the migration in the migration list (mirror `db.rs:252` for 012).
- [x] 1.3 `db.rs`: `get_workspace_plans_dir(&self, workspace_id) -> Result<Option<String>>` + `set_workspace_plans_dir(&self, workspace_id, plans_dir: Option<&str>)` — copy the `get/set_workspace_openspec_dir` pair (`db.rs:1622`/`1636`), trim+empty→NULL, upsert into `workspace_config`.
- [x] 1.4 `db.rs`: unit test mirroring `workspace_openspec_dir_override` (`db.rs:1933`) — set/get/clear round-trip.

## 2. Backend — command + cc_plan_dirs

- [x] 2.1 `commands.rs`: `#[tauri::command] get_workspace_plans_dir(workspace_id) -> Result<{ configured: Option<String>, default_dir: String }, String>`. `configured` = `db.get_workspace_plans_dir`; `default_dir` = `resolve_cc_plans_directory(cwd)` for the workspace's repo/worktree path (resolve cwd the same way `cc_plan_dirs` does — `worktree_path` else `workspace_repo_path`). Serialize keys `configured`/`default_dir` (match `get_workspace_openspec_dir`'s shape).
- [x] 2.2 `hooks/server.rs` `cc_plan_dirs`: (a) **prepend** the workspace's `plans_dir` override (if set) resolved to absolute — additive, first in the search order; (b) for a **relative** configured `plansDirectory`, push BOTH `cwd/rel` and `home/rel` (dedup). Keep the existing cwd-local + home-global fallbacks. Do not change where CC writes.
- [x] 2.3 `lib.rs`: register `commands::get_workspace_plans_dir` + `commands::set_workspace_plans_dir` (the setter mirrors `set_workspace_openspec_dir`'s command if one exists; otherwise the frontend calls a `set_workspace_plans_dir` command — add it next to the openspec setter).
- [x] 2.4 Unit test for the `cc_plan_dirs` relative-hardening (both `cwd/rel` and `home/rel` present) — pure/synthetic, runs on Linux.

## 3. Frontend — Settings input (mirror OpenSpecPathField)

- [x] 3.1 `stores/workspace.ts`: `plansDirDraftAtom` mirroring `openspecDirDraftAtom` (`:134`) — same `{ workspaceId, value, defaultDir, baseline }` shape.
- [x] 3.2 `SettingsPanel.tsx`: `PlansPathField()` mirroring `OpenSpecPathField` (`:277`) — per-workspace selector, `invoke("get_workspace_plans_dir")` for `{ configured, default_dir }`, `ValidatedPathField` `configKey="plans_dir"` `kind="dir"` `caseInsensitive`, placeholder = resolved `default_dir`, help copy explaining it's additive (where Nergal looks, not where CC writes). Render it near `OpenSpecPathField`.
- [x] 3.3 `SettingsPanel.tsx` Save handler (`:2974`): read `plansDirDraftAtom`, invoke `set_workspace_plans_dir` when changed (mirror the `openspecDirDraft` persist block).

## 4. Frontend — empty-state hint

- [x] 4.1 `PlanPanel.tsx` (`:222` "No plan yet"): add a one-line hint — "No plans found · set the plans directory in Settings" — that opens Settings (reuse the existing settings-open atom/handler). Keep it subtle (`text-muted-foreground`), only in the empty state.

## 5. Verification

- [x] 5.1 Full check: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.
- [x] 5.2 Manual (Linux): set a custom plans dir in Settings → the auto-resolved default shows as placeholder; a plan written to the override dir surfaces in the panel; clearing the override falls back to auto-detection; empty panel shows the hint + link opens Settings.
- [x] 5.3 Windows visual check DEFERRED to the final walk (the resolution logic is Linux-testable; only the on-screen result on WebView2 is walk-pending).
