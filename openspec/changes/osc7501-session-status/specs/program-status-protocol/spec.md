## ADDED Requirements

### Requirement: Status reports are parsed into one root status per session

The system SHALL parse every OSC 7501 report that an agent session's PTY output carries. A report has the form `ESC ] 7501 ; <fields> <terminator>`, where the terminator is BEL (`0x07`) or ST (`ESC \`) and the fields are `key=value` pairs separated by `:`. The system SHALL keep, per agent session, one root status with: `state` (one of `working`, `blocked`, `done`, `idle`, `error`), `kind` (the `kind` field, only for `blocked`), and `msg` (the `msg` field decoded from base64 as UTF-8, lossy). A report with an `id` field SHALL be ignored, because it describes a child entry. A report with `state=clear` and no `id` SHALL remove the session's root status. A report with an unknown `state`, or without a `state`, SHALL be ignored. A `msg` that is not valid base64 SHALL be dropped and the rest of the report kept. A report body longer than 4096 bytes SHALL be discarded whole, and parsing SHALL resume after its terminator.

#### Scenario: Working report sets the root status

- **GIVEN** agent session S has no root status
- **WHEN** S's PTY output carries `ESC ] 7501 ; state=working:app=claude-code ESC \`
- **THEN** S's root status SHALL be `state=working` with no `kind` and no `msg`

#### Scenario: Blocked report keeps kind and decoded msg

- **WHEN** S's PTY output carries `ESC ] 7501 ; state=blocked:app=claude-code:kind=permission:msg=QWxsb3cgQmFzaD8= BEL`
- **THEN** S's root status SHALL be `state=blocked`, `kind=permission`, `msg="Allow Bash?"`

#### Scenario: Child report is ignored

- **GIVEN** S's root status is `done`
- **WHEN** S's PTY output carries `ESC ] 7501 ; state=working:app=claude-code:id=task-1 ESC \`
- **THEN** S's root status SHALL stay `done`

#### Scenario: Clear removes the root status

- **GIVEN** S's root status is `working`
- **WHEN** S's PTY output carries `ESC ] 7501 ; state=clear ESC \`
- **THEN** S SHALL have no root status

#### Scenario: Clear for a child keeps the root status

- **GIVEN** S's root status is `working`
- **WHEN** S's PTY output carries `ESC ] 7501 ; state=clear:id=task-1 ESC \`
- **THEN** S's root status SHALL stay `working`

#### Scenario: Unknown state is ignored

- **GIVEN** S's root status is `idle`
- **WHEN** S's PTY output carries `ESC ] 7501 ; state=paused:app=claude-code ESC \`
- **THEN** S's root status SHALL stay `idle`

#### Scenario: Invalid msg is dropped

- **WHEN** S's PTY output carries `ESC ] 7501 ; state=error:app=claude-code:msg=!!! ESC \`
- **THEN** S's root status SHALL be `state=error` with no `msg`

#### Scenario: Report split across PTY reads

- **WHEN** one PTY read ends with `ESC ] 7501 ; state=wor` and the next read starts with `king:app=claude-code ESC \`
- **THEN** S's root status SHALL be `state=working`

#### Scenario: Oversized report is discarded

- **GIVEN** S's root status is `idle`
- **WHEN** S's PTY output carries `ESC ] 7501 ; state=working:msg=` followed by 5000 base64 bytes and `ESC \`, then `ESC ] 7501 ; state=done ESC \`
- **THEN** S's root status SHALL be `done`, never `working`

### Requirement: Root status changes reach the frontend and the MCP directory

The system SHALL publish a root status change for an agent session as one Tauri event `session:program-status` with payload `{session_id, state, kind, msg}`, where `session_id` is the Nergal session id, `state` is the new state or `null` when the root status was removed, and `kind` and `msg` are `null` when absent. The system SHALL emit the event only when the root status differs from the previous one. The system SHALL store the root status in the agent state so the MCP directory reads it. The root status SHALL be removed, with an event whose `state` is `null`, when the session's PTY closes. Auxiliary shells (quake terminals) SHALL never produce root status or events.

#### Scenario: Repeated report emits once

- **WHEN** S's PTY output carries `state=working` twice in a row
- **THEN** exactly one `session:program-status` event with `state="working"` SHALL be emitted for S

#### Scenario: PTY close removes the status

- **GIVEN** S's root status is `working`
- **WHEN** S's PTY reaches EOF
- **THEN** a `session:program-status` event with `state=null` SHALL be emitted for S
- **AND** the agent state SHALL hold no root status for S

#### Scenario: Quake shell reports are ignored

- **WHEN** a quake shell's PTY output carries `ESC ] 7501 ; state=working ESC \`
- **THEN** no `session:program-status` event SHALL be emitted
