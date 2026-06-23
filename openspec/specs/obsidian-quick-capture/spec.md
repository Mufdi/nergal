# obsidian-quick-capture Specification

## Purpose
TBD - created by archiving change obsidian-bridge. Update Purpose after archive.
## Requirements
### Requirement: Quick capture floating panel
The system SHALL expose a global keyboard shortcut `Ctrl+Alt+Q` that toggles a floating panel for fast-thought capture. The panel MUST mount the existing `FloatingPanel` chrome with `panelId = "quick-capture"`, MUST persist geometry via the existing `floating_panel_geometry` SQLite table, and MUST render a single textarea with a hint row showing the submission affordances ("Enter to save · Shift+Enter for newline · Esc to cancel").

#### Scenario: Open and save

- **WHEN** the user presses `Ctrl+Alt+Q` while the active workspace has a configured `quick_capture_path`
- **THEN** the panel SHALL appear with the textarea focused
- **WHEN** the user types text and presses `Enter` (no Shift modifier)
- **THEN** the system SHALL append the text to the configured file with a leading `## <ISO timestamp>` header and a trailing `#<tag>` line (tag default `nergal-inbox`)
- **AND** the panel SHALL close with a Sileo toast "Captured to <basename>"

#### Scenario: Cancel without save

- **WHEN** the user presses `Esc` while the panel is open
- **THEN** the panel SHALL close without writing anything
- **AND** the panel's geometry SHALL be preserved for the next open

#### Scenario: Multiline capture

- **WHEN** the user presses `Shift+Enter` inside the textarea
- **THEN** a newline SHALL be inserted in the buffer
- **AND** `Enter` (no Shift) SHALL submit the entire multiline buffer as one append

### Requirement: Append format and atomicity
Each capture SHALL append exactly the following block to the configured file (in order, ending with a newline):

```
\n\n## <ISO 8601 timestamp>\n<typed text>\n\n#<tag>\n
```

The write MUST use `O_APPEND` and MUST `fsync` before returning success.

#### Scenario: Concurrent captures from two workspaces

- **WHEN** two workspaces share the same `quick_capture_path`
- **AND** the user captures from each within milliseconds
- **THEN** both blocks SHALL appear in the file in submission order
- **AND** neither block SHALL be truncated or interleaved at the byte level

### Requirement: Disabled state when no channel configured
When `vault_root` is set but `quick_capture_path` is null, the system SHALL still register the shortcut handler but SHALL surface a Sileo toast "Set Quick Capture path in Settings → Obsidian Integration" instead of opening the panel.

#### Scenario: Shortcut with no capture path

- **WHEN** the user presses `Ctrl+Alt+Q` and `quick_capture_path` is null
- **THEN** no panel SHALL open
- **AND** the toast SHALL appear pointing at the setting

