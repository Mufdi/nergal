## ADDED Requirements

### Requirement: Terminal column cap
When the terminal reading flag is on, the main terminal (region `center`) SHALL use at most 100 columns. The column count SHALL be the smaller of 100 and the count that fits the pane. The canvas SHALL be centered horizontally in the pane. The row count SHALL stay the count that fits the pane. The PTY and the emulator SHALL receive the capped column count, so the agent wraps at that width. The quake terminal SHALL ignore the flag. A main terminal that is not shown when the flag changes SHALL adopt the new column count the next time it is shown.

#### Scenario: Wide pane is capped and centered
- **GIVEN** the terminal reading flag is on and the pane fits 180 columns of 8 px cells (1440 px wide)
- **WHEN** the active main terminal fits its pane
- **THEN** its column count SHALL be 100
- **AND** `resize_session_terminal` SHALL receive `cols = 100`
- **AND** the canvas left edge SHALL sit 320 px from the pane left edge

#### Scenario: Narrow pane keeps its width
- **GIVEN** the terminal reading flag is on and the pane fits 80 columns
- **WHEN** the active main terminal fits its pane
- **THEN** its column count SHALL be 80 and the canvas SHALL start at the pane left edge

#### Scenario: Flag off uses the full pane
- **GIVEN** the terminal reading flag is off and the pane fits 180 columns
- **WHEN** the active main terminal fits its pane
- **THEN** its column count SHALL be 180 and the canvas SHALL start at the pane left edge

#### Scenario: Quake ignores the flag
- **GIVEN** the terminal reading flag is on and the quake pane fits 180 columns
- **WHEN** the active quake shell fits its pane
- **THEN** its column count SHALL be 180

#### Scenario: The margins act as part of the terminal
- **GIVEN** the terminal reading flag is on and the canvas is centered with empty margins
- **WHEN** the user turns the mouse wheel over a margin, or drags a selection from the canvas into a margin
- **THEN** the terminal SHALL scroll as if the wheel were over the canvas
- **AND** the selection SHALL extend to the nearest edge column

#### Scenario: A flag change during a spawn applies
- **GIVEN** a main terminal measured its first layout with the terminal reading flag off
- **WHEN** the flag turns on before that terminal finishes its spawn
- **THEN** the terminal SHALL fit its pane at 100 columns once the spawn finishes

#### Scenario: A hidden session adopts the flag when shown
- **GIVEN** sessions A and B each have a main terminal at 180 columns, B is shown, and the pane fits 180 columns
- **WHEN** the terminal reading flag turns on and the user then switches to A
- **THEN** A SHALL fit its pane at 100 columns when it is shown

### Requirement: Markdown width cap
When the markdown reading flag is on, every `MarkdownView` and `AnnotatableMarkdownView` SHALL render its content in a column at most `100ch` wide, centered horizontally, the same column count as the terminal cap. When the flag is off, the content SHALL use the full width of its container. While the config key `reading_mode_full_width_tables` is on (the default), a table SHALL span the full width of the view, 1rem from each side. While it is off, a table SHALL stay within the content column. Long unbroken text in a table cell SHALL wrap, with no horizontal scroll. The terminal has one column count for all its output, so the key has no effect on the terminal.

#### Scenario: Flag on caps and centers the markdown
- **GIVEN** the markdown reading flag is on
- **WHEN** a Plan, Spec, Obsidian note, Linear or ClickUp markdown view renders
- **THEN** its root element SHALL have `max-width: 100ch` and equal left and right auto margins

#### Scenario: Flag off keeps the full width
- **GIVEN** the markdown reading flag is off
- **WHEN** a markdown view renders
- **THEN** its root element SHALL have no max width

#### Scenario: A table spans the full view in reading mode
- **GIVEN** the markdown reading flag is on and a view 1400 px wide renders a table whose cell holds a 300-character URL
- **WHEN** the view lays out
- **THEN** the table SHALL start 1rem from the view left edge and end 1rem from the view right edge
- **AND** the URL SHALL wrap inside its cell and the view SHALL have no horizontal scroll

#### Scenario: Full-width tables turned off keep tables in the column
- **GIVEN** the markdown reading flag is on, `reading_mode_full_width_tables` is off, and a view 1400 px wide renders a table
- **WHEN** the view lays out
- **THEN** the table SHALL span the content column only, not the full view

### Requirement: Reading mode toggle follows the focused surface
The shortcut registry SHALL hold an entry `toggle-reading-mode` bound to `leader f` in the `surfaces` which-key group. Its handler SHALL read the focus zone: zone `terminal` SHALL flip the terminal reading flag, zone `panel` SHALL flip the markdown reading flag, and zones `sidebar` and `quake` SHALL change no flag. A change of the terminal reading flag SHALL make the active main terminal fit its pane again, so the new column count reaches the PTY without a window resize. Each flip SHALL show an info toast that names the surface and the new state, so a flip without a visible change (a narrow pane, a panel tab without markdown) is still visible. Before the config has loaded, the handler SHALL change no flag.

#### Scenario: Terminal zone flips the terminal flag
- **GIVEN** the focus zone is `terminal` and both flags are off
- **WHEN** the user presses `leader f`
- **THEN** the terminal reading flag SHALL be on and the markdown reading flag SHALL stay off
- **AND** the active main terminal SHALL fit its pane with the cap
- **AND** an info toast SHALL read `Reading mode: terminal on`

#### Scenario: Panel zone flips the markdown flag
- **GIVEN** the focus zone is `panel` and both flags are off
- **WHEN** the user presses `leader f`
- **THEN** the markdown reading flag SHALL be on and the terminal reading flag SHALL stay off
- **AND** an info toast SHALL read `Reading mode: markdown on`

#### Scenario: Sidebar zone changes nothing
- **GIVEN** the focus zone is `sidebar`
- **WHEN** the user presses `leader f`
- **THEN** both flags SHALL keep their values and no toast SHALL show

#### Scenario: A toggle before the config loads changes nothing
- **GIVEN** the config has not loaded yet and the focus zone is `terminal`
- **WHEN** the user presses `leader f`
- **THEN** both flags SHALL keep their values and `save_config` SHALL not be called

#### Scenario: Turning the terminal flag off restores the full width
- **GIVEN** the terminal reading flag is on, the pane fits 180 columns and the terminal uses 100
- **WHEN** the user presses `leader f` with the focus zone `terminal`
- **THEN** `resize_session_terminal` SHALL receive `cols = 180`

### Requirement: Reading flags persist in the config
The config file SHALL hold three frontend-owned booleans, `reading_mode_terminal`, `reading_mode_markdown` and `reading_mode_full_width_tables`. A config without them SHALL read the first two as `false` and `reading_mode_full_width_tables` as `true`. Settings > Appearance SHALL show a switch `Full-width tables in reading mode` bound to `reading_mode_full_width_tables`. A toggle SHALL send only the changed key to `save_config`. The first spawn of a main terminal SHALL wait for the config to load, for at most 1 second, so the agent starts at the loaded column count. When the config has not loaded after 1 second, the terminal SHALL spawn with the flag off, and it SHALL fit its pane again when the config loads.

#### Scenario: Missing keys read as their defaults
- **GIVEN** a config file without `reading_mode_terminal`, `reading_mode_markdown` and `reading_mode_full_width_tables`
- **WHEN** the config loads
- **THEN** both reading flags SHALL be `false` and `reading_mode_full_width_tables` SHALL be `true`

#### Scenario: A toggle saves only its key
- **GIVEN** the terminal reading flag is off
- **WHEN** the terminal flag turns on
- **THEN** `save_config` SHALL receive `{ reading_mode_terminal: true }` and no other key

#### Scenario: Flags survive a restart
- **GIVEN** the config file holds `reading_mode_terminal: true` and `reading_mode_markdown: true`, and the pane fits 180 columns
- **WHEN** Nergal starts and the active main terminal spawns
- **THEN** the spawn SHALL request 100 columns
- **AND** markdown views SHALL render with the `100ch` cap

#### Scenario: A late config still applies
- **GIVEN** the config file holds `reading_mode_terminal: true` and the config takes more than 1 second to load
- **WHEN** the active main terminal spawns at 180 columns and the config then loads
- **THEN** the terminal SHALL fit its pane again and `resize_session_terminal` SHALL receive `cols = 100`
