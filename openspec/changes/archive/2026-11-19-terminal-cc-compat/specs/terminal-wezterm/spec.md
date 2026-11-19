## ADDED Requirements

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
