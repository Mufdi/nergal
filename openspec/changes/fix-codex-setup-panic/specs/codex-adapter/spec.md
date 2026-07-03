# codex-adapter

Robust merge of nergal's hook entries into `~/.codex/hooks.json`, tolerant of malformed existing files.

## MODIFIED Requirements

### Requirement: setup_agent('codex') writes hooks.json with nergal entries

The system SHALL provide `setup_agent('codex')` that writes `~/.codex/hooks.json` with the nergal hook entries listed in design.md. Existing non-nergal hook entries SHALL be preserved (conservative merge). The merge SHALL NOT panic on a malformed existing file: when the top-level `"hooks"` key is present but holds a non-object value (e.g. an array, string, or number), the runner SHALL overwrite it with a fresh empty object and continue, rather than unwrapping a `None`.

#### Scenario: Setup on fresh Codex install

- **WHEN** `setup_agent('codex')` runs and `~/.codex/hooks.json` does not exist
- **THEN** the file SHALL be created with the full set of nergal hook entries

#### Scenario: Setup with existing user hooks

- **WHEN** `setup_agent('codex')` runs and `~/.codex/hooks.json` contains user-defined hooks under `PostToolUse` that are not nergal's
- **THEN** the nergal entries SHALL be added alongside without removing the user's
- **AND** any obsolete nergal entries from older versions SHALL be cleaned

#### Scenario: Malformed top-level hooks value is normalized

- **GIVEN** `~/.codex/hooks.json` parses to an object whose `"hooks"` key holds a non-object value (e.g. `{"hooks": []}`)
- **WHEN** `merge_nergal_entries` runs
- **THEN** it SHALL NOT panic
- **AND** the non-object `"hooks"` value SHALL be replaced with a fresh empty object
- **AND** the resulting document SHALL contain the canonical nergal entries under `hooks.<event>` for every supported event
