## ADDED Requirements

### Requirement: Descriptor mode follows the reported root status

For a live session that has a root status (`program-status-protocol`), the descriptor's `mode` and `waiting_for` SHALL combine it with the hook-derived activity, and the `mode` values SHALL stay `idle`, `running`, `needs_attention` and `completed`. `mode` SHALL be `needs_attention` when the root status is `blocked` or the hook-derived mode is `needs_attention`. Otherwise `mode` SHALL be `running` for root status `working`, and `idle` for `done`, `idle` and `error`, because a session in those states accepts a new prompt. When the root status is `blocked`, `waiting_for` SHALL be its `msg` cut to 200 characters, or, without a `msg`, `permission`, `question`, `login` or `dialog` for `kind` `permission`, `question`, `auth` or none. When only the hook-derived mode is `needs_attention`, `waiting_for` SHALL keep the hook value. A live session without a root status SHALL keep the hook-derived `mode` and `waiting_for`. `last_activity` SHALL be the later of the hook activity time and the time the root status was set; a root status SHALL NOT create or change the hook-derived activity entry.

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

#### Scenario: Root status without hook activity

- **GIVEN** live session S has no hook-derived activity (no hook since the daemon started)
- **WHEN** S's root status `blocked` was set at time 1000
- **THEN** the descriptor SHALL report S with `mode` `needs_attention` and `last_activity` 1000
- **AND** the hook-derived activity of S SHALL still be absent

#### Scenario: No root status keeps hook mode

- **GIVEN** S never sent an OSC 7501 report and its hook-derived mode is `running`
- **THEN** the descriptor SHALL report S with `mode` `running`
