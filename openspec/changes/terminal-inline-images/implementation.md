# Implementation Plan: terminal-inline-images

> Grounded in the codebase at `8910e96` (branch `mufdidev/fri-611-terminal-images`, stacked on FRI-609), 2026-11-24. wezterm source: `~/.cargo/git/checkouts/wezterm-26cdb9e734a97642/5046fc2` (`WZ`), tag `20240203-110809-5046fc22`.

## Verified codebase facts (do not re-assume)

- `CellSnapshot` (`src-tauri/src/terminal/types.rs:7-16`) has no image data. `GridSnapshot` (`types.rs:46-54`) holds `rows: Vec<Vec<CellSnapshot>>`; `GridUpdate` (`types.rs:64-75`) and `GridRow { index, cells }` (`types.rs:78-82`) derive `PartialEq, Eq, Serialize`.
- Snapshot loop: `TerminalSession::grid_snapshot` (`session.rs:488`) reads `screen.lines_in_phys_range(start..end)` and iterates `line.visible_cells()` with `cell.attrs()` (`session.rs:515-537`); `attrs.images()` is available there. Rows are padded to `cols` (`:535-537`). A second `visible_cells` loop exists at `session.rs:341` (not the snapshot path).
- Differ: `compute_update` (`differ.rs:39`), `hash_row` (`differ.rs:101`), hand-written `impl Hash for CellSnapshot` (`differ.rs:107`). A row is resent when its hash changes, on first emit, on `invalidate()` and on a row-count change.
- `invalidate()` callers: resize (`pty.rs:~921`), `terminal_scroll` primary-screen branch (`pty.rs:2253-2285`), scroll-to-bottom, `terminal_attach` (`pty.rs:2427-2457`).
- Emitter: `TerminalHandle` (`emitter.rs:38`) holds `session: Arc<Mutex<TerminalSession>>`; `run_emitter` (`emitter.rs:104`) snapshots and diffs each tick and sends JSON on the channel.
- wezterm image API (`WZ/termwiz/src/image.rs`): `ImageCell` accessors `top_left()`, `bottom_right()` (`TextureCoordinate { x, y: NotNan<f32> }`), `image_data() -> &Arc<ImageData>`, `z_index()`, `padding()` (left, top, right, bottom), `image_id()`, `placement_id()` (`:81-194`). `ImageData::hash() -> [u8; 32]`, `data() -> MutexGuard<ImageDataType>` (`:499-570`); `ImageDataType` at `:199-228`. `ImageDataType::{EncodedFile(Vec<u8>), EncodedLease, Rgba8 { data, width, height, hash }, AnimRgba8 {..}}`. `CellAttributes::images() -> Option<Vec<ImageCell>>` (`WZ/termwiz/src/cell.rs:518-528`).
- Slice assignment (`WZ/term/src/terminalstate/image.rs:159-217`): adjacent cells of one row carry contiguous `x` fractions; `padding_left`/`padding_top` go to every cell, `padding_right`/`padding_bottom` only to the last column/row (`image.rs:196-207`). Sixel → `Rgba8`, z 0 (`sixel.rs`); iTerm2 stays `EncodedFile` when not downscaled or when PNG/GIF/WebP, and becomes `Rgba8` when a downscaled JPEG/BMP/other is decoded in Rust (`iterm.rs:104-121`), z 0. Printing over a cell or erasing drops its sixel/iTerm2 slice (`WZ/termwiz/src/surface/line/vecstorage.rs:19-30`, `cell.rs:474-495`).
- Frontend: `Region = "center" | "quake"` (`src/components/terminal/terminalService.ts:53`) only selects host and layout; `applyUpdate` (`:1316`), `paintAll` (`:1377`), `paintRow` (`:1385`, bg fill, glyph blit, underline), `paintSelection` (`:1434`), `paintCursor` (`:1457`). The canvas context is unscaled device pixels; cell metrics are device pixels (`fontAtlas.ts`). TS mirror of the grid types in `src/lib/types.ts:121-153`.
- Test helpers and fixtures: `differ.rs` inline builders `snapshot(rows, cursor_x)` and `row(text, width)`; `differ_mouse_tracking_tests.rs` builds `GridSnapshot` by struct literal; `pixel_geometry_tests.rs` has a 64x48 PNG base64 and the OSC 1337 sequence plus `session(pw, ph)`; `sixel_zero_pixel_tests.rs::sixel_64x48()`; `TerminalSession::new_with_pixels`. Frontend pure-module tests live beside the module (`pixelGeometry.test.ts`).
- Sync `#[tauri::command]`s run on the main thread (`pty.rs:111`); neighbours hold `state.instances.lock()` for their whole body. wezterm's `image_cache` is an LRU of 16 by hash (`WZ/term/src/terminalstate/mod.rs:360, 571`); text output does not evict it. `visible_cells()` skips wide-char continuation cells (`WZ/termwiz/src/surface/line/vecstorage.rs:92-100`). Terminal destruction sites: `destroy` (`terminalService.ts:516`) and shell removal (`:532-537`). `reshapeGrid` (`:1317-1325`), `normalizeRow` trims to `cols`.
- Tauri 2 raw responses (tauri 2.11.1, `ipc/protocol.rs:390-407`): `tauri::ipc::Response::new(Vec<u8>)`; the frontend receives an `ArrayBuffer` from `invoke`. No existing use in the repo.

## Execution order

1. Types: `ImageRun` in `types.rs` (doc: `col` is the terminal column, not an index into `cells`); `GridSnapshot.images: Vec<Vec<ImageRun>>` (`grid_snapshot` always fills it to `rows.len()`; readers still use `.get(i)` with an empty fallback, so a hand-built snapshot with `images: vec![]` stays valid); `GridRow.images: Vec<ImageRun>` with `#[serde(skip_serializing_if = "Vec::is_empty")]`. f32 fields: derive `PartialEq` only where `Eq` breaks, and hash with `f32::to_bits`.
2. Snapshot: build runs per visible line from `attrs.images()` using `cell.cell_index()` as the column; fill `TerminalSession.image_registry: HashMap<[u8; 32], Weak<ImageData>>`, pruning dead entries.
3. Differ: `hash_row` covers the row's runs; `compute_update` copies runs into `GridRow`.
4. Core `image_payload(session: &TerminalSession, hash: &[u8;32]) -> Result<Vec<u8>, ImageFetchError>` plus `parse_hash_hex` (`image_fetch.rs`). Async command `terminal_image(session_id, hash_hex) -> Result<tauri::ipc::Response, String>` in `pty.rs`: clone the handle's `Arc<Mutex<TerminalSession>>` and drop `session_ptys`/`instances`; upgrade the `Weak` under the session mutex; build the payload in `spawn_blocking` (`Rgba8` → kind 0 with width/height; `EncodedFile` whose magic bytes are PNG, JPEG or GIF → kind 1 with the pixel size from `ImageDataType::dimensions()` (termwiz `image.rs:344`); anything else → error). Register in `lib.rs`.
5. Frontend types (`src/lib/types.ts`): `ImageRun`, `GridRow.images?`.
6. `src/components/terminal/imagePlacement.ts`: pure `runRects(run, imgW, imgH, cellW, cellH, rowY)` → source and destination rects.
7. `src/components/terminal/imageCache.ts`: pure-logic cache with injected `fetch(hash)` and `decode(bytes)`; budget 256 MB; `touch(hash)` on draw; `setVisible(hashes)` after every grid update (the hashes in `rowImages`); eviction only of entries not in the visible set, oldest-drawn first, with overshoot while the visible set exceeds the budget, up to a hard ceiling of 512 MB above which new images are not decoded (transport-failure state, retryable); an entry enters with the current draw tick; in-flight dedupe; decode failures permanent; transport failures retried up to 2 more times; `dispose()` sets `disposed` and closes every bitmap; a fetch resolving after it skips the decode, a decode resolving after it closes its bitmap, neither notifies. The cache is built with the terminal's attach key (`key` at `terminalService.ts:476`, `${sessionId}::${shellId}` for a quake shell, `:532`), and its fetch calls `terminal_image` with that key, never `ownerSessionId`.
   Also in `imagePlacement.ts`: pure `applyRowImages(rowImages, update)` (`cols` and `totalRows` come from the update) that reshapes with the grid, sets `rowImages[i] = row.images ?? []` for every received row, and drops runs past `cols`.
8. `terminalService.ts`: entry gets a cache and a `rowImages: ImageRun[][]` shadow; `applyUpdate` calls `applyRowImages`, then `cache.setVisible(...)`; `paintRow` draws ready runs after glyphs (z ≥ 0) and before selection/cursor; on `ready(hash)` repaint rows that reference it, then selection and cursor; dispose the cache in both destruction sites (`destroy` and quake shell removal).

## Plan

- **types.rs / session.rs / differ.rs**: as in the order above. Update struct literals in existing tests (`differ.rs` builders, `differ_mouse_tracking_tests.rs`) with `images: vec![]`.
- **pty.rs**: `terminal_image` next to `terminal_attach`; hex-decode the hash (64 chars, else error).
- **Frontend**: the cache fetch calls `invoke<ArrayBuffer>("terminal_image", { sessionId: key, hash })` with the attach key; decode: kind 0 → `createImageBitmap(new ImageData(new Uint8ClampedArray(buf, 9), w, h))`, kind 1 → `createImageBitmap(new Blob([new Uint8Array(buf, 9)]), { imageOrientation: "none" })`.

## Builder coordination

- Interface fixed for the test-writer and both builders: Rust `ImageRun { hash, col, span, u0, v0, u1, v1, pad_left, pad_top, pad_right, pad_bottom, z }` (camelCase JSON); new module `src-tauri/src/terminal/image_fetch.rs` with `parse_hash_hex`, `image_payload(&TerminalSession, &[u8;32])`, `ImageFetchError { MalformedHash, UnknownHash, Dropped, UnsupportedFormat }` and `ImageRegistry { insert, get }` (the registry lives in `TerminalSession`). TS `ImageRun` in `src/lib/types.ts`; `imagePlacement.ts` (`runRects`, `applyRowImages`); `imageCache.ts` (`IMAGE_BUDGET_BYTES`, `IMAGE_CEILING_BYTES`, `parseImageHeader`, `createImageCache({ key, fetch, decode, onReady })` → `{ get, touch, setVisible, dispose, bytes }`).
- Group 1+2 (Rust) and group 3 (frontend) touch disjoint files and may run in parallel; the JSON shape above is their only contract.

## Per-phase risk

- [`Eq` derives on grid types break with f32] → drop `Eq` where needed; nothing relies on `Eq` beyond tests (check with `cargo check`).
- [A run spans a wide character cell] → column from `cell.cell_index()`, span counts cells, matching how wezterm assigned slices.
- [Pending decode repaint races a newer grid update] → repaint reads the current `rowImages`, not a captured row.
- [The async command needs `State` across await] → take `AppHandle`/`State` per Tauri's async rules and clone the `Arc` before any await.
- [Budget eviction thrash with large visible images] → no eviction of hashes in the visible set; hard ceiling 512 MB; tests cover both.
- [Raw `Response` not mapped to ArrayBuffer in this Tauri version] → the Red test for the command checks the bytes in Rust; the walk checks the frontend path; fall back to `Vec<u8>` JSON only if the raw path fails (record in evidence).

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh mufdidev/fri-609-git-panel-agent-actions terminal-inline-images` (stacked base).
- Manual walk on the installed `.deb` with `chafa`/`img2sixel` and yazi (tasks group Verification).
