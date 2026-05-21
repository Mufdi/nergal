# opencode-adapter — Delta

## ADDED Requirements

### Requirement: opencode adapter advertises and implements THEME_SYNC

The opencode adapter SHALL declare `AgentCapability::THEME_SYNC` in its capabilities bitset and SHALL implement `apply_theme(palette)` such that:

1. A theme JSON derived from `palette` is written atomically to `~/.config/opencode/themes/cluihud-active.json`. The file SHALL declare `"$schema": "https://opencode.ai/theme.json"` and SHALL include `defs` + a `theme` map covering at minimum the tokens: `background`, `text`, `border`, `accent`, `primary`, `secondary`, `success`, `error`, `warning`, `info`.
2. `~/.config/opencode/tui.json` SHALL be reconciled: if `"theme"` is absent or already `"cluihud-active"`, set it to `"cluihud-active"`; otherwise leave it untouched.
3. The directory `~/.config/opencode/themes/` SHALL be created if missing.
4. For each opencode session whose port is registered in the adapter's `session_ports` map, the adapter SHOULD POST `{"command": "theme cluihud-active"}` to `http://127.0.0.1:<port>/tui/execute-command` with a 1.5s timeout. The implementation MAY skip this step if the implementation spike (recorded in `handoff/spike-opencode-live-switch.md`) concludes the command syntax is not supported live.

#### Scenario: Apply theme writes the JSON file

- **WHEN** `apply_theme(palette)` is called
- **THEN** `~/.config/opencode/themes/cluihud-active.json` SHALL be written atomically
- **AND** the JSON SHALL parse against the opencode theme schema
- **AND** the `defs` section SHALL include hex values derived from `palette`

#### Scenario: tui.json reconciliation respects user choice

- **WHEN** `~/.config/opencode/tui.json` contains `"theme": "tokyonight"`
- **THEN** the `theme` key SHALL remain `"tokyonight"`
- **AND** `cluihud-active.json` SHALL still be written

#### Scenario: Live HTTP switch is best-effort

- **WHEN** the live-switch POST to `/tui/execute-command` times out, returns a non-2xx, or fails to connect
- **THEN** `apply_theme` SHALL still return `Ok(())`
- **AND** the next opencode session spawn SHALL pick up the theme via `tui.json`

#### Scenario: No running opencode sessions

- **WHEN** `apply_theme` is called and `session_ports` is empty
- **THEN** no HTTP POST SHALL be attempted
- **AND** the file writes SHALL still occur (so the next spawn picks the theme up)
