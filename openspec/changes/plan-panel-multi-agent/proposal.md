## Why

The Plan panel is broken today for any user whose Claude Code `plansDirectory` setting doesn't resolve to `<cwd>/.claude/plans/` — the only path the `list_session_plans` command scans. The Settings UI exposes a `plans_directory` field that suggests configurability but is ignored by the panel: it's only consumed by the global `list_plans` command (not invoked from the frontend) and by the live-event watcher. A reported user verified that their cluihud setting matched their plan location and still saw an empty panel — the field is decorative.

Beyond the CC bug, the panel inherits CC's path convention for every agent. OpenCode's plan mode is read-only by design (verified against `anomalyco/opencode` `packages/opencode/src/agent/agent.ts` on the `dev` branch — only `edit` permission exceptions for `.opencode/plans/*.md` and `Global.Path.data/plans/*.md`, not automatic save; cross-referenced with upstream issue [#11078](https://github.com/anomalyco/opencode/issues/11078) where the model refuses to write even with permission). Codex's plan mode is TUI-only with no disk persistence. Pi has no native plan mode — only a community extension that restricts tools without producing artifacts. The current behavior shows silent empty panels for these agents, which is misleading.

## What Changes

- Add `plan_capability(session, cwd) -> PlanCapability` method to the `AgentAdapter` trait. Variants in v1: `FileBased { dir, label }` and `NotApplicable`.
- CC adapter returns `FileBased`, with the directory resolved from CC's `plansDirectory` setting following CC's own resolution rules (cwd-relative vs absolute, with project-local settings overriding home).
- OpenCode, Codex, and Pi adapters return `NotApplicable`. Rationale documented in `docs/Per-agent feature limitations.md` (Obsidian vault) for future revisit. OpenCode-specific note: if upstream lands reliable automatic plan persistence, revisit to declare `FileBased`.
- `list_session_plans` Tauri command delegates to the active session's adapter, removing the hardcoded path.
- Frontend hides the "Plans" entry from the right-panel chrome when the active session's capability is `NotApplicable`. Empty state for `FileBased` shows the resolved path and a config hint.
- The `plans_directory` field is removed from cluihud's Config and Settings UI — the resolved path comes from the agent adapter, not from cluihud config.

## Capabilities

### New Capabilities

- `plan-panel-multi-agent`: defines the `PlanCapability` model, the trait method, the per-adapter declarations, the command delegation, and the frontend panel visibility + empty-state behavior.

### Modified Capabilities

- `cc-adapter`: the existing "Plan watcher respects user's plansDirectory configuration" requirement (introduced in the archived `agent-adapter-foundation` change) is reformulated. Path resolution moves into the CC adapter's `plan_capability()` impl, and both the watcher and `list_session_plans` SHALL consume that resolver — eliminating the two divergent code paths that produced the original bug.

## Impact

- **Backend**:
  - `src-tauri/src/agents/mod.rs` — new `PlanCapability` enum and trait method.
  - `src-tauri/src/agents/claude_code/adapter.rs` — implements resolver + `plan_capability`.
  - `src-tauri/src/agents/claude_code/plan.rs` — refactored to consume the resolver instead of taking a path argument from `lib.rs`.
  - `src-tauri/src/agents/opencode/adapter.rs`, `codex/adapter.rs`, `pi/adapter.rs` — each declares its `plan_capability`.
  - `src-tauri/src/commands.rs` — `list_session_plans` rewritten to delegate via `AgentRegistry`.
  - `src-tauri/src/config.rs` — `plans_directory` field removed (back-compat: tolerated on load, ignored).
  - `src-tauri/src/lib.rs` — plan watcher setup updated to use the CC resolver instead of `config.plans_directory`.

- **Frontend**:
  - `src/stores/plan.ts` — new `activePlanCapabilityAtom` that fetches capability for the active session.
  - `src/components/panel/PlanListView.tsx` — empty state ramifies on capability + resolved path.
  - Right-panel chrome (panel switcher) — hides the Plans entry based on capability; redirects to fallback panel when active session changes to `NotApplicable`.
  - `src/components/settings/SettingsPanel.tsx` — removes the `plans_directory` form field.

- **Out of scope**:
  - Codex plan persistence via rollout JSONL parsing. Parked as a future Change 2 (`codex-plan-from-rollout`) pending an empirical spike. Documented in `Per-agent feature limitations.md`.
  - Annotation behavior changes — annotations continue to apply only when a plan is loaded; no spec or code change needed.
  - Global `list_plans` Tauri command — retained as-is if any callers remain; verify with grep during impl and remove if unused.

- **Existing flows preserved**:
  - CC users whose `plansDirectory` is `.claude/plans` (the dev's current setup) see no behavior change.
  - Hook pipeline, plan review (`ExitPlanMode` FIFO), plan editing, and the `submit_plan_decision` command are untouched.
