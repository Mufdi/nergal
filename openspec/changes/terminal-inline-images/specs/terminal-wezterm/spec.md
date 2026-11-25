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
The backend SHALL keep, per terminal, a registry from image hash to the wezterm image data that visible rows reference, without extending the image's lifetime. A command `terminal_image(session_id, hash)` SHALL return that image as a raw IPC response that the frontend receives as bytes (an `ArrayBuffer`, never a JSON array): a header of 1 kind byte (0 = RGBA pixels, 1 = encoded file), a little-endian u32 width and a little-endian u32 height (the image's pixel size for both kinds), then the bytes. Image bytes SHALL never be part of a `GridUpdate`. It SHALL return an error for an unknown or malformed hash and for an image no longer held by the emulator. It SHALL NOT block the main thread or hold the global PTY maps while it copies or decodes bytes.

#### Scenario: Fetch a sixel image
- **GIVEN** a visible row shows a 64x48 sixel image with hash H
- **WHEN** the frontend calls `terminal_image(session, H)`
- **THEN** the response starts with kind 0, width 64 and height 48, followed by 12288 RGBA bytes

#### Scenario: Fetch an iTerm2 PNG
- **GIVEN** a visible row shows an iTerm2 PNG with hash H
- **WHEN** the frontend calls `terminal_image(session, H)`
- **THEN** the response starts with kind 1, width 64 and height 48, and is followed by the PNG file bytes

#### Scenario: Unknown hash
- **WHEN** the frontend calls `terminal_image(session, H)` for a hash no visible row has shown
- **THEN** the command returns an error and no bytes

### Requirement: The backend narrows encoded formats before WebKit
`terminal_image` SHALL serve an encoded file as kind 1 only when its magic bytes are PNG, JPEG or GIF. It SHALL decode a TIFF (which `chafa -f iterm` sends), BMP or WebP in the backend and serve it as kind 0 RGBA, refusing, before the pixels are decoded, an image whose width × height × 4 exceeds 100,000,000 bytes (wezterm's per-image cap, so nothing wezterm placed is refused), and bounding the decoder's working memory at 300,000,000 bytes. Any other encoded format, and a file it cannot decode, SHALL return an error. At most 2 backend decodes SHALL run at once across the app; a further fetch waits for a free slot.

#### Scenario: Fetch an iTerm2 TIFF
- **GIVEN** a visible row shows an iTerm2 TIFF of 64x48 pixels with hash H
- **WHEN** the frontend calls `terminal_image(session, H)`
- **THEN** the response starts with kind 0, width 64 and height 48, followed by 12288 RGBA bytes

#### Scenario: Unsupported encoded format
- **GIVEN** a visible row shows an iTerm2 image whose bytes are not PNG, JPEG or GIF and that the backend cannot decode
- **WHEN** the frontend calls `terminal_image(session, H)`
- **THEN** the command returns an error and the cells show text only

#### Scenario: JPEG and GIF stay encoded
- **GIVEN** visible rows show an iTerm2 64x48 JPEG with hash J and an iTerm2 64x48 GIF with hash G, neither downscaled
- **WHEN** the frontend calls `terminal_image` for J and for G
- **THEN** each response starts with kind 1, width 64 and height 48, followed by the file bytes

#### Scenario: Other encoded formats are refused
- **GIVEN** a visible row shows an iTerm2 image in a format other than PNG, JPEG, GIF, TIFF, BMP or WebP (for example a binary PNM)
- **WHEN** the frontend calls `terminal_image(session, H)`
- **THEN** the command returns an error

#### Scenario: A large single-strip TIFF decodes
- **GIVEN** a visible row shows a deflate-compressed 5000x5000 RGB TIFF written as one strip, placed at its native pixel size
- **WHEN** the frontend calls `terminal_image(session, H)`
- **THEN** the response starts with kind 0, width 5000 and height 5000, followed by 100,000,000 RGBA bytes

#### Scenario: Fetch an iTerm2 WebP
- **GIVEN** a visible row shows an iTerm2 64x48 WebP with hash H
- **WHEN** the frontend calls `terminal_image(session, H)`
- **THEN** the response starts with kind 0, width 64 and height 48, followed by 12288 RGBA bytes

#### Scenario: Backend decodes are bounded
- **GIVEN** both backend decode slots are taken
- **WHEN** a fetch of a visible TIFF starts
- **THEN** it does not complete while both slots stay taken
- **AND** it completes with kind 0 once a slot frees

### Requirement: The frontend bounds image fetches
Each terminal's image cache SHALL keep at most 2 `terminal_image` fetches in flight; a further hash that a row references SHALL wait and start when a fetch settles, without a new grid update. The count SHALL include requests still running from before a release of the cache's bitmaps, so hiding and showing a terminal never stacks more than 2 requests. A queued hash that no current row references when a slot frees SHALL be dropped instead of fetched. A response that is not bytes, or an error whose message names an unsupported image format, SHALL be permanent for that hash, and the first non-bytes response of each cache SHALL be reported once on the console.

#### Scenario: Third fetch waits for a slot
- **GIVEN** a cache with fetches of A and B in flight
- **WHEN** a row references C
- **THEN** no fetch of C starts until A or B settles
- **AND** once A settles, the fetch of C starts and C is drawn when decoded

#### Scenario: Requests from before a release still count
- **GIVEN** a cache with fetches of A and B in flight
- **WHEN** the terminal is hidden (bitmaps released) and shown again, and a row references C
- **THEN** no fetch of C starts until A or B settles

#### Scenario: A queued hash that left the screen is not fetched
- **GIVEN** fetches of A and B in flight and C and D queued
- **WHEN** a grid update leaves only D referenced, and A settles
- **THEN** D is fetched and C is not

#### Scenario: Unsupported format is not retried
- **GIVEN** the fetch of H fails with an unsupported image format error
- **WHEN** later grid updates still reference H
- **THEN** no second fetch of H is made

#### Scenario: A non-bytes response is reported once
- **GIVEN** the transport returns a JSON number array instead of bytes for A and for B
- **THEN** A and B are permanent failures, their cells show text, and one console warning is written

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
Each terminal's frontend image cache SHALL hold at most 256 MB of decoded image data, counted as width × height × 4 bytes per bitmap from the response header, reserved when its decode starts and released if the decode fails. When an insertion exceeds the budget, the cache SHALL drop the least recently drawn images until it fits, releasing their bitmaps, but SHALL NOT drop an image that a current row references; the cache may exceed the budget while referenced images alone exceed it. A decode failure SHALL be permanent for that hash; a fetch failure SHALL be retried at most twice, at most once per grid update. With 512 MB or more held (decoded plus reserved), the cache SHALL NOT start decoding a new image even when a row references it; while another decode is in flight, it SHALL NOT start one that would take the total past 512 MB. Such an image's cells show text until the cache shrinks. A single decode started below the ceiling may cross it by at most one image (wezterm caps one image at 100 MB). A dropped image's cells SHALL show text only until a row referencing it is painted again, which fetches it again. A terminal that is hidden (another session or shell shown in its place) SHALL release its bitmaps and fetch them again when shown. Closing a terminal, including a quake shell, SHALL release all its bitmaps, and afterwards a completing fetch SHALL skip the decode and a completing decode SHALL release its bitmap, both without drawing.

#### Scenario: Budget evicts the least recently drawn image
- **GIVEN** a cache with a 256 MB budget holding images A (drawn first) and B (drawn later), 100 MB each, neither referenced by a current row
- **WHEN** image C of 100 MB is decoded
- **THEN** A is dropped and B and C stay

#### Scenario: Concurrent decodes count toward the ceiling
- **GIVEN** a cache holding 500 MB of referenced images
- **WHEN** two referenced 10 MB images finish fetching before either decode resolves
- **THEN** only the first is decoded and the second waits for room

#### Scenario: Hidden terminal releases its images
- **GIVEN** a terminal showing an image
- **WHEN** the user switches to another session
- **THEN** the hidden terminal's bitmaps are released
- **AND** switching back fetches and draws the image again

#### Scenario: Referenced images are never evicted
- **GIVEN** rows reference images A and B of 150 MB each, both cached
- **WHEN** a 10 MB image C, also referenced, finishes decoding during an update that repaints none of A's or B's rows
- **THEN** A, B and C stay cached
- **AND** once A is no longer referenced, the next insertion drops A

#### Scenario: Evicted image is fetched again when it comes back
- **GIVEN** image A was dropped by the budget
- **WHEN** a row referencing A is painted
- **THEN** the frontend fetches A again and draws it once decoded
