## MODIFIED Requirements

### Requirement: Configurable scratchpad path

The scratchpad directory path SHALL be configurable via the application settings under the key `scratchpad_path`. Changing the path SHALL flush any pending autosaves at the old location, close all open tabs, and reload the listing from the new directory.

#### Scenario: Change path via settings

- **WHEN** the user enters a new path in the settings panel and clicks Apply
- **THEN** the system SHALL canonicalize the new path and create the directory if missing
- **AND** any in-flight autosave SHALL be flushed to the old location before tabs close
- **AND** the new path SHALL be persisted to `config.json` under `scratchpad_path`
- **AND** the watcher SHALL be replaced to point at the new path

#### Scenario: Reset to default

- **WHEN** the user clicks "Reset to default" in settings
- **THEN** the path SHALL be set to `~/.config/nergal/scratchpad/`

#### Scenario: Reveal in file manager

- **WHEN** the user clicks "Reveal in file manager" in settings
- **THEN** the system SHALL open the native file manager with the scratchpad directory highlighted via the platform desktop integration layer (`opener::reveal_item_in_dir`), without invoking any OS-specific subprocess directly
