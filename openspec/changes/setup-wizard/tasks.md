# Tasks — setup-wizard

> **DOC-ONLY for now** (user constraint 2026-06-29): these tasks are specified but NOT to be implemented until the Windows port walk closes. Walk findings related to onboarding may be folded in before implementation starts. Read `implementation.md` first — it carries the verified line-refs + execution order.

## 1. Backend — config flag

- [ ] 1.1 Add `onboarding_completed: bool` (`#[serde(default)]`) to `Config` after `mcp_server_enabled` (`src-tauri/src/config.rs:115` struct) + default `false` in the `Default` impl.
- [ ] 1.2 Confirm it round-trips through `get_config` (`commands.rs:94`) and `save_config` (`commands.rs:118`), and is **NOT** added to `BACKEND_OWNED_CONFIG_KEYS` (`commands.rs:105-112`) — the wizard must be able to write it.

## 2. Backend — `setup_status` probe

- [ ] 2.1 Define `#[derive(Serialize)] struct SetupStatus { hooks_registered: bool, hooks_path: String, agents: Vec<AgentReadiness>, default_agent: Option<String>, shell: String, transcripts_dir: String }` (+ `AgentReadiness { id, installed }`).
- [ ] 2.2 Extract/locate a `hooks_registered(&settings) -> bool` check shared with `setup::run()` so the wizard and the registrar agree on "registered".
- [ ] 2.3 `#[tauri::command] pub fn setup_status(...)` assembling: hooks check, `AgentRegistry::scan` (the scan behind `list_available_agents`), `config::resolve_pty_shell`, configured transcripts dir, `default_agent` from config.
- [ ] 2.4 Register `commands::setup_status,` at `lib.rs:433` under the `// Setup command` comment.

## 3. Frontend — config + setup store

- [ ] 3.1 Add `onboarding_completed?: boolean` to the `Config` type (`src/lib/types.ts:33-59`) + a `SetupStatus` type mirroring the Rust struct (`types.ts`).
- [ ] 3.2 Add `onboarding_completed: false` to the `configAtom` default (`src/stores/config.ts:12-29`).
- [ ] 3.3 Add `wizardOpenAtom`, `setupStatusAtom`, and a derived `setupCriticalMissingAtom` (true only when `onboarding_completed` AND a critical check is false) in `stores/config.ts` or a new `stores/setup.ts`.
- [ ] 3.4 Add `refreshSetupStatus()` invoking `setup_status` + `list_available_agents`, writing `setupStatusAtom`; subscribe to `agents:detected` for live agent updates.

## 4. Frontend — wizard overlay

- [ ] 4.1 `components/setup/SetupWizard.tsx` using `Dialog`/`DialogContent` (match `SettingsPanel` conventions + design tokens + focus zones).
- [ ] 4.2 Step: hooks — show state; when missing, prominent "Configure" button → `invoke("setup_hooks")` → `refreshSetupStatus()`; show the `~/.claude/settings.json` path being touched.
- [ ] 4.3 Step: agent presence — reuse `list_available_agents`/`availableAgentsAtom`; if none `installed`, show install guidance (no auto-install).
- [ ] 4.4 Step: suggestions — default-agent pick (when >1 installed), resolved shell, transcripts dir; non-critical, never gate completion.
- [ ] 4.5 Step: platform note — render per-OS card from `installSource` (reuse the `AboutSection` fetch pattern, `SettingsPanel.tsx:2042-2058`).
- [ ] 4.6 Skippable: closing the wizard sets `onboarding_completed=true` via `save_config` (frontend-writable path) and re-reads config.

## 5. Frontend — first-run trigger + mount + re-entry

- [ ] 5.1 First-run: chain off the config-load effect (`App.tsx:81-85`) — after `setConfig(cfg)`, if `cfg.onboarding_completed === false` open the wizard; D8 nicety — if hooks already registered, set the flag true instead of opening.
- [ ] 5.2 Mount `<SetupWizard/>` as a sibling in `App.tsx:163` (between `<AskUserModal/>` and `<WorktreeGate/>`).
- [ ] 5.3 Settings re-entry: "Run setup again" button after `DetectedAgentsList` in the `agents` section (~`SettingsPanel.tsx:3115`) → set `wizardOpenAtom=true` + close Settings.

## 6. Frontend — health-gate chip

- [ ] 6.1 New chip (modeled on `IncidentChips`, `StatusBar.tsx:555-609`) rendered in the center cluster (`StatusBar.tsx:204`), driven by `setupCriticalMissingAtom`; click re-opens the wizard; returns `null` when both critical checks pass.
- [ ] 6.2 Debounce re-probe so the chip only shows on a *confirmed* critical-false, not during in-flight probes.

## 7. Verification

- [ ] 7.1 Full check: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.
- [ ] 7.2 Manual walk — fresh state: wizard opens; "Configure" registers hooks; re-probe flips `hooks_registered`; panels receive events.
- [ ] 7.3 Manual walk — skip with a critical missing: app usable; health chip visible; click re-opens wizard.
- [ ] 7.4 Manual walk — both critical pass: no chip; wizard does not auto-open next launch (flag persisted); D8 existing-install seeding does not pop the wizard.
- [ ] 7.5 Manual walk — Settings "Run setup again" re-opens; per-OS note matches `installSource`.
