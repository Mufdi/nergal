## ADDED Requirements

### Requirement: Grid rows carry inline image placements
Each `GridRow` of a `GridUpdate` SHALL list the image runs it shows, read from the wezterm-term cell attributes of that visible line. A run SHALL be a maximal sequence of adjacent cells that show contiguous slices of the same image at the same `z`. A run SHALL carry the image hash (hex SHA-256 from wezterm), the first column, the cell span, the source rectangle as fractions of the image (`u0`, `v0`, `u1`, `v1`), the padding in pixels as wezterm reports it (left and top of every cell, right of the last cell, bottom of the last row), and `z`; cells merge into a run only when their left and top padding match. `col` is the terminal column. A row without images SHALL carry no image field. A change in a row's runs SHALL make the differ resend that row.

#### Scenario: Sixel image becomes placements
- **GIVEN** an 80x24 terminal with 800x480 pixel geometry (10x20 px cells)
- **WHEN** the app writes a 64x48 sixel image at column 0 of row 0
- **THEN** rows 0, 1 and 2 each carry one run with column 0 and span 7
- **AND** row 0's run has `v0` = 0 and row 2's run has `v1` = 1

#### Scenario: iTerm2 image becomes placements
- **GIVEN** the same terminal
- **WHEN** the app writes an OSC 1337 `File=inline=1` PNG of 64x48 pixels at the cursor
- **THEN** the covered rows carry runs whose hash matches the image fetched by `terminal_image`

#### Scenario: Overwriting the cells removes the placement
- **GIVEN** row 0 shows an image run
- **WHEN** the app clears the screen (`ESC[2J`) or prints text over every covered cell
- **THEN** the next grid update resends row 0 without image runs

#### Scenario: Image rows resend when only the image changes
- **GIVEN** the differ has emitted a row showing image A
- **WHEN** the app replaces it with image B of the same size and the text is unchanged
- **THEN** the next grid update includes that row with image B's hash

### Requirement: Image bytes are fetched once per terminal
The backend SHALL keep, per terminal, a registry from image hash to the wezterm image data that visible rows reference, without extending the image's lifetime. A command `terminal_image(session_id, hash)` SHALL return that image as a raw IPC response: a header of 1 kind byte (0 = RGBA pixels, 1 = encoded file), a little-endian u32 width and a little-endian u32 height, then the bytes. Image bytes SHALL never be part of a `GridUpdate`. The command SHALL serve encoded files only when their magic bytes are PNG, JPEG or GIF. It SHALL return an error for an unknown or malformed hash, for an image no longer held by the emulator, and for any other encoded format. It SHALL NOT block the main thread or hold the global PTY maps while it copies bytes.

#### Scenario: Fetch a sixel image
- **GIVEN** a visible row shows a 64x48 sixel image with hash H
- **WHEN** the frontend calls `terminal_image(session, H)`
- **THEN** the response starts with kind 0, width 64 and height 48, followed by 12288 RGBA bytes

#### Scenario: Fetch an iTerm2 PNG
- **GIVEN** a visible row shows an iTerm2 PNG with hash H
- **WHEN** the frontend calls `terminal_image(session, H)`
- **THEN** the response starts with kind 1 and is followed by the PNG file bytes

#### Scenario: Unsupported encoded format
- **GIVEN** a visible row shows an iTerm2 image whose bytes are not PNG, JPEG or GIF
- **WHEN** the frontend calls `terminal_image(session, H)`
- **THEN** the command returns an error and the cells show text only

#### Scenario: Unknown hash
- **WHEN** the frontend calls `terminal_image(session, H)` for a hash no visible row has shown
- **THEN** the command returns an error and no bytes

### Requirement: The canvas draws inline images
The canvas SHALL draw each image run of a painted row inside its cells: the source rectangle of the decoded image, scaled to the run's cell rectangle minus its padding, after the row's background and glyphs and before the selection and the cursor. The frontend SHALL fetch an image the first time a row references its hash, decode it without blocking the paint loop, and repaint the rows that reference it once the bitmap is ready. A row whose image is pending or failed SHALL paint its text only. An animated GIF SHALL show its first frame. This SHALL apply to the main terminal and to quake shells, live and scrolled into history.

#### Scenario: Image appears after decode
- **GIVEN** a grid update brings a row with an unseen hash H
- **WHEN** the row paints before H is decoded
- **THEN** the row shows its text only
- **AND** when the bitmap for H is ready, the row repaints with the image

#### Scenario: One fetch per image per terminal
- **GIVEN** a terminal has fetched H
- **WHEN** later grid updates reference H again, including after a scroll
- **THEN** no second `terminal_image` call for H is made while H is cached

#### Scenario: Images scroll with their lines
- **GIVEN** a primary-screen image is 5 rows above the viewport top in the scrollback
- **WHEN** the user scrolls up 5 rows
- **THEN** the image is drawn in the rows it occupies

### Requirement: Each terminal caps its decoded images
Each terminal's frontend image cache SHALL hold at most 256 MB of decoded image data, counted as width × height × 4 bytes per bitmap. When an insertion exceeds the budget, the cache SHALL drop the least recently drawn images until it fits, releasing their bitmaps, but SHALL NOT drop an image drawn in the current paint pass; the cache may exceed the budget until that pass ends. A decode failure SHALL be permanent for that hash; a fetch failure SHALL be retried at most twice. A dropped image's cells SHALL show text only until a row referencing it is painted again, which fetches it again. Closing a terminal, including a quake shell, SHALL release all its bitmaps, and a fetch or decode that completes afterwards SHALL release its bitmap without drawing.

#### Scenario: Budget evicts the least recently drawn image
- **GIVEN** a cache with a 256 MB budget holding images A (drawn first) and B (drawn later), 100 MB each
- **WHEN** image C of 100 MB is decoded
- **THEN** A is dropped and B and C stay

#### Scenario: Images drawn in the same pass do not evict each other
- **GIVEN** two visible images of 150 MB each are drawn in one paint pass
- **WHEN** the second is inserted
- **THEN** neither is dropped during that pass

#### Scenario: Evicted image is fetched again when it comes back
- **GIVEN** image A was dropped by the budget
- **WHEN** a row referencing A is painted
- **THEN** the frontend fetches A again and draws it once decoded
