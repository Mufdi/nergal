## ADDED Requirements

### Requirement: Descriptor mode follows the reported root status

For a live session that has a root status (`program-status-protocol`), the descriptor's `mode` and `waiting_for` SHALL combine it with the hook-derived activity, and the `mode` values SHALL stay `idle`, `running`, `needs_attention` and `completed`. `mode` SHALL be `needs_attention` when the root status is `blocked` or the hook-derived mode is `needs_attention`. Otherwise `mode` SHALL be `running` for root status `working`, and `idle` for `done`, `idle` and `error`, because a session in those states accepts a new prompt. When the root status is `blocked`, `waiting_for` SHALL be its `msg` cut to 200 characters, or, without a `msg`, `permission`, `question`, `login` or `dialog` for `kind` `permission`, `question`, `auth` or none. When only the hook-derived mode is `needs_attention`, `waiting_for` SHALL keep the hook value. A live session without a root status SHALL keep the hook-derived `mode` and `waiting_for`.

#### Scenario: Working after Stop reports running

- **GIVEN** live session S got a `Stop` hook (hook-derived mode `idle`)
- **WHEN** S's root status is `working`
- **THEN** the descriptor SHALL report S with `mode` `running` and no `waiting_for`

#### Scenario: Folder-trust prompt reports needs_attention

- **WHEN** S's root status is `blocked` with no `kind` and no `msg`
- **THEN** the descriptor SHALL report S with `mode` `needs_attention` and `waiting_for` `dialog`

#### Scenario: Blocked msg becomes waiting_for

- **WHEN** S's root status is `blocked` with `kind=permission` and `msg="Allow Bash?"`
- **THEN** the descriptor SHALL report S with `mode` `needs_attention` and `waiting_for` `Allow Bash?`

#### Scenario: Error reports idle

- **WHEN** S's root status is `error`
- **THEN** the descriptor SHALL report S with `mode` `idle`

#### Scenario: Background agent wait wins over a done status

- **GIVEN** S's hook-derived mode is `needs_attention` with `waiting_for` starting with `background agent`
- **WHEN** S's root status is `done`
- **THEN** the descriptor SHALL report S with `mode` `needs_attention` and the hook `waiting_for`

#### Scenario: No root status keeps hook mode

- **GIVEN** S never sent an OSC 7501 report and its hook-derived mode is `running`
- **THEN** the descriptor SHALL report S with `mode` `running`
