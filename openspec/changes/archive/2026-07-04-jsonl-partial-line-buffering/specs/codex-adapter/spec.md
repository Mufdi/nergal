# codex-adapter

Rollout tail never drops a record split across reads.

## ADDED Requirements

### Requirement: Rollout tail holds the offset at the last terminated line

The Codex rollout tail SHALL advance its read offset only past fully newline-terminated lines. When a read ends mid-line, the partial tail SHALL remain un-consumed and be re-read complete on the next modify event; a read ending mid-UTF-8-sequence SHALL NOT discard the preceding complete lines.

#### Scenario: envelope split across two reads

- **GIVEN** a modify event fires while Codex has written only part of a rollout envelope line
- **WHEN** the tail reads, then Codex completes the line and a second read runs
- **THEN** the envelope is ingested exactly once and no status update is lost
