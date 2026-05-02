# scratchpad Specification

## Purpose
TBD - created by archiving change scratchpad-floating-panel. Update Purpose after archive.
## Requirements
### Requirement: Floating panel toggle

The scratchpad SHALL be exposed as a single floating panel mounted at the workspace root, toggled with `Ctrl+Alt+L`. The panel SHALL be draggable, resizable, and semi-transparent so the workspace remains visible behind it.

#### Scenario: Open with Ctrl+Alt+L

- **WHEN** the user presses `Ctrl+Alt+L` from any focus zone outside the terminal
- **THEN** the floating panel SHALL appear with a fade + scale-in animation under 150 ms
- **AND** focus SHALL move to the active note's editor immediately

#### Scenario: Close with Ctrl+Alt+L or Esc

- **WHEN** the panel is open and the user presses `Ctrl+Alt+L`, `Esc` while focus is inside the panel, or clicks the close icon
- **THEN** the panel SHALL animate out and unmount

#### Scenario: Click-through outside the card

- **WHEN** the panel is open
- **THEN** clicks outside the visible card SHALL pass through to the workspace below (the panel does NOT capture pointer events on its margin)

### Requirement: Auto-create initial tab

The scratchpad SHALL ensure at least one tab exists whenever the panel is opened, so the user can start typing without an explicit "create" step.

#### Scenario: Empty scratchpad on first open

- **WHEN** the user opens the panel and the scratchpad directory contains no notes
- **THEN** the system SHALL create a new note silently
- **AND** the editor SHALL receive focus

### Requirement: Multi-tab with UUID-keyed files

Each tab SHALL be backed by exactly one `.md` file in the configured scratchpad directory. The filename SHALL be `scratch-{uuid}.md` where `{uuid}` is a UUID v4. The tab id SHALL equal the UUID and SHALL persist across path changes and external renames.

#### Scenario: Create new tab

- **WHEN** the user clicks the `+` button or presses `Ctrl+T` while the panel is focused
- **THEN** a new UUID v4 SHALL be generated
- **AND** an empty file `scratch-{uuid}.md` SHALL be created in the scratchpad directory
- **AND** the new tab SHALL become active

#### Scenario: Display name derived from position

- **WHEN** the tab list is rendered
- **THEN** each tab SHALL display the name `Scratch N` where `N = position + 1` in the current ordering
- **AND** closing the middle tab SHALL renumber the remaining tabs in the UI without renaming any file

### Requirement: Atomic autosave

Edits SHALL be persisted via an atomic `tmp + rename` pattern, with the temp file living in the same directory as the target so `rename(2)` is atomic regardless of filesystem.

#### Scenario: Debounced autosave

- **WHEN** the user types in an active tab
- **THEN** the editor SHALL flush the buffer to disk after 300 ms of inactivity
- **AND** the file SHALL never be observed in a partially written state

#### Scenario: Crash recovery

- **WHEN** the application starts
- **THEN** any orphan `.scratch-{uuid}.md.tmp` files in the scratchpad directory SHALL be removed

### Requirement: Soft-delete with epoch purge

Closing a tab SHALL move its file to the `.trash/` subdirectory with the deletion timestamp (Unix epoch in milliseconds) embedded in the filename. The trash SHALL be purged on application startup, removing files older than 30 days based on the embedded epoch.

#### Scenario: Close tab moves to trash

- **WHEN** the user closes a tab
- **THEN** the file `scratch-{uuid}.md` SHALL be renamed to `.trash/scratch-{uuid}-trashed-{epoch_ms}.md`
- **AND** the corresponding `scratchpad_meta` row SHALL be deleted

#### Scenario: Restore last closed tab

- **WHEN** the panel is focused and the user presses `Ctrl+Shift+T`
- **AND** the in-memory close stack contains at least one tab id
- **AND** the matching `.trash/scratch-{uuid}-trashed-*.md` file still exists
- **THEN** the most recently trashed copy SHALL be renamed back to `scratch-{uuid}.md`
- **AND** the restored tab SHALL become active

#### Scenario: Purge expired notes on startup

- **WHEN** the application starts
- **THEN** the system SHALL scan `.trash/` and delete any file whose embedded `epoch_ms` is older than 30 days
- **AND** the system SHALL log the count of purged files

### Requirement: Path validation and symlink rejection

All filesystem operations SHALL refuse paths that escape the configured scratchpad directory and SHALL reject symlinks on every read, write, list, and watcher event.

#### Scenario: Reject path traversal

- **WHEN** any operation receives a target path whose canonical form is not contained in the canonical scratchpad directory
- **THEN** the operation SHALL fail with an explicit error and SHALL NOT touch the filesystem

#### Scenario: Reject symlink in scratchpad directory

- **WHEN** an entry inside the scratchpad directory matches the `scratch-{uuid}.md` pattern but is a symlink
- **THEN** `scratchpad_list_tabs` SHALL omit it
- **AND** the watcher SHALL ignore notify events for it
- **AND** `scratchpad_read_tab` and `scratchpad_write_tab` SHALL fail with an explicit error

### Requirement: Filesystem watcher with own-write tracking

The system SHALL watch the scratchpad directory non-recursively with a 200 ms debounce. The watcher SHALL filter to the canonical filename pattern, exclude `.trash/` and dotfiles, and ignore files larger than 1 MB. Own-writes (writes performed by the editor itself) SHALL be suppressed via a per-file ring buffer of the last 8 SHA-256 hashes.

#### Scenario: External edit refreshes a clean buffer

- **WHEN** an external tool modifies `scratch-{uuid}.md`
- **AND** the corresponding tab's in-memory buffer is not dirty
- **THEN** the system SHALL emit `scratchpad:tab-changed`
- **AND** the editor SHALL replace its buffer with the on-disk content

#### Scenario: External edit on a dirty buffer raises a soft conflict

- **WHEN** an external tool modifies `scratch-{uuid}.md`
- **AND** the corresponding tab's in-memory buffer has unsaved edits
- **THEN** the editor SHALL display a non-blocking conflict banner
- **AND** the in-memory buffer SHALL NOT be replaced

#### Scenario: Own-writes are suppressed

- **WHEN** the editor performs an autosave that triggers a notify event
- **AND** the on-disk SHA-256 matches one of the last 8 own-write hashes for the file
- **THEN** the watcher SHALL NOT emit `scratchpad:tab-changed` for the event

### Requirement: Reusable floating panel chrome

The drag, resize, opacity, and geometry persistence SHALL be implemented as a generic `FloatingPanel` component keyed by `panel_id`. Geometry persistence SHALL use a multi-row table so that future floating tools can reuse the chrome without schema migration.

#### Scenario: Geometry persists across restarts

- **WHEN** the user moves or resizes the panel
- **THEN** the new geometry SHALL be persisted in `floating_panel_geometry` keyed by `panel_id = 'scratchpad'`
- **AND** the next time the panel opens, the persisted geometry SHALL be restored

#### Scenario: Off-screen geometry is clamped on load

- **WHEN** the persisted geometry would place the panel partially or fully outside the current viewport
- **THEN** the system SHALL reset the panel to a centered default and persist the new geometry on the next interaction

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
- **THEN** the path SHALL be set to `~/.config/cluihud/scratchpad/`

#### Scenario: Reveal in file manager

- **WHEN** the user clicks "Reveal in file manager" in settings
- **THEN** the system SHALL spawn `xdg-open` with the canonical path as a single argument (never as a shell-interpreted string)

### Requirement: No coupling with sessions or workspaces

The scratchpad store SHALL NOT import session, workspace, or right-panel state atoms. The scratchpad SHALL remain global, cross-project, and cross-session.

#### Scenario: Switching active session does not affect scratchpad

- **WHEN** the user switches between sessions or workspaces
- **THEN** the scratchpad tab list, active tab, geometry, and content SHALL remain unchanged

### Requirement: Selection seam for future send-to-prompt

The active editor SHALL publish its current selection to a Jotai atom on every selection change. This SHALL exist in v1 as the integration seam for a future send-to-prompt adapter, without coupling the scratchpad store to session state.

#### Scenario: Selection updates atom on change

- **WHEN** the user changes the editor selection in the active scratchpad tab
- **THEN** the `currentScratchpadSelectionAtom` SHALL be updated with the selected text

