Linear: FRI-611 (parent FRI-600)

## Why

Nergal advertises sixel, and wezterm-term already decodes sixel and iTerm2 inline images (OSC 1337) and stores them on the cells. The snapshot that feeds the canvas drops them (`CellSnapshot` has no image data), so an app such as yazi or chafa sends an image and nothing appears. This is the first of three parts of FRI-600; FRI-612 animates GIFs and FRI-613 turns on kitty graphics.

## What Changes

- **Placements in the grid update**: each `GridRow` carries the image runs it shows: image hash, first column, cell span, the source rectangle as fractions of the image, the cell padding, and `z`. Rows without images carry nothing extra. The differ treats a placement change as a row change.
- **Bytes once, on demand**: a new command `terminal_image(session_id, hash)` returns one image as a raw IPC response: a small header (kind, width, height) and then RGBA pixels (sixel) or the encoded file (iTerm2 PNG/JPEG/GIF). Image bytes never travel in a grid update.
- **Canvas drawing**: the frontend fetches an unknown image once per terminal, decodes it off the main thread with `createImageBitmap`, and draws each run in its cells. A row whose image is still decoding paints its text and repaints when the bitmap is ready. A GIF shows its first frame.
- **Memory budget**: each terminal keeps at most 256 MB of decoded images. Past the budget the least recently drawn images are dropped; their cells show text only until they come back into view, when the image is fetched again.
- **Scroll and quake**: images scroll with their lines, show in the scrollback, and work in the main terminal and in quake shells through the same code.
- No `TERM_PROGRAM` change: Nergal draws what apps already send.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-wezterm`: adds requirements for inline image placements, the image fetch command, canvas drawing and the per-terminal image budget.

## Impact

- Backend: `src-tauri/src/terminal/{types,session,differ,emitter}.rs`, `src-tauri/src/pty.rs` (the `terminal_image` command), `src-tauri/src/lib.rs` (registration).
- Frontend: `src/lib/types.ts`, `src/components/terminal/terminalService.ts`, new pure modules `src/components/terminal/imagePlacement.ts` and `src/components/terminal/imageCache.ts`.
- No config, schema or new dependency. No new OS seam (Linux-first, existing CI gates unchanged).
