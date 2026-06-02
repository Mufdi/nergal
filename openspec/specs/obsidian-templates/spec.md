# obsidian-templates Specification

## Purpose
TBD - created by archiving change obsidian-bridge. Update Purpose after archive.
## Requirements
### Requirement: Templates folder watcher
The system SHALL watch the `templates_path` directory configured in `obsidian_config` using a `notify` filesystem watcher with a 200 ms debounce window. The watcher MUST: (a) start when `templates_path` becomes non-null, (b) be torn down and re-spawned when `templates_path` changes, (c) be dropped when `templates_path` is cleared. On every change event, the watcher SHALL re-scan the folder for files matching `template-*.md`, parse each (frontmatter + body), and emit a Tauri event `obsidian:templates-updated` with the resolved list.

The watcher SHALL be path-rooted (single directory, non-recursive). Files in subdirectories of `templates_path` SHALL NOT be discovered.

#### Scenario: Watcher starts on config save

- **WHEN** the user sets `templates_path` for the first time and saves Settings
- **THEN** the watcher SHALL spawn on that directory
- **AND** an initial `obsidian:templates-updated` event SHALL be emitted with the current list

#### Scenario: New template file is created externally

- **WHEN** the user creates `<templates_path>/template-tdd.md` from Obsidian or any editor
- **AND** the file is saved
- **THEN** within 250 ms the system SHALL emit `obsidian:templates-updated` with the new entry included

#### Scenario: Templates path changed

- **WHEN** the user changes `templates_path` to a different directory and saves
- **THEN** the watcher on the old path SHALL be dropped
- **AND** a new watcher on the new path SHALL spawn
- **AND** `obsidian:templates-updated` SHALL be emitted reflecting the new directory's contents

### Requirement: Template file format
A template file MUST: (a) have a filename matching `template-*.md`, (b) optionally start with YAML frontmatter exposing `name` (string) and/or `description` (string), (c) treat the remainder of the file as the body to send. The system SHALL strip the frontmatter (if any) before sending; the body SHALL be sent as-is including newlines.

If `name` is absent in frontmatter, the system SHALL derive a display name by stripping `template-` and `.md` from the filename and replacing dashes with spaces.

#### Scenario: Template with frontmatter

- **WHEN** the file `template-tdd.md` contains:
```
---
name: "TDD refactor"
description: "Test-driven refactor for the focused file"
---
Refactor the selected file following TDD…
```
- **THEN** the resolved entry SHALL have `name = "TDD refactor"`, `description = "Test-driven refactor for the focused file"`, and `body` SHALL be the text after the frontmatter (no leading newlines added or stripped)

#### Scenario: Template without frontmatter

- **WHEN** the file `template-explore-tradeoffs.md` contains just markdown body
- **THEN** the resolved entry SHALL have `name = "explore tradeoffs"` and `description = null`
- **AND** the entire file content SHALL be the body

### Requirement: Command palette integration
The command palette SHALL surface each discovered template as a command entry in a new category `templates` (sorting after `action`). Each entry MUST: (a) have id `template-<filename>`, (b) label `Send template: <name>`, (c) include `<description>` in the keywords for search, (d) no keyboard shortcut, (e) on activation, write the template body to the active session's PTY using `write_to_session_pty`, terminating with a `\r` so the prompt is submitted when the session is at the input.

If no session is active, the command SHALL be visible but disabled (greyed out, no-op on click with a tooltip).

#### Scenario: Send a template

- **WHEN** the user opens the command palette and selects "Send template: TDD refactor"
- **AND** an active session exists
- **THEN** the system SHALL invoke `write_to_session_pty(session_id, body + "\r")`
- **AND** the palette SHALL close

#### Scenario: No active session

- **WHEN** the user invokes a template command with no active session
- **THEN** the command SHALL no-op
- **AND** a Sileo toast SHALL surface "Select an active session to send a template"

#### Scenario: Templates path unset

- **WHEN** `templates_path` is null in the resolved config
- **THEN** no template entries SHALL appear in the palette

