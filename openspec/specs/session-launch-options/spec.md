# session-launch-options Specification

## Purpose

Let the user choose how a session launches — initial permission mode, bypass availability, and a shell prelude — at creation time, per session, applied again on every resume. Complements the `agent-adapter` spec (which owns the adapter-side flag mapping); this spec owns persistence and UI.
## Requirements
### Requirement: Launch options persist on the session row

`LaunchOptions { permission_preset, allow_skip_in_cycle, startup_command }` SHALL persist as a nullable JSON column `launch_options` on `sessions` (migration `011`). All-default options SHALL be stored as NULL. A malformed column SHALL parse as `None` and never break session loading. `startup_command` is a **prelude**: a quick command expected to exit (env setup like `nvm use`, `source .env`) that runs in the agent terminal so the agent inherits its environment. Long-running commands SHALL be expressed as environment shells (see the `quake-terminal` spec), never as the prelude — a non-exiting prelude blocks the agent launch.

#### Scenario: Options round-trip and re-apply on resume

- **WHEN** a session is created with non-default launch options and later resumed (including after an app restart)
- **THEN** `find_session` SHALL return the same options
- **AND** the spawn path SHALL apply them to the relaunch command

#### Scenario: Startup command runs on every spawn

- **WHEN** a session with a `startup_command` spawns (fresh or resume)
- **THEN** the PTY layer SHALL run the prelude between `cd <cwd>` and the agent binary, chained with `&&`
- **AND** a failing prelude SHALL abort the agent launch

#### Scenario: Prelude UI signals the must-exit contract

- **WHEN** the user reaches the startup-command input in the new-session modal
- **THEN** it SHALL be framed as a prelude that must exit, pointing long-running commands to the environment-shells section

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

### Requirement: External deep-link session spawns require in-app confirmation

A session spawned as a result of an external `nergal://` deep link SHALL require explicit in-app confirmation before any workspace is created, any session is spawned, or any prompt is queued. This applies to every route that creates a workspace/session for an unknown path — `session/new` (with a prompt) AND `open-file` (which creates a workspace + session and spawns the agent for an unknown path) AND `open-workspace`'s workspace-creation branch. The confirmation SHALL display the resolved absolute `cwd`/path, a warning when the target is not a git repository (determined by a pre-creation probe, before any workspace exists), and, when present, the literal prompt text (HTML-escaped). If the user declines, no workspace is created, no session is spawned, and no prompt is queued.

The confirmation's default keyboard action SHALL NOT be "proceed": because the dialog is triggered by external input and steals focus, a stray Enter SHALL NOT confirm it. The proceed action requires an explicit pointer or keyed action on the proceed control.

The only deep-link route exempt is focusing an ALREADY-KNOWN session/workspace (no creation, no prompt).

#### Scenario: deep link with prompt is confirmed

- **GIVEN** a `nergal://session/new?cwd=/home/u/proj&prompt=<text>` link is dispatched
- **WHEN** the confirmation dialog appears showing `/home/u/proj` and the prompt text, and the user confirms
- **THEN** the session is created, the prompt is queued, and the session is activated (current behavior, now gated)

#### Scenario: deep link with prompt is declined

- **GIVEN** the same link is dispatched
- **WHEN** the confirmation dialog appears and the user declines
- **THEN** no session is created, no prompt is queued, and no workspace is created solely for this spawn

#### Scenario: non-git target is flagged

- **GIVEN** a `nergal://session/new?cwd=/home/u&prompt=<text>` link where `/home/u` is not a git repository
- **WHEN** the confirmation dialog appears
- **THEN** it warns that the target directory is not a git repository before the user decides

#### Scenario: workspace-creation from an external link is confirmed

- **GIVEN** a `nergal://open-workspace?path=/some/dir` link where `/some/dir` is not an already-known workspace
- **WHEN** the link is dispatched
- **THEN** creating the workspace requires confirmation showing the resolved path

#### Scenario: open-file for an unknown path is gated

- **GIVEN** a `nergal://open-file?path=/some/dir/file.rs` link where `/some/dir` is not a known workspace
- **WHEN** the link is dispatched (this branch would create a workspace + session and spawn the agent)
- **THEN** confirmation is required before the workspace/session is created; a decline creates nothing and spawns no agent

#### Scenario: a stray Enter does not confirm the external dialog

- **GIVEN** the deep-link confirmation is open and the user is typing in the terminal
- **WHEN** an Enter keypress reaches the dialog
- **THEN** it does not trigger the proceed action (proceed requires an explicit pointer/keyed action on the proceed control)

#### Scenario: a second deep link does not bypass a pending confirm

- **GIVEN** a deep-link confirmation is already pending
- **WHEN** a second deep link arrives
- **THEN** it is queued behind the first (honoring the confirm queue), not coalesced or auto-confirmed

