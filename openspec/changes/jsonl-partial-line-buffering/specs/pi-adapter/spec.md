# pi-adapter

Tail reader never drops a record split across reads.

## MODIFIED Requirements

### Requirement: Tail-f semantics with offset tracking

The JSONL tail SHALL read existing content on first open (catch-up), then continue reading from the last offset on each notify modify event. Lines SHALL NOT be re-emitted across modify events. The offset SHALL advance only past fully newline-terminated lines: when a read ends mid-line (the writer was mid-append), the partial tail's bytes SHALL remain un-consumed so the next read re-reads the completed line. A read ending mid-UTF-8-sequence SHALL NOT discard the preceding complete lines.

#### Scenario: Catch-up on open

- **WHEN** the JSONL file already contains 50 lines when the tail opens
- **THEN** the tail SHALL parse and emit events for all 50 lines once
- **AND** SHALL track offset = position after the last terminated line at end of catch-up

#### Scenario: Append-only follow

- **WHEN** Pi appends a new line to the JSONL after the tail is initialized
- **THEN** notify SHALL emit a modify event
- **AND** the tail SHALL read from `last_offset` to EOF
- **AND** emit one event per new complete line, then update `last_offset`

#### Scenario: Record split across two reads is emitted once, intact

- **GIVEN** a modify event fires while Pi has written only half of a JSONL record
- **WHEN** the tail reads (partial line: nothing emitted, offset held) and Pi completes the record triggering a second read
- **THEN** the completed record is parsed and emitted exactly once
