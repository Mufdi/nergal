# config-persistence

A malformed config file is preserved and logged, never silently overwritten with defaults.

## ADDED Requirements

### Requirement: Unparseable config is backed up, not destroyed

When `config.json` exists but fails to deserialize, the system SHALL log the parse error, copy the original file to `config.json.corrupt` before falling back to defaults, and SHALL NOT treat the parse failure identically to a missing file.

#### Scenario: corrupt file is preserved

- **GIVEN** a `config.json` containing invalid JSON (e.g. truncated by a crash)
- **WHEN** the app starts and a later settings change triggers `config.save()`
- **THEN** `config.json.corrupt` contains the original pre-corruption bytes and the parse error is in the logs

#### Scenario: missing file stays silent

- **WHEN** `config.json` does not exist (fresh install)
- **THEN** defaults load, no backup file is created, and no error is logged

#### Scenario: valid file round-trips unchanged

- **WHEN** `config.json` parses successfully
- **THEN** load behaves exactly as before this change (including `theme_mode` normalization)
