## Why

The plan panel goes empty when Claude Code writes plans to a directory Nergal's
auto-detection doesn't search. The Windows walk-3 surfaced the concrete case: CC
on Windows writes to the home-global `~/.claude/plans`, not the project-local
`<cwd>/.claude/plans`. Auto-detection was already fixed on `main` (the panel opens
from the hook payload's inline plan content; `cc_plan_dirs` searches cwd-local +
home-global; `resolve_cc_plans_directory` honors a configured `plansDirectory`).

This change adds the **escape hatch + hardening** so a user is never stuck with a
silently-empty panel: a per-workspace override to tell Nergal where to look, a
hardened relative-path search that doesn't depend on unconfirmed CC-on-Windows
resolution behaviour, and an empty-state hint pointing at the setting.

It mirrors the existing per-workspace **OpenSpec Directory** override 1:1
(`workspace_config.openspec_dir`, migration 012, `get/set_workspace_openspec_dir`,
`resolve_openspec_dir`, `OpenSpecPathField` + `ValidatedPathField`) — same DB
shape, same command shape (`{ configured, default_dir }`), same UI component.

## What Changes

- **Per-workspace `plans_dir` override (DB)** — add a `plans_dir` column to
  `workspace_config` (migration `030_workspace_plans_dir.sql`, mirroring the
  `openspec_dir` column from migration 012). Add `get_workspace_plans_dir` /
  `set_workspace_plans_dir` to `db.rs` (mirror the openspec pair).
- **`get_workspace_plans_dir` command** — returns `{ configured: Option<String>,
  default_dir: String }` where `default_dir` is the auto-resolved
  `resolve_cc_plans_directory(cwd)` for the workspace, so the Settings UI can show
  what Nergal resolved (transparency/debug) with the override as an optional
  prefill. Register in `lib.rs`.
- **`cc_plan_dirs` additive override + relative hardening** (`hooks/server.rs`) —
  prepend the workspace's configured `plans_dir` override to the search list (it
  is **additive**: it tells Nergal where to *look*, it does NOT change where CC
  *writes*). For a **relative** `plansDirectory`, search BOTH `cwd/rel` and
  `home/rel` so resolution is robust regardless of how CC resolves relatives on
  Windows — no dependency on unconfirmed runtime behaviour.
- **Settings input** — a `PlansPathField` mirroring `OpenSpecPathField`
  (`ValidatedPathField` with `configKey="plans_dir"`, per-workspace selector,
  auto-resolved `default_dir` as placeholder, persisted on Settings Save via
  `set_workspace_plans_dir`). New `plansDirDraftAtom` mirroring
  `openspecDirDraftAtom`.
- **Empty-state hint** — when the plan panel is empty (`PlanPanel.tsx:222` "No
  plan yet"), add a one-line hint linking to the plans-directory setting, so a
  user whose CC writes plans somewhere unexpected is guided to the override
  instead of staring at a dead panel.

## Capabilities

### New Capabilities

- `plans-directory`: per-workspace override for where Nergal looks for CC's
  plans, the robust (additive) resolution rules, and the empty-state guidance.

### Modified Capabilities

(none — no existing spec covers the plan-panel resolution path; `plans_dir`
mirrors the un-spec'd `openspec_dir` override)

## Impact

- **`src-tauri/migrations/030_workspace_plans_dir.sql`** (new): `ALTER TABLE
  workspace_config ADD COLUMN plans_dir TEXT`.
- **`src-tauri/src/db.rs`**: register migration; `get_workspace_plans_dir` +
  `set_workspace_plans_dir` (mirror `openspec_dir` pair); unit test.
- **`src-tauri/src/commands.rs`**: `get_workspace_plans_dir` command
  (`{ configured, default_dir }`).
- **`src-tauri/src/hooks/server.rs`**: `cc_plan_dirs` prepends the override +
  adds the home-resolved relative dir.
- **`src-tauri/src/lib.rs`**: register `get_workspace_plans_dir` +
  `set_workspace_plans_dir`.
- **`src/stores/workspace.ts`**: `plansDirDraftAtom` (mirror
  `openspecDirDraftAtom`).
- **`src/components/settings/SettingsPanel.tsx`**: `PlansPathField` + persist the
  draft on Save (mirror the `openspecDirDraft` save at `:2974`).
- **`src/components/plan/PlanPanel.tsx`**: empty-state hint.
- **Out of scope**: the Windows visual walk (deferred to the final walk); the
  token-fallback security follow-up (separate backlog item).
