## MODIFIED Requirements

### Requirement: Environment shells

A session SHALL support a list of environment shells defined as `(label, command, cwd?)`. Each entry spawns a quake shell, in `cwd` when set (`~` expands; relative paths resolve against the workspace root — worktrees live inside the repo, so session-cwd-relative would point somewhere the user doesn't mean; a missing directory falls back to the session cwd). The command SHALL auto-run when the session is first created, and SHALL be pre-filled (typed, not executed) when the session is re-opened after an app restart. The persisted set SHALL be the session's **live tab set**: definitions seed from the new-session modal, ad-hoc tabs join it, closed tabs leave it, and a command submitted in any quake shell updates that tab's remembered command — along with the shell's working directory at submit time, when it differs from the session cwd.

#### Scenario: Auto-run on creation

- **WHEN** a session is created with an environment shell `(dev, "pnpm dev")`
- **THEN** a quake shell labeled `dev` SHALL spawn and run `pnpm dev`

#### Scenario: Pre-fill on re-open

- **WHEN** that session is re-opened after an app restart
- **THEN** the `dev` shell SHALL respawn with `pnpm dev` typed at the prompt, not executed, so a single Enter re-runs it

#### Scenario: Ad-hoc shells remember their last command

- **WHEN** the user opens an ad-hoc shell, runs `docker compose up`, closes Nergal, and re-opens the session
- **THEN** that tab SHALL come back with `docker compose up` pre-filled

#### Scenario: Shells remember their working directory

- **WHEN** the user `cd`s an ad-hoc shell to `../backend`, runs `pnpm start`, and re-opens the session after a restart
- **THEN** that tab SHALL respawn with `../backend` as its working directory and `pnpm start` pre-filled
- **AND** if the remembered directory no longer exists, the shell SHALL spawn in the session cwd instead

#### Scenario: Cwd in definitions and suggestions

- **WHEN** the user sets a cwd on an environment-shell row in the new-session modal or on a per-workspace suggestion
- **THEN** the spawned shell SHALL start in that directory, resolving `~` and relative paths against the workspace root
- **AND** the modal SHALL live-validate the directory and SHALL NOT create the session while a non-empty cwd doesn't resolve to a real directory
- **AND** quick-picking the suggestion SHALL carry its cwd into the session's definition
