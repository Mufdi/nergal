# session-launch-options

## ADDED Requirements

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
