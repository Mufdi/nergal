## ADDED Requirements

### Requirement: Quake terminal overlay

The system SHALL provide a full-width Quake terminal overlay that drops below the TopBar over the content area (above the StatusBar), holding N shell tabs for the active session, resizable in height. It SHALL be toggled and focused with `Ctrl+}` (bound respecting `event.code` per the WebKitGTK convention).

#### Scenario: Toggle and focus cycle

- **WHEN** the overlay is hidden and the user presses `Ctrl+}`
- **THEN** it SHALL open and receive focus (accent border on the quake zone)
- **WHEN** it is visible but unfocused and the user presses `Ctrl+}`
- **THEN** focus SHALL move to it without hiding it
- **WHEN** it is visible and focused and the user presses `Ctrl+}`
- **THEN** it SHALL hide

#### Scenario: Stays visible when focus leaves

- **WHEN** the overlay is open and the user focuses another zone (e.g. `Ctrl+ñ` to the agent terminal)
- **THEN** the overlay SHALL remain visible (logs readable) and the accent border SHALL follow the focused zone

#### Scenario: Visibility is per-session

- **WHEN** the quake is open in session A and the user switches to session B where it was closed
- **THEN** the overlay SHALL be hidden, and SHALL reappear when switching back to A (mirrors the right panel's per-session collapsed state)

#### Scenario: Tab shortcuts while the quake holds focus

- **WHEN** focus is in the quake zone
- **THEN** `Ctrl+W` SHALL close the active shell tab (instead of soft-closing the session), `Ctrl+Shift+T` SHALL open a new shell tab, and `Ctrl+Tab` / `Ctrl+Shift+Tab` SHALL cycle between shell tabs

### Requirement: Quake is a distinct focus zone

The system SHALL add a `quake` focus zone with a `data-focus-zone='quake'` container and the accent border driven by the existing focus system. It SHALL NOT participate in the `alt+left/right` zone cycle; it is reachable only via `Ctrl+}`.

#### Scenario: Accent border reflects focus

- **WHEN** the quake zone holds focus
- **THEN** its border SHALL render in the focus accent color, consistent with the other zones

### Requirement: Per-session auxiliary shells

Auxiliary shells SHALL be shell PTYs owned by a session (keyed under its id), spawned without writing any agent launch command. They SHALL use the session's cwd (worktree). Switching the active session SHALL swap the quake's shells; closing a session SHALL terminate its shells.

#### Scenario: Shells follow the active session

- **WHEN** the user switches from session A to session B
- **THEN** the quake SHALL show B's shells, not A's

#### Scenario: Shells die with the session

- **WHEN** a session is closed
- **THEN** its auxiliary shell PTYs SHALL be terminated

#### Scenario: Shell exit retires its tab

- **WHEN** the shell process ends on its own (the user types `exit`, or it crashes)
- **THEN** its tab SHALL be removed from the quake; teardown-driven kills SHALL NOT be confused with self-exits

### Requirement: Per-region terminal rendering

The canvas renderer SHALL support multiple regions (at least `center` for the agent terminal and `quake` for shells), each with its own host and its own active terminal, so the agent terminal and a shell render simultaneously.

#### Scenario: Agent and shell visible together

- **WHEN** the overlay is open with a running shell while an agent session is active
- **THEN** both the agent terminal (center) and the shell (quake) SHALL render at the same time without the single-host conflict

### Requirement: Environment shells

A session SHALL support a list of environment shells defined as `(label, command)`. Each entry spawns a quake shell. The command SHALL auto-run when the session is first created, and SHALL be pre-filled (typed, not executed) when the session is re-opened after an app restart. The persisted set SHALL be the session's **live tab set**: definitions seed from the new-session modal, ad-hoc tabs join it, closed tabs leave it, and a command submitted in any quake shell updates that tab's remembered command.

#### Scenario: Auto-run on creation

- **WHEN** a session is created with an environment shell `(dev, "pnpm dev")`
- **THEN** a quake shell labeled `dev` SHALL spawn and run `pnpm dev`

#### Scenario: Pre-fill on re-open

- **WHEN** that session is re-opened after an app restart
- **THEN** the `dev` shell SHALL respawn with `pnpm dev` typed at the prompt, not executed, so a single Enter re-runs it

#### Scenario: Ad-hoc shells remember their last command

- **WHEN** the user opens an ad-hoc shell, runs `docker compose up`, closes Nergal, and re-opens the session
- **THEN** that tab SHALL come back with `docker compose up` pre-filled

### Requirement: Prelude vs environment shells

The launch-options `startup_command` SHALL be a prelude that runs in the agent terminal before the agent (so the agent inherits its env) and is expected to exit. Long-running commands SHALL be expressed as environment shells, never as the prelude, so they do not block the agent launch.

#### Scenario: Long-running command does not block the agent

- **WHEN** the user wants `pnpm dev` at session start
- **THEN** it SHALL be configured as an environment shell (quake), and the agent SHALL launch normally regardless of `pnpm dev` not exiting

### Requirement: Per-workspace environment-shell suggestions

Settings SHALL provide a per-workspace library of suggested environment shells `(label, command)`, persisted per workspace. At session creation, the user SHALL be able to quick-pick from the active workspace's suggestions to populate the environment-shells list.

#### Scenario: Suggestions are scoped to the workspace

- **WHEN** the user opens the new-session modal in workspace A
- **THEN** only workspace A's suggested environment shells SHALL be offered for quick-pick
