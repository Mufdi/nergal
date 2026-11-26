## ADDED Requirements

### Requirement: The emulator answers the OSC 7501 query

The emulator SHALL answer the Program Status Protocol query `ESC ] 7501 ; ? <terminator>` (terminator BEL or ST) by writing `ESC ] 7501 ; ? ESC \` to the PTY, in every terminal Nergal runs. The answer SHALL reach the PTY before any answer that wezterm writes for queries that follow the OSC 7501 query in the same PTY read, because Claude Code treats the protocol as unsupported when its DA1 answer arrives first. The query SHALL also be recognized when it is split across two PTY reads.

#### Scenario: Query is answered

- **WHEN** the PTY output carries `ESC ] 7501 ; ? ESC \`
- **THEN** the emulator SHALL write exactly `ESC ] 7501 ; ? ESC \` to the PTY

#### Scenario: Answer precedes the DA1 answer

- **WHEN** one PTY read carries `ESC ] 7501 ; ? ESC \` followed by `ESC [ c`
- **THEN** the bytes written to the PTY SHALL start with `ESC ] 7501 ; ? ESC \` and be followed by the DA1 answer `ESC [ ? 65 ; 4 ; 6 ; 18 ; 22 c`

#### Scenario: Split query is answered

- **WHEN** one PTY read ends with `ESC ] 75` and the next starts with `01 ; ? BEL`
- **THEN** the emulator SHALL write `ESC ] 7501 ; ? ESC \` to the PTY once

#### Scenario: A status report is not a query

- **WHEN** the PTY output carries `ESC ] 7501 ; state=working ESC \`
- **THEN** the emulator SHALL write nothing to the PTY

### Requirement: OSC 7501 sequences are never drawn

The emulator SHALL consume OSC 7501 queries and reports without drawing any of their bytes in the grid, and SHALL leave the cursor where it was.

#### Scenario: Report leaves the grid unchanged

- **GIVEN** the cursor is at column 0 of an empty row
- **WHEN** the PTY output carries `ESC ] 7501 ; state=blocked:kind=question:msg=SGk= ESC \` followed by `ok`
- **THEN** the row SHALL read `ok` starting at column 0
