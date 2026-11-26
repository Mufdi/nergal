## ADDED Requirements

### Requirement: The scanner finds OSC 7501 sequences in the PTY output

The system SHALL scan each agent session's PTY output for `ESC ] 7501 ;` followed by a body and a terminator. The terminator SHALL be BEL (`0x07`) or ST (`ESC \`). An `ESC` followed by any byte other than `\`, a CAN (`0x18`) or a SUB (`0x1a`) SHALL abort the sequence without a result, and that `ESC` SHALL be scanned again as a possible start. A body equal to `?` SHALL be a query. Any other body SHALL be a report. A body longer than 4096 bytes SHALL be discarded whole, and scanning SHALL resume after its terminator or abort. A prefix or body split across PTY reads SHALL be joined.

#### Scenario: Report split across PTY reads

- **WHEN** one PTY read ends with `ESC ] 7501 ; state=wor` and the next read starts with `king:app=claude-code ESC \`
- **THEN** the scanner SHALL yield one report with body `state=working:app=claude-code`

#### Scenario: Oversized report is discarded

- **WHEN** the output carries `ESC ] 7501 ; state=working:msg=` followed by 5000 base64 bytes and `ESC \`, then `ESC ] 7501 ; state=done ESC \`
- **THEN** the scanner SHALL yield only the report `state=done`

#### Scenario: An aborted sequence does not swallow later reports

- **WHEN** the output carries `ESC ] 7501 ; state=working`, then `ESC [ 2 J`, then `ESC ] 7501 ; state=done BEL`
- **THEN** the scanner SHALL yield only the report `state=done`

#### Scenario: CAN aborts a sequence

- **WHEN** the output carries `ESC ] 7501 ; state=working` followed by `0x18`, then `ESC ] 7501 ; ? BEL`
- **THEN** the scanner SHALL yield only one query

#### Scenario: Other OSC codes are ignored

- **WHEN** the output carries `ESC ] 8 ; ; https://x ESC \`, `ESC ] 11 ; ? BEL` and `ESC ] 75010 ; state=done BEL`
- **THEN** the scanner SHALL yield nothing

### Requirement: Reports are parsed into one root status per session

The system SHALL parse each report's fields, which are `key=value` pairs separated by `:`. The system SHALL keep, per agent session, one root status with: `state` (one of `working`, `blocked`, `done`, `idle`, `error`), `kind` (only for `blocked`), `msg`, and the time it was set. `msg` SHALL be the `msg` field decoded from base64 as UTF-8 (lossy), with control characters (C0, DEL and C1) removed. A report with an `id` field SHALL be ignored, because it describes a child entry. A report with `state=clear` and no `id` SHALL remove the session's root status. A report with an unknown `state`, or without a `state`, SHALL be ignored. A `msg` that is not valid base64 SHALL be dropped and the rest of the report kept.

#### Scenario: Working report sets the root status

- **GIVEN** agent session S has no root status
- **WHEN** S's PTY output carries `ESC ] 7501 ; state=working:app=claude-code ESC \`
- **THEN** S's root status SHALL be `state=working` with no `kind` and no `msg`

#### Scenario: Blocked report keeps kind and decoded msg

- **WHEN** S's PTY output carries `ESC ] 7501 ; state=blocked:app=claude-code:kind=permission:msg=QWxsb3cgQmFzaD8= BEL`
- **THEN** S's root status SHALL be `state=blocked`, `kind=permission`, `msg="Allow Bash?"`

#### Scenario: Control characters are removed from msg

- **WHEN** a report carries `state=blocked:msg=` with the base64 of `"Allow\u001b[31m Bash?\u0007"`
- **THEN** S's root status `msg` SHALL be `"Allow[31m Bash?"`

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

### Requirement: The root status belongs to the PTY that reported it

The system SHALL store each root status with the id of the PTY whose output carried the report. A report or a PTY EOF from a PTY that does not own the stored root status SHALL leave it unchanged, unless no root status is stored, in which case a report sets it with the reporting PTY as owner. Auxiliary shells (quake terminals) SHALL never set or clear a root status.

#### Scenario: EOF of an old PTY keeps the new PTY's status

- **GIVEN** S's root status `working` is owned by PTY B
- **WHEN** PTY A of session S reaches EOF
- **THEN** S's root status SHALL stay `working`

#### Scenario: Late report from an old PTY is ignored

- **GIVEN** S's root status `working` is owned by PTY B
- **WHEN** PTY A of session S reports `state=done`
- **THEN** S's root status SHALL stay `working`

#### Scenario: EOF of the owner removes the status

- **GIVEN** S's root status `working` is owned by PTY B
- **WHEN** PTY B reaches EOF
- **THEN** S SHALL have no root status

#### Scenario: Quake shell reports are ignored

- **WHEN** a quake shell's PTY output carries `ESC ] 7501 ; state=working ESC \`
- **THEN** no root status SHALL be set

### Requirement: Root status changes reach the frontend

The system SHALL publish a root status change for an agent session as one Tauri event `session:program-status` with payload `{session_id, state, kind, msg}`. `session_id` is the Nergal session id. `state` is the new state, or `null` when the root status was removed. `kind` and `msg` are `null` when absent. The system SHALL emit the event only when `state`, `kind` or `msg` differs from the stored value. A session ending (`SessionEnd` hook) SHALL NOT remove the root status on either side, because Claude Code clears it with `state=clear` and PTY EOF removes it.

#### Scenario: Repeated report emits once

- **WHEN** S's PTY output carries `state=working` twice in a row
- **THEN** exactly one `session:program-status` event with `state="working"` SHALL be emitted for S

#### Scenario: PTY close emits a removal

- **GIVEN** S's root status `working` is owned by PTY B
- **WHEN** PTY B reaches EOF
- **THEN** a `session:program-status` event with `state=null` SHALL be emitted for S

#### Scenario: SessionEnd keeps the status

- **GIVEN** S's root status is `idle`
- **WHEN** a `SessionEnd` hook and then a `SessionStart` hook arrive for S (as with `/clear`)
- **THEN** S's root status SHALL still be `idle` in the agent state and in the frontend
