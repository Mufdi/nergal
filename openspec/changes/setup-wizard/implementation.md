# Implementation Plan: setup-wizard

> Grounded in current codebase, symbols verified 2026-06-29 (via Explore map). Behaviour — not just symbol existence — confirmed for the load-bearing claims below. **DOC-ONLY: no code is written yet.** Implementation happens after the Windows port walk closes; walk findings related to onboarding may be folded into this same change.

## Verified codebase facts (do not re-assume)

**Backend (Rust, `src-tauri/src/`):**
- `setup::run()` registers the `nergal hook …` commands in `~/.claude/settings.json` — `setup.rs:150`. Idempotent: merges into `hooks`, removes only obsolete Nergal hooks, never clobbers unrelated keys.
- `setup_hooks` Tauri command wraps `setup::run()` and returns `Result<String, String>` — `commands.rs:606`. Registered in the handler at `lib.rs:432` under the `// Setup command` comment.
- Agent detection: `AgentRegistry::scan() -> Vec<(AgentId, DetectionResult)>` — `agents/registry.rs:56`; `DetectionResult { installed: bool, … }` — `agents/mod.rs:274`. The frontend-facing wrapper is `list_available_agents` (registered `lib.rs:505`, defined `commands.rs:3095`) returning `AvailableAgent[]` (each with `installed: bool`).
- Shell resolution: `config::resolve_pty_shell(&configured) -> (String, Vec<String>)` — `config.rs` (added in the Windows port). Default via `config::default_shell()`.
- `Config` struct — `config.rs:115`. Fields end with `… mcp_server_enabled, summary, cross_session, agent_spawned_worktrees`. `#[serde(default)]` is the established pattern for additive fields (`cross_session`, `agent_spawned_worktrees`).
- `get_config` returns full `Config::load()` — `commands.rs:94`. `save_config` merges the frontend payload over on-disk config and drops `BACKEND_OWNED_CONFIG_KEYS` — `commands.rs:118`; that list is at `commands.rs:105-112` (`summary`, `cross_session`, `agent_spawned_worktrees`, the three poll/window ints). A frontend-writable flag must **NOT** be added there.
- `agents:detected` event is emitted by the hook server on a `rescan_agents` control message — `hooks/server.rs:336`; there is **no spontaneous startup emit**. Initial population is via the imperative `list_available_agents`.

**Frontend (React/TS, `src/`):**
- Overlay pattern: `Dialog`/`DialogContent` (shadcn) — `components/ui/dialog.tsx`. Canonical multi-section example: `SettingsPanel.tsx` (mounted `Workspace.tsx:425`, gated by `settingsOpenAtom` in `stores/config.ts:5`). Atom-driven open/close.
- Root mount siblings: `App.tsx:162-164` mounts `<Workspace/>`, `<AskUserModal/>`, `<WorktreeGate/>` inside `ErrorBoundary`.
- Startup config load: `App.tsx:81-85` — `invoke<Config>("get_config")` then `setConfig(cfg)`. Earliest point where `cfg.onboarding_completed` is known.
- Listener setup (incl. `setupAgentListeners` → `agents:detected`): `App.tsx:125-157`.
- Status bar: `StatusBar.tsx:75`; center transient-indicator cluster at `StatusBar.tsx:204` (hosts `LocalhostPortChips`/`IncidentChips`/`NotificationHistory`). Canonical warning-badge: `IncidentChips` — `StatusBar.tsx:555-609` (TriangleAlert chip, amber `bg-yellow-500/15 text-yellow-400` / red, returns `null` when empty, atom-driven).
- Agent store: `availableAgentsAtom` — `stores/agent.ts:54` (`AgentDetection[]`, shape `stores/agent.ts:36-42`). `AvailableAgent` richer type — `types.ts:185-200`.
- Frontend `Config` type — `types.ts:33-59`. `configAtom` defaults — `stores/config.ts:12-29`.
- Settings sections: `SectionId` + `SECTIONS` — `SettingsPanel.tsx:1983-1998`; `agents` section render ~`3097`; `DetectedAgentsList` (1224-1256) has a Rescan button calling `list_available_agents`. `installSource` is read only inside `AboutSection` — `SettingsPanel.tsx:2042-2058` (via `get_install_source`).
- **No existing onboarding/first-run/welcome/wizard code anywhere** (Explore confirmed zero matches) — build from scratch, no duplication risk.

## Execution order

1. **Backend config flag**: add `onboarding_completed: bool` (`#[serde(default)]`) to `Config` after `mcp_server_enabled` — `config.rs`. Confirm it round-trips through `get_config`/`save_config` and is NOT in `BACKEND_OWNED_CONFIG_KEYS`.
2. **Backend `setup_status` command**: new command + return struct (serde) reporting `{ hooks_registered, hooks_path, agents: [{id, installed}], default_agent, shell, transcripts_dir }`. Reuse `setup`'s settings-path + hook-presence probe, `AgentRegistry::scan` (the same scan behind `list_available_agents`), `config::resolve_pty_shell`, and the configured transcripts dir. Register at `lib.rs:433` under `// Setup command`.
3. **Frontend config plumbing**: `onboarding_completed?: boolean` in `types.ts`; default `false` in `configAtom` (`stores/config.ts`); add `wizardOpenAtom` (+ a derived `setupStatusAtom`/store) in `stores/config.ts` or a new `stores/setup.ts`.
4. **Wizard overlay component** (`components/setup/SetupWizard.tsx`): `Dialog`/`DialogContent`, multi-step, matching `SettingsPanel` conventions + design-system tokens + focus zones. Steps: hooks (one-click Configure → `setup_hooks` → re-probe), agent presence (reuse `list_available_agents`/store; link to install if none), suggestions (default-agent pick, shell, transcripts), platform note (via `installSource`). Skippable (close = set `onboarding_completed=true` via `save_config`).
5. **First-run trigger**: chain off `App.tsx:81-85` — after `setConfig(cfg)`, if `cfg.onboarding_completed === false` → open wizard. D8 migration nicety: if hooks already registered at that point, set `onboarding_completed=true` instead of opening.
6. **Health-gate chip**: new chip component in `StatusBar.tsx:204` cluster, modeled on `IncidentChips`, driven by a `setupCriticalMissingAtom` (true when `onboarding_completed` and hooks-or-agent critical check is false); click re-opens the wizard. Returns `null` when both critical checks pass.
7. **Mount + Settings re-entry**: mount `<SetupWizard/>` as a sibling in `App.tsx:163`; add a "Run setup again" button after `DetectedAgentsList` in the `agents` section (~`SettingsPanel.tsx:3115`) that sets `wizardOpenAtom=true` + closes Settings.
8. **Verify** (full check) + manual walk of the scenarios.

## Plan

- **`config.rs`**: `pub onboarding_completed: bool` with `#[serde(default)]`; default `false` in the `Default` impl.
- **`commands.rs`**: `#[derive(Serialize)] struct SetupStatus { … }` + `#[tauri::command] pub fn setup_status(...) -> Result<SetupStatus, String>`. Reuse the hook-presence check from `setup` (extract a small `hooks_registered(settings: &Map) -> bool` helper if not already isolable) so wizard and `setup::run` agree on "registered". Agents via the shared registry scan.
- **`lib.rs`**: add `commands::setup_status,` at line 433.
- **`types.ts`**: `onboarding_completed?: boolean` on `Config`; `SetupStatus` type mirroring the Rust struct.
- **`stores/config.ts`** (or new `stores/setup.ts`): `wizardOpenAtom`, `setupStatusAtom`, derived `setupCriticalMissingAtom`; helper `refreshSetupStatus()` invoking `setup_status` + `list_available_agents`.
- **`components/setup/SetupWizard.tsx`**: the overlay. Reuse `lib/confirm.ts` only if a destructive confirm is needed (it is not — Configure is additive/idempotent).
- **`components/setup/SetupHealthChip.tsx`**: the status-bar chip (or inline in `StatusBar.tsx` next to `IncidentChips`).
- **`App.tsx`**: first-run trigger + mount.
- **`StatusBar.tsx`**: render the health chip in the center cluster.
- **`SettingsPanel.tsx`**: "Run setup again" button in the `agents` section.

## Per-phase risk

- **[Settings.json mutation surprises the user]** → reuse idempotent `setup::run()` behind the explicit "Configure" click (D4); never auto-write. Show the path being modified in the step copy.
- **[`configAtom` stale / `save_config` drops keys]** (`[[feedback_frontend_config_stale]]`) → `onboarding_completed` is frontend-writable; write via the normal `save_config` path, NOT added to `BACKEND_OWNED_CONFIG_KEYS`; re-read after save. Verify the round-trip in a manual check.
- **[Existing users get the wizard popped]** → D8: seed `onboarding_completed=true` at first load when hooks are already registered.
- **[Agent list empty on initial mount]** (no startup `agents:detected`) → the wizard fetches via `list_available_agents` imperatively and subscribes to `agents:detected` for refresh; never reads a possibly-empty store snapshot as authoritative.
- **[Health chip flicker during probe]** → debounce the re-probe and only show the chip when a critical check is *confirmed* false (not during in-flight probes).
- **[Shell value drift Windows]** → `setup_status.shell` reflects `resolve_pty_shell` (the same resolver the PTY uses), so the wizard shows what the session will actually use.

## Verification

Project full check (CLAUDE.md): `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.

Change-specific manual walk (maps to spec scenarios):
1. Fresh state (`onboarding_completed=false`, hooks absent): wizard opens on mount; "Configure" registers hooks; re-probe flips `hooks_registered` true; panels receive events.
2. Skip with a critical missing: app usable; status-bar health chip visible; clicking it re-opens the wizard.
3. Both critical pass: no health chip; wizard does not auto-open on next launch (`onboarding_completed=true` persisted).
4. Settings → "Run setup again" re-opens the wizard regardless of the flag.
5. Existing install (hooks already present, flag absent → D8): wizard does NOT pop; flag seeded true.
6. Per-OS note matches `installSource` (Windows→SmartScreen).
