## ADDED Requirements

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

## MODIFIED Requirements

### Requirement: Input encoding via wezterm
User key events SHALL be encoded to terminal bytes using `wezterm-term`'s input encoder in the backend. The frontend SHALL NOT encode key events locally. The encoder SHALL produce legacy xterm bytes, switching to modifyOtherKeys bytes when the app requests them with `CSI > 4;2 m`. CSI-u key encoding SHALL stay off. Kitty-protocol key encoding is NOT provided, which is why Kitty is not advertised by default. Two keys are remapped before the encoder: Ctrl+Backspace becomes Ctrl+W, and unmodified Shift+Enter becomes LF.

#### Scenario: Key event sent to backend
- **WHEN** the user presses a key while the terminal has focus
- **THEN** the frontend SHALL build a `TerminalKeyEvent` from the `KeyboardEvent` and invoke `terminal_input(session_id, event)`
- **AND** the frontend SHALL NOT attempt to compute the terminal byte representation locally

#### Scenario: Backend encodes and writes to PTY
- **WHEN** `terminal_input` is invoked
- **THEN** the backend SHALL map the event to `wezterm_term::KeyCode` + `Modifiers`
- **AND** SHALL call `Terminal::key_down`, which SHALL encode and write bytes to the PTY writer

#### Scenario: Ctrl+Backspace deletes a word
- **WHEN** the user presses Ctrl+Backspace
- **THEN** the PTY SHALL receive `0x17` (Ctrl+W)
- **AND** plain Backspace SHALL write `0x7f`

#### Scenario: Shift+Enter inserts a newline
- **WHEN** the user presses Shift+Enter with no other modifier
- **THEN** the PTY SHALL receive `0x0a` (LF), not `0x0d`

#### Scenario: Ctrl+Enter is legacy until modifyOtherKeys is requested
- **GIVEN** the default config and no modifyOtherKeys request
- **WHEN** the user presses Ctrl+Enter
- **THEN** the PTY SHALL receive `0x0d`

#### Scenario: modifyOtherKeys distinguishes Ctrl+Enter
- **GIVEN** the app wrote `ESC[>4;2m`
- **WHEN** the user presses Ctrl+Enter
- **THEN** the PTY SHALL receive `ESC[27;5;13~`

#### Scenario: Opted-in Kitty flags still do not change key encoding
- **GIVEN** `terminal_kitty_keyboard = true` and the app wrote `ESC[>1u`
- **WHEN** the user presses Ctrl+Enter
- **THEN** the PTY SHALL receive `0x0d`

## RENAMED Requirements

- FROM: `### Requirement: Kitty keyboard protocol enabled by default`
- TO: `### Requirement: Kitty keyboard protocol not advertised by default`

## MODIFIED Requirements

### Requirement: Kitty keyboard protocol not advertised by default
The terminal SHALL NOT answer the Kitty keyboard query by default, because the key encoder cannot produce Kitty sequences. Advertising Kitty makes apps skip their modifyOtherKeys request. The terminal SHALL answer the query only when `terminal_kitty_keyboard = true`.

#### Scenario: Default does not answer the Kitty query
- **WHEN** a session with no config override receives `ESC[?u`
- **THEN** the terminal SHALL write nothing to the PTY

#### Scenario: Opt-in answers the Kitty query
- **WHEN** a session with `terminal_kitty_keyboard = true` receives `ESC[?u`
- **THEN** the terminal SHALL write `ESC[?0u` to the PTY
