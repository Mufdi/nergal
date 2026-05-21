# pi-adapter — Delta

## ADDED Requirements

### Requirement: pi adapter advertises and implements THEME_SYNC

The pi adapter SHALL declare `AgentCapability::THEME_SYNC` in its capabilities bitset and SHALL implement `apply_theme(palette)` such that:

1. A 51-token theme JSON derived from `palette` is written atomically to `~/.pi/agent/themes/cluihud-active.json`. The file SHALL satisfy pi's documented theme schema (`https://raw.githubusercontent.com/badlogic/pi-mono/main/packages/coding-agent/src/modes/interactive/theme/theme-schema.json`) with `"name": "cluihud-active"`.
2. `~/.pi/agent/settings.json` SHALL be reconciled: if the `"theme"` key is absent OR equals `"cluihud-active"`, set it to `"cluihud-active"`; otherwise leave it untouched (user's selection wins). Other keys in settings.json SHALL be preserved.
3. The directory `~/.pi/agent/themes/` SHALL be created if missing.
4. No PTY messages or signals SHALL be sent to running pi sessions. pi's documented hot-reload picks up the file change live.

#### Scenario: Apply theme creates and updates the cluihud-active.json file

- **WHEN** `apply_theme(palette)` is called and `~/.pi/agent/themes/cluihud-active.json` does not exist
- **THEN** the file SHALL be created with all 51 required color tokens populated
- **AND** the `name` field SHALL be `"cluihud-active"`
- **AND** the colors SHALL be derived from `palette` per the mapping documented in `implementation.md`

#### Scenario: Subsequent calls overwrite the theme file

- **WHEN** `apply_theme(palette_v2)` is called after a previous `apply_theme(palette_v1)`
- **THEN** `cluihud-active.json` SHALL be overwritten with the new palette
- **AND** the write SHALL be atomic (write-temp-then-rename) to prevent torn reads by pi's hot-reloader

#### Scenario: User-selected theme is preserved

- **WHEN** `~/.pi/agent/settings.json` contains `"theme": "my-custom"` at the time `apply_theme` is called
- **THEN** the `theme` key in settings.json SHALL remain `"my-custom"`
- **AND** `cluihud-active.json` SHALL still be written (so a future user switch to `cluihud-active` works immediately)
- **AND** the registry SHALL log at debug level that the user's selection took precedence

#### Scenario: Other settings.json keys are preserved

- **WHEN** `apply_theme` reconciles `~/.pi/agent/settings.json`
- **THEN** unrelated keys (`defaultProvider`, `defaultModel`, `lastChangelogVersion`, etc.) SHALL remain unchanged
- **AND** the write SHALL be atomic

#### Scenario: Missing settings.json is created

- **WHEN** `apply_theme` is called and `~/.pi/agent/settings.json` does not exist
- **THEN** the file SHALL be created with `{ "theme": "cluihud-active" }` as its sole content
