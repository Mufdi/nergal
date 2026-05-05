# terminal-wezterm Specification

## Purpose
TBD - created by archiving change replace-xterm-with-wezterm. Update Purpose after archive.
## Requirements
### Requirement: Backend-owned VT emulation
The terminal emulation for each active session SHALL be performed by `wezterm-term` in the Rust backend. The frontend SHALL NOT parse raw terminal bytes.

#### Scenario: PTY bytes are fed to wezterm-term
- **WHEN** the PTY reader thread reads a chunk of bytes from a session's master fd
- **THEN** the chunk SHALL be passed to that session's `wezterm_term::Terminal` via `advance_bytes`
- **AND** no raw terminal bytes SHALL be emitted to the frontend for that session

#### Scenario: One terminal per session
- **WHEN** a session starts
- **THEN** a dedicated `wezterm_term::Terminal` instance SHALL be created for it
- **AND** when the session ends, that instance SHALL be dropped

### Requirement: Grid-update IPC event
The backend SHALL emit a `terminal:grid-update` event with changed rows, cursor position, title, and scroll offset, instead of raw PTY bytes.

#### Scenario: Only changed rows are emitted
- **WHEN** terminal state advances and at least one row has changed since the last emission
- **THEN** the backend SHALL emit `terminal:grid-update` containing only the rows whose content changed
- **AND** rows that did not change SHALL NOT be included

#### Scenario: Updates are coalesced
- **WHEN** multiple byte chunks arrive within an 8ms window
- **THEN** the backend SHALL emit a single `terminal:grid-update` at the end of the window
- **AND** the update SHALL reflect the final state after all chunks were processed

#### Scenario: Full-grid request on mount
- **WHEN** the frontend calls `terminal_get_full_grid(session_id)`
- **THEN** the backend SHALL return a `GridUpdate` containing all non-empty rows of the current grid
- **AND** the frontend SHALL be able to use this to render the terminal from scratch

### Requirement: Input encoding via wezterm
User key events SHALL be encoded to terminal bytes using `wezterm-term`'s input encoder in the backend. The frontend SHALL NOT encode key events locally.

#### Scenario: Key event sent to backend
- **WHEN** the user presses a key while the terminal has focus
- **THEN** the frontend SHALL build a `TerminalKeyEvent` from the `KeyboardEvent` and invoke `terminal_input(session_id, event)`
- **AND** the frontend SHALL NOT attempt to compute the terminal byte representation locally

#### Scenario: Backend encodes and writes to PTY
- **WHEN** `terminal_input` is invoked
- **THEN** the backend SHALL map the event to `wezterm_term::KeyCode` + `Modifiers`
- **AND** SHALL call `Terminal::key_down`, which SHALL encode and write bytes to the PTY writer

#### Scenario: Ctrl+Backspace encodes distinctly from Backspace
- **WHEN** the user presses Ctrl+Backspace with Kitty keyboard protocol enabled
- **THEN** the bytes written to the PTY SHALL include the Ctrl modifier in the encoding (per Kitty protocol CSI-u)
- **AND** SHALL be distinguishable from the bytes produced by plain Backspace

### Requirement: Kitty keyboard protocol enabled by default
The terminal SHALL enable the Kitty keyboard protocol by default, with an opt-out via config.

#### Scenario: Default-on Kitty protocol
- **WHEN** a session starts with no explicit config override
- **THEN** the terminal SHALL be configured with `enable_kitty_keyboard = true`

#### Scenario: User opt-out
- **WHEN** `terminal.kitty_keyboard = false` is set in `config.toml`
- **THEN** new sessions SHALL start with `enable_kitty_keyboard = false`

### Requirement: Canvas-based frontend renderer
The frontend SHALL render the grid using HTML `<canvas>` with a pre-generated glyph atlas.

#### Scenario: Cells drawn from atlas
- **WHEN** a `terminal:grid-update` arrives
- **THEN** changed cells SHALL be drawn by blitting glyph rectangles from the OffscreenCanvas atlas to the visible canvas
- **AND** glyphs not in the atlas SHALL fall back to direct `fillText` rendering

#### Scenario: Atlas regenerates on theme or font change
- **WHEN** the user changes the terminal font, font size, or color theme
- **THEN** the atlas SHALL be regenerated before the next render frame

### Requirement: Preserve input flow and hook system
The externally observable behavior of input flow (user keystroke → bytes reach the shell/claude) and the hook system SHALL remain unchanged.

#### Scenario: Same bytes reach the shell
- **WHEN** the user types a sequence of printable characters in the new terminal
- **THEN** the bytes written to the PTY SHALL match what the legacy xterm.js-based terminal produced, modulo any Kitty protocol opt-in differences

#### Scenario: Hooks continue to fire
- **WHEN** Claude emits a hook event during a session on the new terminal
- **THEN** the hook SHALL reach the hook socket server and be delivered to atom state identically to the legacy path
- **AND** no hook event SHALL be lost or reordered

### Requirement: Selection and clipboard
The terminal SHALL support mouse-drag selection, keyboard copy, and bracketed paste.

#### Scenario: Drag to select
- **WHEN** the user mouse-downs on a cell and drags to another cell
- **THEN** the range SHALL be visually highlighted
- **AND** on mouseup the selection SHALL remain highlighted until a new selection is started or input clears it

#### Scenario: Copy selection to clipboard
- **WHEN** the user presses Ctrl+Shift+C (or the configured copy shortcut) with an active selection
- **THEN** the selected cells SHALL be serialized to text and written to the system clipboard

#### Scenario: Bracketed paste
- **WHEN** the user presses Ctrl+Shift+V
- **THEN** the clipboard text SHALL be read and sent to the PTY wrapped in `\x1b[200~...\x1b[201~` sequences

### Requirement: OSC 8 hyperlinks
Cells carrying OSC 8 hyperlink metadata SHALL be rendered as clickable links.

#### Scenario: Hyperlinked cell renders underlined
- **WHEN** a cell's `hyperlink` field is present
- **THEN** the cell SHALL be rendered with an underline style

#### Scenario: Click opens the link
- **WHEN** the user clicks on a cell with a hyperlink
- **THEN** the URL SHALL be opened via the system handler (browser for http(s), OS file handler for file://)

### Requirement: Feature flag for migration
A config flag SHALL gate the new terminal path so both implementations can coexist during migration.

#### Scenario: Legacy path by default initially
- **WHEN** `experimental.wezterm_terminal` is absent or false
- **THEN** the legacy xterm.js-based terminal SHALL be used

#### Scenario: New path when flag is on
- **WHEN** `experimental.wezterm_terminal = true`
- **THEN** the new wezterm-term-based terminal SHALL be used
- **AND** no xterm.js code SHALL be loaded for that session

