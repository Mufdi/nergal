# session-launch-options Specification

## Purpose

Let the user choose how a session launches — initial permission mode, bypass availability, and a shell prelude — at creation time, per session, applied again on every resume. Complements the `agent-adapter` spec (which owns the adapter-side flag mapping); this spec owns persistence and UI.

## Requirements

### Requirement: Launch options persist on the session row

`LaunchOptions { permission_preset, allow_skip_in_cycle, startup_command }` SHALL persist as a nullable JSON column `launch_options` on `sessions` (migration `011`). All-default options SHALL be stored as NULL. A malformed column SHALL parse as `None` and never break session loading.

#### Scenario: Options round-trip and re-apply on resume

- **WHEN** a session is created with non-default launch options and later resumed (including after an app restart)
- **THEN** `find_session` SHALL return the same options
- **AND** the spawn path SHALL apply them to the relaunch command

#### Scenario: Startup command runs on every spawn

- **WHEN** a session with a `startup_command` spawns (fresh or resume)
- **THEN** the PTY layer SHALL run the prelude between `cd <cwd>` and the agent binary, chained with `&&`
- **AND** a failing prelude SHALL abort the agent launch

### Requirement: Agent picker is the launch-options surface

The agent picker modal SHALL open on every session creation — even with a single installed agent — because it hosts the launch options. Below the agent cards it SHALL render a keyboard-navigable options list scoped to the selected agent: one check row per supported preset (radio semantics — selecting one clears the others; re-selecting clears to default), an "allow skip in cycle" toggle when the adapter supports it (disabled while the Bypass preset is selected), and a startup-command input.

#### Scenario: Keyboard flow

- **WHEN** the modal is open
- **THEN** `←`/`→` SHALL switch agents, `↑`/`↓` SHALL move between the agent row and option rows, `Space` SHALL toggle the highlighted row, `1–9` SHALL jump-create, and `Enter` SHALL create the session from any row (including the startup input)

#### Scenario: Options follow the selected agent

- **WHEN** the user switches to an agent that doesn't support the currently selected preset or toggle
- **THEN** the unsupported selections SHALL reset to default

#### Scenario: Zero installed agents skips the modal

- **WHEN** no agent is detected as installed
- **THEN** session creation SHALL fall back to the backend default agent without showing an empty modal
