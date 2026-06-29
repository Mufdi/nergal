# Architect Brief — setup-wizard

## Project mission
Nergal — Linux/macOS/Windows desktop wrapper for the Claude Code CLI (Tauri 2 + React 19). The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline + transcript watchers. Nergal runs *around* the agent, augmenting the loop — it does not reimplement agent primitives.

## Control metadata (from `.work-modules.json`)
- **Tier**: L · **Ceremony**: deep · **Risk**: medium
- **Files estimate**: 9 · **Tags**: feature, onboarding, ux · **Visibility**: public
- **Spec target**: `setup-wizard` (new capability)

## Context
The hook-registration primitive (`setup::run()` / `setup_hooks`) and agent detection (`AgentRegistry::scan`/`list_available_agents`) exist but nothing surfaces or triggers them on first run → fresh installs have dead panels. This change adds the guided first-run wizard + non-blocking health-gate. Crystallized decisions (user, 2026-06-29): first-run + health-gate trigger; one-click consented hooks config; skippable/non-blocking. See `design.md` D1-D8.

## Gating decision (for the FUTURE implementation phase)
- **Triple-prompt gating: ON** — `files_estimate (9) >= 5`. Spawn single-reviewer sequential by default; escalate to 4-parallel if the diff touches `~/.claude/settings.json` mutation paths or scope creeps >1.5×.
- **Iterative-plan-review: recommended** — `files_estimate >= 5` triggers A3. Run the Claude evaluator on this plan before/at implementation start (it was deferred now per the doc-only constraint).
- **6-phase gates**: compile+test+lint always; no security/deps gates unless the diff grows to touch auth/deps.

## Dependencies / blockers
- **BLOCKED-BY**: the Windows port walk must complete first (user constraint — implementation is parked). This change is documented now to capture the fresh memory; build it after the port closes.
- Walk findings related to onboarding/first-run may be folded into THIS change before implementation (update the same artifacts, do not create a new change — see config rule "Mid-implementation revision").

## Lazy-skill notes for the build phase (A6)
At implementation time (frontend-heavy), apply: `/frontend-build` aggregator, `modal-vs-page` (validates the overlay-not-route choice in D7), `shadcn` (the app's Dialog is shadcn-based), and the project design system `docs/design.md` (R0162 tokens) + `docs/patterns.md` (focus zones, keyboard nav). Run `/frontend-review` before commit.

## Reuse map (anchors in `implementation.md`)
Backend: `setup_hooks`/`setup::run` (`setup.rs:150`, `commands.rs:606`), `AgentRegistry::scan` (`registry.rs:56`), `list_available_agents` (`commands.rs:3095`), `resolve_pty_shell`, `Config` (`config.rs:115`). Frontend: `Dialog`/`SettingsPanel` overlay, `App.tsx:81-85` first-run point, `StatusBar.tsx:204`/`IncidentChips` health-chip model, `availableAgentsAtom` (`stores/agent.ts:54`), `installSource` (`SettingsPanel.tsx:2042`).
