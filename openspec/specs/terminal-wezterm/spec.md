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

### Requirement: Canvas-based frontend renderer

The frontend SHALL render the grid using HTML `<canvas>` with a pre-generated glyph atlas. The renderer SHALL support multiple **regions** (`center` for agent terminals, `quake` for auxiliary shells), each with its own host element and its own active terminal, so terminals in different regions render simultaneously. Region-less calls SHALL default to `center`, keeping the original single-terminal API back-compatible.

#### Scenario: Cells drawn from atlas

- **WHEN** a `terminal:grid-update` arrives
- **THEN** changed cells SHALL be drawn by blitting glyph rectangles from the OffscreenCanvas atlas to the visible canvas
- **AND** glyphs not in the atlas SHALL fall back to direct `fillText` rendering

#### Scenario: Atlas regenerates on theme or font change

- **WHEN** the user changes the terminal font, font size, or color theme
- **THEN** the atlas SHALL be regenerated before the next render frame

#### Scenario: Regions render independently

- **WHEN** an agent terminal is active in the `center` region and an auxiliary shell is active in the `quake` region
- **THEN** both SHALL render at the same time, each into its own host
- **AND** showing/hiding a terminal in one region SHALL NOT affect the other region's active terminal

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

### Requirement: Wheel scrollback follows the terminal-emulator standard

In the primary screen, wheel input SHALL scroll the local scrollback at the terminal-emulator standard rate of 3 lines per wheel notch (one notch = 40 px of WebKitGTK pixel delta, or 1 unit in `DOM_DELTA_LINE` mode). Fractional deltas (hi-res wheels, touchpads) SHALL be accumulated across events rather than rounded per event, so no travel is lost.

#### Scenario: Discrete wheel notch scrolls 3 lines

- **WHEN** the user rolls a discrete mouse wheel one notch over the terminal in the primary screen
- **THEN** the viewport SHALL move 3 lines

#### Scenario: Hi-res wheel deltas accumulate

- **WHEN** a hi-res wheel emits several sub-line pixel deltas for a single notch
- **THEN** the fractional values SHALL accumulate in a remainder and the viewport SHALL move the same total distance as a discrete notch
- **AND** a direction change SHALL discard the leftover remainder

#### Scenario: Alt screen forwards wheel to the app

- **WHEN** the alternate screen is active (agent TUI, vim, less)
- **THEN** wheel events SHALL be forwarded to the running application via wezterm-term's mouse pipeline instead of scrolling local scrollback

### Requirement: Kitty keyboard protocol not advertised
The terminal SHALL NOT answer the Kitty keyboard query, because the key encoder cannot produce Kitty sequences and advertising Kitty makes apps skip their modifyOtherKeys request. There SHALL be no user-facing setting to enable it: a config file that still carries the removed `terminal_kitty_keyboard` key SHALL be loaded with that key ignored. Kitty advertising returns only together with a real Kitty key encoder (FRI-503, option A).

#### Scenario: Session does not answer the Kitty query
- **WHEN** a session created through the normal app path receives `ESC[?u`
- **THEN** the terminal SHALL write nothing to the PTY

#### Scenario: Legacy config key is ignored
- **GIVEN** a saved config containing `"terminal_kitty_keyboard": true`
- **WHEN** the config is loaded and a session is created through the normal app path
- **THEN** the load SHALL succeed
- **AND** the session SHALL NOT answer `ESC[?u`

### Requirement: Focus reporting
The terminal SHALL forward focus changes of its input surface to wezterm-term. wezterm-term SHALL emit a focus report only when the running app enabled DECSET 1004, and only when the focus state changes.

#### Scenario: Focus lost after DECSET 1004
- **GIVEN** the app wrote `ESC[?1004h`
- **WHEN** the terminal input surface loses focus
- **THEN** the PTY SHALL receive exactly `ESC[O`

#### Scenario: Focus gained after DECSET 1004
- **GIVEN** the app wrote `ESC[?1004h` and the terminal reported a focus loss
- **WHEN** the terminal input surface gains focus
- **THEN** the PTY SHALL receive exactly `ESC[I`

#### Scenario: No report without DECSET 1004
- **WHEN** the input surface loses and regains focus and the app never wrote `ESC[?1004h`
- **THEN** the PTY SHALL receive no bytes

#### Scenario: Focus taken back within 100 ms is not reported
- **GIVEN** the app wrote `ESC[?1004h`
- **WHEN** the input surface blurs and regains focus within 100 ms
- **THEN** the PTY SHALL receive no focus report

#### Scenario: Hiding a visible terminal reports focus loss
- **GIVEN** the app wrote `ESC[?1004h` in the visible terminal of a region
- **WHEN** another terminal becomes active in that region, the region is hidden, or its host is removed
- **THEN** the PTY of the hidden terminal SHALL receive `ESC[O`

#### Scenario: Repeated state is not reported twice
- **GIVEN** the app wrote `ESC[?1004h`
- **WHEN** a focus loss is forwarded twice in a row
- **THEN** the PTY SHALL receive `ESC[O` once

### Requirement: Middle and right buttons reach the alt-screen app
While the alternate screen is active, the canvas SHALL forward middle and right mouse button press and release events to wezterm-term. A right click over the alternate screen SHALL NOT open the webview context menu. On the primary screen these buttons SHALL NOT be forwarded.

#### Scenario: Right click with SGR mouse mode
- **GIVEN** the app wrote `ESC[?1000h` and `ESC[?1006h`
- **WHEN** the user presses and releases the right button over cell column 4, row 2 (zero-based)
- **THEN** the PTY SHALL receive `ESC[<2;5;3M` followed by `ESC[<2;5;3m`

#### Scenario: Middle click with SGR mouse mode
- **GIVEN** the app wrote `ESC[?1000h` and `ESC[?1006h`
- **WHEN** the user presses and releases the middle button over cell column 4, row 2 (zero-based)
- **THEN** the PTY SHALL receive `ESC[<1;5;3M` followed by `ESC[<1;5;3m`

#### Scenario: Release outside the canvas
- **GIVEN** the user pressed the right button over the alt screen at cell column 4, row 2
- **WHEN** the user releases the button with the pointer outside the canvas
- **THEN** the canvas SHALL forward the release at the press cell

#### Scenario: Focus loss while a button is held
- **GIVEN** the user pressed the right button over the alt screen
- **WHEN** the terminal reports a focus loss and then the user releases the button
- **THEN** the canvas SHALL NOT forward the release, because wezterm-term already sent one on focus loss

#### Scenario: Second press while one is held
- **GIVEN** the user holds the right button pressed over the alt screen
- **WHEN** the user presses the middle button
- **THEN** the canvas SHALL NOT forward the middle press

#### Scenario: Primary screen ignores middle and right
- **WHEN** the alternate screen is not active and the user presses the middle or right button
- **THEN** the canvas SHALL NOT invoke `terminal_mouse_button`

### Requirement: A paste body cannot end the bracketed paste
The bracketed-paste encoder SHALL remove every bracketed-paste marker from the body before wrapping it. It SHALL remove the 7-bit forms `ESC[200~` / `ESC[201~`. It SHALL also remove the code point U+009B followed by `200~` or `201~` (it reaches the PTY as the UTF-8 bytes `C2 9B`). It SHALL repeat the removal until no marker remains. Other escape sequences in the body SHALL stay unchanged.

#### Scenario: Embedded end marker
- **WHEN** the body is `safe` + `ESC[201~` + `rm -rf ~` + `CR`
- **THEN** the PTY SHALL receive `ESC[200~safe` + `rm -rf ~` + `CR` + `ESC[201~`

#### Scenario: Marker rebuilt by a first removal
- **WHEN** the body is `a` + `ESC[20` + `ESC[201~` + `1~b`
- **THEN** the PTY SHALL receive `ESC[200~ab ESC[201~` with no space

#### Scenario: Color sequences survive
- **WHEN** the body is `ESC[31mred ESC[0m` with no spaces
- **THEN** the body SHALL reach the PTY unchanged between the markers

### Requirement: Terminal-generated sequences reach the PTY whole
The PTY writer that wezterm-term uses SHALL write each message completely, retrying short writes, under one lock acquisition.

#### Scenario: A failed write is logged
- **WHEN** the PTY writer returns an error for a wezterm-term message
- **THEN** the backend SHALL log a warning before returning the error

#### Scenario: Short writes do not truncate a mouse report
- **GIVEN** a PTY writer that accepts at most 3 bytes per write call
- **WHEN** wezterm-term writes the 9-byte report `ESC[<2;5;3m`
- **THEN** the PTY SHALL receive all 9 bytes in order

