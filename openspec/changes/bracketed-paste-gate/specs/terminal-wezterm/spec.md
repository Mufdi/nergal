## MODIFIED Requirements

### Requirement: Selection and clipboard
The terminal SHALL support mouse-drag selection, keyboard copy, and paste. A user paste SHALL be wrapped in bracketed-paste markers only while the running app has enabled bracketed paste (DECSET 2004). Otherwise the text SHALL be written without markers: every newline (`\r\n`, `\n` or `\r`) written as `\r`, then embedded bracketed-paste markers removed. A delivery to an agent session (`paste_to_session`, and the `terminal_deliver` command the PR viewer's "Apply with Claude" uses) SHALL stay wrapped whatever the mode, so a multi-line body arrives as one paste.

#### Scenario: Drag to select
- **WHEN** the user mouse-downs on a cell and drags to another cell
- **THEN** the range SHALL be visually highlighted
- **AND** on mouseup the selection SHALL remain highlighted until a new selection is started or input clears it

#### Scenario: Copy selection to clipboard
- **WHEN** the user presses Ctrl+Shift+C (or the configured copy shortcut) with an active selection
- **THEN** the selected cells SHALL be serialized to text and written to the system clipboard

#### Scenario: Bracketed paste
- **GIVEN** the app wrote `ESC[?2004h`
- **WHEN** the user presses Ctrl+Shift+V with clipboard text `a` + LF + `b`
- **THEN** the PTY SHALL receive `ESC[200~a` + LF + `bESC[201~`

#### Scenario: Plain paste without DECSET 2004
- **GIVEN** the app has not enabled bracketed paste, or wrote `ESC[?2004l`
- **WHEN** the user pastes `a` + CR + LF + `b` + LF + `c` + `ESC[201~` + `d`
- **THEN** the PTY SHALL receive `a` + CR + `b` + CR + `cd`

#### Scenario: Agent deliveries stay bracketed
- **GIVEN** the agent session's app has not enabled bracketed paste
- **WHEN** `paste_to_session` or `terminal_deliver` delivers `x` + LF + `y`
- **THEN** the PTY SHALL receive `ESC[200~x` + LF + `yESC[201~`

### Requirement: A paste body cannot end the bracketed paste
Every paste encoder (the agent delivery, and the bracketed and plain user paste) SHALL remove every bracketed-paste marker from the body; the bracketed ones do so before wrapping it, and the plain one after turning newlines into `\r`. It SHALL remove the 7-bit forms `ESC[200~` / `ESC[201~`. It SHALL also remove the code point U+009B followed by `200~` or `201~` (it reaches the PTY as the UTF-8 bytes `C2 9B`). It SHALL repeat the removal until no marker remains. Other escape sequences in the body SHALL stay unchanged.

#### Scenario: Embedded end marker
- **GIVEN** the app enabled bracketed paste, or the body is an agent delivery
- **WHEN** the body is `safe` + `ESC[201~` + `rm -rf ~` + `CR`
- **THEN** the PTY SHALL receive `ESC[200~safe` + `rm -rf ~` + `CR` + `ESC[201~`

#### Scenario: Marker rebuilt by a first removal
- **GIVEN** the app enabled bracketed paste, or the body is an agent delivery
- **WHEN** the body is `a` + `ESC[20` + `ESC[201~` + `1~b`
- **THEN** the PTY SHALL receive `ESC[200~ab ESC[201~` with no space

#### Scenario: Color sequences survive
- **GIVEN** the app enabled bracketed paste, or the body is an agent delivery
- **WHEN** the body is `ESC[31mred ESC[0m` with no spaces
- **THEN** the body SHALL reach the PTY unchanged between the markers

#### Scenario: Plain paste removes the same markers
- **GIVEN** the app has not enabled bracketed paste
- **WHEN** the user pastes `a` + U+009B + `201~b` + `ESC[20` + `ESC[201~` + `1~c`
- **THEN** the PTY SHALL receive `abc`
