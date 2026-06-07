## MODIFIED Requirements

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
