## RENAMED Requirements

- FROM: `### Requirement: Middle and right buttons reach the alt-screen app`
- TO: `### Requirement: Middle and right buttons reach a mouse-tracking app`

## MODIFIED Requirements

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
