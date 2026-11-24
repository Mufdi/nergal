## MODIFIED Requirements

### Requirement: Selection and clipboard
The terminal SHALL support mouse-drag selection, keyboard copy, and paste. A user paste SHALL be wrapped in bracketed-paste markers only while the running app has enabled bracketed paste (DECSET 2004). Otherwise the text SHALL be written without markers: every newline (`\r\n`, `\n` or `\r`) written as `\r`, then embedded bracketed-paste markers removed. A delivery to an agent session (`paste_to_session`, and the `deliver_prompt_to_session` command the git panel's "Apply with Agent" and "Ask Agent" use) SHALL stay wrapped whatever the mode, so a multi-line body arrives as one paste. `deliver_prompt_to_session` SHALL then write a single CR as a separate write after a settle delay, so the agent submits the whole paste as one turn, and SHALL resolve only after that CR is written, returning any paste or submit error.

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
- **WHEN** `paste_to_session` or `deliver_prompt_to_session` delivers `x` + LF + `y`
- **THEN** the PTY SHALL receive `ESC[200~x` + LF + `yESC[201~`

#### Scenario: Prompt delivery submits after the paste
- **GIVEN** an agent session with a live PTY
- **WHEN** `deliver_prompt_to_session` delivers `x` + LF + `y`
- **THEN** the PTY SHALL first receive `ESC[200~x` + LF + `yESC[201~`
- **AND** about 250 ms later it SHALL receive one separate write holding only CR
- **AND** the command SHALL resolve after that CR write, not before

#### Scenario: Prompt delivery to a shell tab is refused
- **WHEN** `deliver_prompt_to_session` targets an auxiliary shell id (one containing `::`)
- **THEN** it SHALL return an error and write nothing to any PTY
