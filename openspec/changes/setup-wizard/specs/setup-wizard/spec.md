## ADDED Requirements

### Requirement: Readiness status probe
The system SHALL expose a single backend command that reports Nergal's runtime readiness in one call, so the wizard and the health-gate render from one source of truth.

The reported status MUST include, at minimum:
- `hooks_registered` (bool): whether the `nergal hook …` commands are present in `~/.claude/settings.json`.
- `agent_detected` (bool) + `agents` (list of `{ id, installed }`): whether at least one agent CLI is on PATH, and the per-agent detection result.
- `default_agent` (optional id): the configured/resolved default agent when more than one is installed.
- `shell` (string): the resolved interactive shell path.
- `transcripts_dir` (string): the configured transcripts directory.

The two **critical** checks are `hooks_registered` and `agent_detected`; the rest are informational.

#### Scenario: Probe reports all readiness fields
- **WHEN** the frontend invokes the readiness command
- **THEN** the response contains `hooks_registered`, `agent_detected`, the per-agent `agents` list, the resolved `shell`, and `transcripts_dir`

#### Scenario: Hooks present is detected as registered
- **WHEN** `~/.claude/settings.json` already contains the `nergal hook …` commands
- **THEN** `hooks_registered` is `true`

#### Scenario: No agent on PATH is detected as not-ready
- **WHEN** no supported agent CLI (`claude`/`codex`/…) is resolvable on PATH
- **THEN** `agent_detected` is `false` and every entry in `agents` has `installed: false`

### Requirement: First-run wizard trigger
The system SHALL open the setup wizard automatically on the first launch after install, tracked by a persisted `onboarding_completed` flag in the application config (default `false`). The wizard MUST NOT auto-open again once `onboarding_completed` is `true`.

#### Scenario: Wizard opens on a fresh install
- **WHEN** the app mounts and `onboarding_completed` is `false`
- **THEN** the setup wizard overlay opens after the initial config load

#### Scenario: Wizard does not reopen after completion
- **WHEN** the app mounts and `onboarding_completed` is `true`
- **THEN** the setup wizard does not auto-open

#### Scenario: Completing or skipping the wizard persists the flag
- **WHEN** the user completes or skips the wizard
- **THEN** `onboarding_completed` is set to `true` and persists across restarts

### Requirement: One-click consented hooks configuration
The wizard SHALL detect the hook registration state and, when hooks are missing, offer a visible action that registers them by invoking the existing idempotent hooks-setup command. The system MUST NOT modify `~/.claude/settings.json` without an explicit user action.

#### Scenario: Configure hooks from the wizard
- **WHEN** `hooks_registered` is `false` and the user activates the "Configure" action
- **THEN** the system runs the hooks-setup command and re-probes, and `hooks_registered` becomes `true`

#### Scenario: No silent settings mutation
- **WHEN** the wizard opens with hooks missing
- **THEN** `~/.claude/settings.json` is not modified until the user activates the "Configure" action

#### Scenario: Idempotent re-run
- **WHEN** the user activates "Configure" and hooks are already present
- **THEN** the command completes without error and does not duplicate hook entries

### Requirement: Non-blocking skippable flow
The wizard SHALL be dismissible at any time and MUST NOT block use of the application. Pending checks MUST remain reachable after dismissal.

#### Scenario: User skips with critical checks unresolved
- **WHEN** the user dismisses the wizard while `hooks_registered` or `agent_detected` is `false`
- **THEN** the app is fully usable and the pending state remains visible via the health-gate indicator

### Requirement: Health-gate regression banner
After first run, the system SHALL show a non-blocking indicator in the status-bar surface when a **critical** check (`hooks_registered` or `agent_detected`) is `false`, with a one-click path that re-opens the wizard. The indicator MUST NOT appear when both critical checks pass.

#### Scenario: Banner appears when hooks regress
- **WHEN** `onboarding_completed` is `true` and a re-probe reports `hooks_registered: false`
- **THEN** the health-gate indicator is shown and activating it re-opens the wizard

#### Scenario: Banner hidden when all critical checks pass
- **WHEN** both `hooks_registered` and `agent_detected` are `true`
- **THEN** the health-gate indicator is not shown

### Requirement: Settings re-entry
The system SHALL provide an entry in the Settings panel that re-opens the setup wizard on demand, regardless of `onboarding_completed`.

#### Scenario: Reopen wizard from Settings
- **WHEN** the user activates the "Run setup again" entry in Settings
- **THEN** the setup wizard overlay opens

### Requirement: Agent, shell and transcripts review
The wizard SHALL surface the detected agents (allowing selection of a default when more than one is installed), the resolved shell, and the transcripts directory as informational/suggested settings. These non-critical checks MUST NOT gate the "all set" state.

#### Scenario: Default agent selectable when multiple installed
- **WHEN** more than one agent reports `installed: true`
- **THEN** the wizard lets the user pick the default agent and persists the choice

#### Scenario: Non-critical checks do not block completion
- **WHEN** the shell or transcripts suggestion is left at its default
- **THEN** the wizard still reaches the "all set" state provided both critical checks pass

### Requirement: Platform-aware guidance
The wizard SHALL render a platform-specific note derived from the install source (SmartScreen on Windows, Gatekeeper on macOS, launcher-PATH guidance on Linux).

#### Scenario: Windows install shows SmartScreen note
- **WHEN** the install source resolves to Windows
- **THEN** the wizard shows the SmartScreen guidance note

#### Scenario: Note matches the install source
- **WHEN** the install source resolves to a given OS
- **THEN** only that OS's note is shown
