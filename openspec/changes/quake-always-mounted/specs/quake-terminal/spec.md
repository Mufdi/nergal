## MODIFIED Requirements

### Requirement: Quake terminal overlay

The system SHALL provide a full-width Quake terminal overlay that drops below the TopBar over the content area (above the StatusBar), holding N shell tabs for the active session, resizable in height. It SHALL be toggled and focused with `Ctrl+}` (bound respecting `event.code` per the WebKitGTK convention). The overlay SHALL stay mounted while closed: a closed overlay is translated above the content area and hidden, and its shell terminals keep their rendered grid. The open and close slide SHALL animate only `transform`, on a compositing layer that persists across toggles.

#### Scenario: Toggle and focus cycle

- **WHEN** the overlay is hidden and the user presses `Ctrl+}`
- **THEN** it SHALL open and receive focus (accent border on the quake zone)
- **WHEN** it is visible but unfocused and the user presses `Ctrl+}`
- **THEN** focus SHALL move to it without hiding it
- **WHEN** it is visible and focused and the user presses `Ctrl+}`
- **THEN** it SHALL hide

#### Scenario: Stays visible when focus leaves

- **WHEN** the overlay is open and the user focuses another zone (e.g. `Ctrl+ñ` to the agent terminal)
- **THEN** the overlay SHALL remain visible (logs readable) and the accent border SHALL follow the focused zone

#### Scenario: Visibility is per-session

- **WHEN** the quake is open in session A and the user switches to session B where it was closed
- **THEN** the overlay SHALL be hidden, and SHALL reappear when switching back to A (mirrors the right panel's per-session collapsed state)

#### Scenario: Tab shortcuts while the quake holds focus

- **WHEN** focus is in the quake zone
- **THEN** `Ctrl+W` SHALL close the active shell tab (instead of soft-closing the session), `Ctrl+Shift+T` SHALL open a new shell tab, and `Ctrl+Tab` / `Ctrl+Shift+Tab` SHALL cycle between shell tabs

#### Scenario: Slide phases on open and close

- **GIVEN** the overlay is in phase `hidden`
- **WHEN** it opens
- **THEN** it SHALL enter phase `entering`, and SHALL reach phase `shown` when the slide-down animation ends
- **WHEN** it closes from phase `shown` in the same session
- **THEN** it SHALL enter phase `exiting`, and SHALL reach phase `hidden` only when the slide-up animation ends (or a fallback timer of twice the slide duration fires)

#### Scenario: Session switch snaps without a slide

- **GIVEN** the overlay is open in session A and closed in session B
- **WHEN** the user switches from A to B
- **THEN** the overlay SHALL go straight to phase `hidden` with no exit slide over session B
- **AND** if the quake held focus, the focus zone SHALL reset to `terminal`
- **WHEN** the user switches back from B to A
- **THEN** the overlay SHALL play the enter slide

#### Scenario: Focus glow does not animate during a slide

- **GIVEN** the panel glow is on
- **WHEN** the overlay is in phase `entering` or `exiting`
- **THEN** the `border-color` and `box-shadow` transitions of the overlay and of the panels below it SHALL be off, so the focus accent changes in a single frame

#### Scenario: Scrollback survives close and reopen

- **GIVEN** a quake shell has printed 200 lines and the user scrolled up 50 lines
- **WHEN** the user closes the overlay and reopens it in the same session
- **THEN** the shell SHALL show the same rows and scroll position, with no re-fit and no full repaint on reopen

#### Scenario: Hidden overlay never holds focus

- **GIVEN** focus is inside the quake overlay
- **WHEN** the overlay leaves phase `shown` for any reason (toggle or session switch)
- **THEN** the focused element inside the overlay SHALL be blurred, and a hidden overlay SHALL not be focusable or clickable
