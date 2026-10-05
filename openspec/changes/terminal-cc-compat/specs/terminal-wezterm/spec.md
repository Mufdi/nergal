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

#### Scenario: Primary screen ignores middle and right
- **WHEN** the alternate screen is not active and the user presses the middle or right button
- **THEN** the canvas SHALL NOT invoke `terminal_mouse_button`

### Requirement: A paste body cannot end the bracketed paste
The bracketed-paste encoder SHALL remove every bracketed-paste marker from the body before wrapping it. It SHALL remove the 7-bit forms `ESC[200~` / `ESC[201~` and the C1 forms `U+009B 200~` / `U+009B 201~`. It SHALL repeat the removal until no marker remains. Other escape sequences in the body SHALL stay unchanged.

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

#### Scenario: Short writes do not truncate a mouse report
- **GIVEN** a PTY writer that accepts at most 3 bytes per write call
- **WHEN** wezterm-term writes the 9-byte report `ESC[<2;5;3m`
- **THEN** the PTY SHALL receive all 9 bytes in order
