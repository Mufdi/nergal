## MODIFIED Requirements

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
