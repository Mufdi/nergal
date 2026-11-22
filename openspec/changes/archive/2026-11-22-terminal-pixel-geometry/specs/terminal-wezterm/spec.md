## ADDED Requirements

### Requirement: Pixel geometry reaches the emulator and the PTY

The backend SHALL give the wezterm-term emulator and the PTY the canvas size in device pixels: `pixel_width = cols × cell width` and `pixel_height = rows × cell height`. The frontend SHALL gate each call on the last size it requested, recorded when the call is sent, and SHALL forget that record only when the failed call is still the latest request. The frontend SHALL send that size after it spawns or re-attaches a session terminal or a quake shell, when it resizes one, and when the cell size changes while the column and row counts stay the same. After such a cell size change, the canvas backing width SHALL equal the `pixel_width` sent, and the backing height SHALL equal the `pixel_height` sent.

#### Scenario: Spawn with pixel geometry
- **GIVEN** a session terminal spawned with 80 columns, 24 rows, `pixel_width` 800 and `pixel_height` 480
- **WHEN** the app writes `CSI 14 t`
- **THEN** the emulator SHALL answer `ESC [ 4 ; 480 ; 800 t`
- **AND** the PTY window size SHALL report `ws_xpixel` 800 and `ws_ypixel` 480

#### Scenario: Cell size report follows the geometry
- **GIVEN** a session terminal with 80 columns, 24 rows, `pixel_width` 800 and `pixel_height` 480
- **WHEN** the app writes `CSI 16 t`
- **THEN** the emulator SHALL answer `ESC [ 6 ; 20 ; 10 t`

#### Scenario: Resize carries the new geometry
- **GIVEN** a session terminal with 80 columns and 24 rows
- **WHEN** the frontend resizes it to 100 columns, 30 rows, `pixel_width` 1000 and `pixel_height` 600
- **THEN** `CSI 14 t` SHALL answer `ESC [ 4 ; 600 ; 1000 t`
- **AND** the PTY window size SHALL report `ws_xpixel` 1000 and `ws_ypixel` 600

#### Scenario: A resize with an unchanged size changes nothing
- **GIVEN** a session terminal at 80 columns, 24 rows, `pixel_width` 800 and `pixel_height` 480, where the app set a scroll region with `CSI 5 ; 10 r`
- **WHEN** the frontend resizes it to the same 80 columns, 24 rows, 800 and 480
- **THEN** the backend SHALL NOT resize the PTY master, so the child gets no `SIGWINCH`
- **AND** the backend SHALL NOT resize the emulator, so the scroll region of rows 5 to 10 stays in effect

#### Scenario: A cell size change resends the geometry and resizes the canvas
- **GIVEN** a terminal at 80 columns and 24 rows with a cell of 9 × 18 device pixels
- **WHEN** the font changes to a cell of 10 × 20 device pixels and the column and row counts stay at 80 and 24
- **THEN** the frontend SHALL call `resize_session_terminal` with 80 columns, 24 rows, `pixel_width` 800 and `pixel_height` 480
- **AND** the canvas backing size SHALL be 800 × 480, taken from the same geometry function

#### Scenario: An unchanged geometry sends nothing
- **GIVEN** a terminal that last sent 80 columns, 24 rows, 800 and 480
- **WHEN** `fit` runs again with the same container size and the same cell
- **THEN** the frontend SHALL NOT call `resize_session_terminal`

#### Scenario: A failed resize is sent again
- **GIVEN** a terminal whose last `resize_session_terminal` call with 80 columns, 24 rows, 800 and 480 was rejected
- **WHEN** `fit` runs again with the same container size and the same cell
- **THEN** the frontend SHALL call `resize_session_terminal` again with the same values

#### Scenario: Answers that resolve out of order never leave a stale size
- **GIVEN** a terminal that sent a resize to 100 columns and then a resize to 90 columns, both still pending
- **WHEN** `fit` computes 100 columns again before either call resolves
- **THEN** the frontend SHALL call `resize_session_terminal` with 100 columns, because the last requested size is 90

#### Scenario: A failure does not clear a newer request
- **GIVEN** a terminal that requested 100 columns and then 90 columns
- **WHEN** the 100-column call is rejected after the 90-column call was sent
- **THEN** the last requested size SHALL stay at 90 columns

#### Scenario: The post-spawn resize uses the current size
- **GIVEN** a terminal that spawned with 80 columns and 24 rows, and `fit` moved it to 100 columns and 30 rows while the spawn was pending
- **WHEN** the spawn resolves
- **THEN** the post-spawn `resize_session_terminal` call SHALL carry 100 columns and 30 rows

#### Scenario: Re-attaching to a live PTY applies the current geometry
- **GIVEN** a live session PTY created with no pixel size
- **WHEN** the frontend re-attaches to it with 80 columns, 24 rows and a cell of 10 × 20 device pixels
- **THEN** the frontend SHALL call `resize_session_terminal` with `pixel_width` 800 and `pixel_height` 480

### Requirement: The emulator never gets less than one pixel per cell

The backend SHALL clamp every pixel size it gives the emulator and the PTY. When the caller sends no pixel size for an axis, or a value smaller than the column count (width) or the row count (height), the backend SHALL use a fallback cell of 8 × 16 device pixels for that axis. The result SHALL saturate at 65 535 instead of overflowing. As a result, the pixel geometry Nergal provides can never cause a division by zero while wezterm-term places an inline image.

#### Scenario: Zero columns or rows are rejected
- **GIVEN** a live session terminal
- **WHEN** `resize_session_terminal` arrives with 0 columns or 0 rows
- **THEN** the backend SHALL return an error and leave the PTY and the emulator unchanged

#### Scenario: Missing geometry uses the fallback cell
- **GIVEN** a session terminal spawned with 80 columns and 24 rows and no pixel size
- **WHEN** the app writes `CSI 14 t`
- **THEN** the emulator SHALL answer `ESC [ 4 ; 384 ; 640 t`

#### Scenario: Geometry below one pixel per cell uses the fallback cell
- **GIVEN** a session terminal with 80 columns and 24 rows
- **WHEN** it is resized with `pixel_width` 0 and `pixel_height` 10
- **THEN** `CSI 14 t` SHALL answer `ESC [ 4 ; 384 ; 640 t`

#### Scenario: A huge column count saturates
- **GIVEN** a clamp request with 9000 columns, 24 rows and no pixel size
- **WHEN** the backend computes the geometry
- **THEN** `pixel_width` SHALL be 65 535 and `pixel_height` SHALL be 384

#### Scenario: Sixel image on a terminal created without pixel geometry
- **GIVEN** a session terminal with 80 columns and 24 rows and no pixel size
- **WHEN** the app writes a 64 × 48 sixel image
- **THEN** the emulator SHALL not panic
- **AND** the next grid snapshot SHALL return 24 rows

#### Scenario: iTerm2 image with less than one pixel per cell
- **GIVEN** a session terminal resized to 80 columns, 24 rows, `pixel_width` 10 and `pixel_height` 10
- **WHEN** the app writes an OSC 1337 `File=inline=1` sequence that carries a 64 × 48 PNG
- **THEN** the emulator SHALL not panic
- **AND** the next grid snapshot SHALL return 24 rows
