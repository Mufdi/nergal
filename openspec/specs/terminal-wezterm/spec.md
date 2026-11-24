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

### Requirement: Middle and right buttons reach a mouse-tracking app
While the running app has mouse tracking enabled (DECSET 1000, 1002 or 1003) and the view sits at the live bottom (scroll offset 0), on the primary or the alternate screen, the canvas SHALL forward middle and right mouse button press and release events to wezterm-term, and a right click SHALL NOT open the webview context menu. Without mouse tracking, with the view scrolled back into history, or with Shift held, these buttons SHALL NOT be forwarded and the context menu SHALL NOT be suppressed. A context menu opened from the keyboard follows the same rule without the Shift exception for the Menu key (Shift+F10 carries Shift). The backend SHALL drop a button press while the primary screen is scrolled back, whatever the frontend sent, and SHALL still forward the release of a button pressed at the live bottom. Each grid update SHALL carry the current mouse-tracking state, and a change of that state SHALL produce a grid update even when nothing visible changed.

#### Scenario: Right click with SGR mouse mode
- **GIVEN** the app wrote `ESC[?1000h` and `ESC[?1006h`
- **WHEN** the user presses and releases the right button over cell column 4, row 2 (zero-based)
- **THEN** the PTY SHALL receive `ESC[<2;5;3M` followed by `ESC[<2;5;3m`

#### Scenario: Middle click with SGR mouse mode
- **GIVEN** the app wrote `ESC[?1000h` and `ESC[?1006h`
- **WHEN** the user presses and releases the middle button over cell column 4, row 2 (zero-based)
- **THEN** the PTY SHALL receive `ESC[<1;5;3M` followed by `ESC[<1;5;3m`

#### Scenario: Release outside the canvas
- **GIVEN** the user pressed the right button with mouse tracking on, at cell column 4, row 2
- **WHEN** the user releases the button with the pointer outside the canvas
- **THEN** the canvas SHALL forward the release at the press cell

#### Scenario: Focus loss while a button is held
- **GIVEN** the user pressed the right button with mouse tracking on
- **WHEN** the terminal reports a focus loss and then the user releases the button
- **THEN** the canvas SHALL NOT forward the release, because wezterm-term already sent one on focus loss

#### Scenario: Second press while one is held
- **GIVEN** the user holds the right button pressed with mouse tracking on
- **WHEN** the user presses the middle button
- **THEN** the canvas SHALL NOT forward the middle press

#### Scenario: Keyboard context menu over the terminal
- **GIVEN** the running app has mouse tracking enabled and the view sits at the live bottom, with focus in the terminal
- **WHEN** the user presses Shift+F10
- **THEN** no byte SHALL reach the PTY and the context menu SHALL NOT be suppressed
- **AND WHEN** the user presses the Menu key
- **THEN** no byte SHALL reach the PTY and the context menu SHALL be suppressed
- **AND** without mouse tracking the Menu key SHALL NOT suppress the context menu

#### Scenario: Mouse tracking on the primary screen
- **GIVEN** the app wrote `ESC[?1000h` and the alternate screen is not active
- **WHEN** the user presses the right button
- **THEN** the canvas SHALL invoke `terminal_mouse_button` and SHALL suppress the context menu

#### Scenario: Primary screen ignores middle and right
- **GIVEN** the alternate screen is not active and no mouse tracking mode is enabled
- **WHEN** the user presses the middle or right button
- **THEN** the canvas SHALL NOT invoke `terminal_mouse_button`

#### Scenario: Alternate screen without mouse tracking
- **GIVEN** the alternate screen is active and no mouse tracking mode is enabled
- **WHEN** the user presses the middle or right button
- **THEN** the canvas SHALL NOT invoke `terminal_mouse_button` and SHALL NOT suppress the context menu

#### Scenario: Scrolled-back primary screen does not forward
- **GIVEN** the app wrote `ESC[?1000h` on the primary screen and the user scrolled the view 30 lines into history
- **WHEN** the user presses the right button
- **THEN** the canvas SHALL NOT invoke `terminal_mouse_button` and SHALL NOT suppress the context menu

#### Scenario: Shift bypasses mouse reporting
- **GIVEN** the app wrote `ESC[?1000h`
- **WHEN** the user holds Shift and presses the right button
- **THEN** the canvas SHALL NOT invoke `terminal_mouse_button` and SHALL NOT suppress the context menu

#### Scenario: The backend drops a report from a scrolled-back view
- **GIVEN** the app wrote `ESC[?1000h` on the primary screen and the session view is scrolled 30 lines into history
- **WHEN** `terminal_mouse_button` arrives with a right-button press
- **THEN** the PTY SHALL receive no mouse report

#### Scenario: A release after scrolling still reaches the app
- **GIVEN** the app wrote `ESC[?1000h` and the user pressed the right button at the live bottom
- **WHEN** the view scrolls 30 lines into history and the release arrives
- **THEN** the PTY SHALL receive the release report

#### Scenario: Mouse tracking state in the snapshot
- **WHEN** the app writes `ESC[?1000h`, `ESC[?1002h` or `ESC[?1003h`
- **THEN** the grid snapshot SHALL report `mouse_tracking: true`
- **AND** after the matching `l` sequence (for example `ESC[?1000l`) it SHALL report `false`

#### Scenario: A mode switch without a redraw still updates the canvas
- **GIVEN** the differ already emitted an update and no row, cursor or title changes afterwards
- **WHEN** the app writes `ESC[?1000h`
- **THEN** the differ SHALL emit an update with `mouse_tracking: true`
- **AND** a following `ESC[?1000l` and `ESC[?1000h`, with no other change, SHALL each emit one update with the new value

#### Scenario: The update names the field mouseTracking
- **WHEN** an attached session receives `ESC[?1000h`
- **THEN** the JSON sent on its grid channel SHALL contain `"mouseTracking":true`

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

### Requirement: Color queries report the theme colors
The terminal SHALL answer `OSC 10 ; ?` with the foreground and `OSC 11 ; ?` with the background that the canvas paints for the active theme. When the theme changes, every live session SHALL answer the new colors without a restart. Until the frontend reports colors, the terminal SHALL answer wezterm's default palette. A color an application sets with `OSC 10` or `OSC 11` SHALL keep taking precedence, as wezterm does.

#### Scenario: Light theme reports a white background
- **GIVEN** the frontend reported foreground `#18181b` and background `#ffffff`
- **WHEN** a session receives `ESC ] 11 ; ? ESC \`
- **THEN** it answers `ESC ] 11 ; rgb:ffff/ffff/ffff ESC \`

#### Scenario: Theme switch reaches a live session
- **GIVEN** a live session after the frontend reported background `#ffffff`
- **WHEN** the frontend reports background `#09090b`
- **THEN** the same session answers `OSC 11` with `rgb:0909/0909/0b0b`

#### Scenario: Colors set by an application keep precedence
- **GIVEN** the frontend reported background `#ffffff` and the application sent `ESC ] 11 ; rgb:1111/2222/3333 ESC \`
- **WHEN** the frontend reports background `#09090b` and the session receives `OSC 11 ; ?`
- **THEN** it answers `rgb:1111/2222/3333`

#### Scenario: Default cells stay unresolved
- **GIVEN** the frontend reported colors
- **WHEN** the session snapshots a cell with default colors
- **THEN** the cell's `fg` and `bg` are absent, so the canvas paints the theme

#### Scenario: Colors are normalized before they are sent
- **WHEN** the theme token is `#fff`, `#FFFFFF` or `rgb(255, 255, 255)`
- **THEN** the frontend sends the RGB triple `[255, 255, 255]`
- **AND** a token it cannot parse sends nothing

### Requirement: Synchronized output holds the frame
`TerminalSession` SHALL track DEC private mode 2026 from the bytes it parses: `CSI ? 2026 h` turns it on, and `CSI ? 2026 l` or `ESC c` turn it off, also when a sequence is split across chunks at any byte. A repeated `h` SHALL NOT restart the hold. While the mode is on, the emitter SHALL NOT send a grid update. It SHALL send one when the mode turns off, or 150 ms after the mode turned on, whichever comes first. After the cap, output SHALL flow unheld until the mode turns off and on again.

#### Scenario: Frame split across chunks is sent whole
- **GIVEN** an attached renderer
- **WHEN** the session parses `CSI ? 2026 h`, then `AAA` in one chunk and `BBB` plus `CSI ? 2026 l` in a later chunk 20 ms after it
- **THEN** the renderer receives no update that contains `AAA` without `BBB`

#### Scenario: Unclosed frame is sent after the timeout
- **GIVEN** an attached renderer
- **WHEN** the session parses `CSI ? 2026 h` and `AAA`, and nothing else
- **THEN** the renderer receives an update with `AAA` no earlier than 140 ms and no later than 600 ms after `h`

#### Scenario: Sequence split at any byte
- **WHEN** the session parses `ESC [ ? 2026 h` split into two chunks at each of its 7 inner positions
- **THEN** the mode is on after the second chunk every time

#### Scenario: Closing the mode flushes promptly
- **GIVEN** an attached renderer and the mode on with `AAA` pending
- **WHEN** the session parses `CSI ? 2026 l`
- **THEN** the renderer receives the update less than 100 ms later

#### Scenario: A repeated h does not extend the hold
- **WHEN** the session parses `CSI ? 2026 h`, then `CSI ? 2026 h` again 100 ms later
- **THEN** `sync_output_since` still reports the first instant

#### Scenario: Output without the mode is unchanged
- **WHEN** the session parses `hello` with the mode off
- **THEN** the renderer receives the update less than 200 ms later

### Requirement: Pixel geometry reaches the emulator and the PTY

The backend SHALL give the wezterm-term emulator and the PTY the canvas size in device pixels: `pixel_width = cols × cell width` and `pixel_height = rows × cell height`. The frontend SHALL gate each call on the last size it requested, recorded when the call is sent, and SHALL forget that record only when the failed call is still the latest request. The frontend SHALL send that size after it spawns or re-attaches a session terminal or a quake shell, when it resizes one, and when the cell size changes while the column and row counts stay the same. After such a cell size change, the canvas backing width SHALL equal the `pixel_width` sent, and the backing height SHALL equal the `pixel_height` sent.

#### Scenario: Spawn with pixel geometry
- **GIVEN** a session terminal spawned with 80 columns, 24 rows, `pixel_width` 800 and `pixel_height` 480
- **WHEN** the app writes `CSI 14 t`
- **THEN** the emulator SHALL answer `ESC [ 4 ; 480 ; 800 t`
- **AND** the PTY window size SHALL report `ws_xpixel` 800 and `ws_ypixel` 480

#### Scenario: Cell size report follows the geometry
- **GIVEN** a session terminal with 80 columns, 24 rows, `pixel_width` 800 and `pixel_height` 480
- **WHEN** the app writes `CSI 16 t`
- **THEN** the emulator SHALL answer `ESC [ 6 ; 20 ; 10 t`

#### Scenario: Resize carries the new geometry
- **GIVEN** a session terminal with 80 columns and 24 rows
- **WHEN** the frontend resizes it to 100 columns, 30 rows, `pixel_width` 1000 and `pixel_height` 600
- **THEN** `CSI 14 t` SHALL answer `ESC [ 4 ; 600 ; 1000 t`
- **AND** the PTY window size SHALL report `ws_xpixel` 1000 and `ws_ypixel` 600

#### Scenario: A resize with an unchanged size changes nothing
- **GIVEN** a session terminal at 80 columns, 24 rows, `pixel_width` 800 and `pixel_height` 480, where the app set a scroll region with `CSI 5 ; 10 r`
- **WHEN** the frontend resizes it to the same 80 columns, 24 rows, 800 and 480
- **THEN** the backend SHALL NOT resize the PTY master, so the child gets no `SIGWINCH`
- **AND** the backend SHALL NOT resize the emulator, so the scroll region of rows 5 to 10 stays in effect

#### Scenario: A cell size change resends the geometry and resizes the canvas
- **GIVEN** a terminal at 80 columns and 24 rows with a cell of 9 × 18 device pixels
- **WHEN** the font changes to a cell of 10 × 20 device pixels and the column and row counts stay at 80 and 24
- **THEN** the frontend SHALL call `resize_session_terminal` with 80 columns, 24 rows, `pixel_width` 800 and `pixel_height` 480
- **AND** the canvas backing size SHALL be 800 × 480, taken from the same geometry function

#### Scenario: An unchanged geometry sends nothing
- **GIVEN** a terminal that last sent 80 columns, 24 rows, 800 and 480
- **WHEN** `fit` runs again with the same container size and the same cell
- **THEN** the frontend SHALL NOT call `resize_session_terminal`

#### Scenario: A failed resize is sent again
- **GIVEN** a terminal whose last `resize_session_terminal` call with 80 columns, 24 rows, 800 and 480 was rejected
- **WHEN** `fit` runs again with the same container size and the same cell
- **THEN** the frontend SHALL call `resize_session_terminal` again with the same values

#### Scenario: Answers that resolve out of order never leave a stale size
- **GIVEN** a terminal that sent a resize to 100 columns and then a resize to 90 columns, both still pending
- **WHEN** `fit` computes 100 columns again before either call resolves
- **THEN** the frontend SHALL call `resize_session_terminal` with 100 columns, because the last requested size is 90

#### Scenario: A failure does not clear a newer request
- **GIVEN** a terminal that requested 100 columns and then 90 columns
- **WHEN** the 100-column call is rejected after the 90-column call was sent
- **THEN** the last requested size SHALL stay at 90 columns

#### Scenario: The post-spawn resize uses the current size
- **GIVEN** a terminal that spawned with 80 columns and 24 rows, and `fit` moved it to 100 columns and 30 rows while the spawn was pending
- **WHEN** the spawn resolves
- **THEN** the post-spawn `resize_session_terminal` call SHALL carry 100 columns and 30 rows

#### Scenario: Re-attaching to a live PTY applies the current geometry
- **GIVEN** a live session PTY created with no pixel size
- **WHEN** the frontend re-attaches to it with 80 columns, 24 rows and a cell of 10 × 20 device pixels
- **THEN** the frontend SHALL call `resize_session_terminal` with `pixel_width` 800 and `pixel_height` 480

### Requirement: The emulator never gets less than one pixel per cell

The backend SHALL clamp every pixel size it gives the emulator and the PTY. When the caller sends no pixel size for an axis, or a value smaller than the column count (width) or the row count (height), the backend SHALL use a fallback cell of 8 × 16 device pixels for that axis. The result SHALL saturate at 65 535 instead of overflowing. As a result, the pixel geometry Nergal provides can never cause a division by zero while wezterm-term places an inline image.

#### Scenario: Zero columns or rows are rejected
- **GIVEN** a live session terminal
- **WHEN** `resize_session_terminal` arrives with 0 columns or 0 rows
- **THEN** the backend SHALL return an error and leave the PTY and the emulator unchanged

#### Scenario: Missing geometry uses the fallback cell
- **GIVEN** a session terminal spawned with 80 columns and 24 rows and no pixel size
- **WHEN** the app writes `CSI 14 t`
- **THEN** the emulator SHALL answer `ESC [ 4 ; 384 ; 640 t`

#### Scenario: Geometry below one pixel per cell uses the fallback cell
- **GIVEN** a session terminal with 80 columns and 24 rows
- **WHEN** it is resized with `pixel_width` 0 and `pixel_height` 10
- **THEN** `CSI 14 t` SHALL answer `ESC [ 4 ; 384 ; 640 t`

#### Scenario: A huge column count saturates
- **GIVEN** a clamp request with 9000 columns, 24 rows and no pixel size
- **WHEN** the backend computes the geometry
- **THEN** `pixel_width` SHALL be 65 535 and `pixel_height` SHALL be 384

#### Scenario: Sixel image on a terminal created without pixel geometry
- **GIVEN** a session terminal with 80 columns and 24 rows and no pixel size
- **WHEN** the app writes a 64 × 48 sixel image
- **THEN** the emulator SHALL not panic
- **AND** the next grid snapshot SHALL return 24 rows

#### Scenario: iTerm2 image with less than one pixel per cell
- **GIVEN** a session terminal resized to 80 columns, 24 rows, `pixel_width` 10 and `pixel_height` 10
- **WHEN** the app writes an OSC 1337 `File=inline=1` sequence that carries a 64 × 48 PNG
- **THEN** the emulator SHALL not panic
- **AND** the next grid snapshot SHALL return 24 rows
