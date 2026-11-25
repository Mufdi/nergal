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

- Interface fixed for the test-writer and both builders: Rust `ImageRun { hash, col, span, u0, v0, u1, v1, pad_left, pad_top, pad_right, pad_bottom, z }` (camelCase JSON); new module `src-tauri/src/terminal/image_fetch.rs` with `parse_hash_hex`, `image_payload(&TerminalSession, &[u8;32])`, `ImageFetchError { MalformedHash, UnknownHash, Dropped, UnsupportedFormat }` and `ImageRegistry { insert, get }` (the registry lives in `TerminalSession`). TS `ImageRun` in `src/lib/types.ts`; `imagePlacement.ts` (`runRects`, `applyRowImages`); `imageCache.ts` (`IMAGE_BUDGET_BYTES`, `IMAGE_CEILING_BYTES`, `parseImageHeader`, `createImageCache({ key, fetch, decode, onReady })` → `{ get, touch, setVisible(hashes): string[] /* hashes re-requested */, releaseAll, dispose, bytes }`).
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

## Revision 2: Walk 4.3 fixes (2026-11-25)

### Verified codebase facts (do not re-assume)

- `src-tauri/tauri.conf.json:15-16`: `csp` and `devCsp` both carry `connect-src 'self'`; nothing else in the repo asserts the CSP string.
- Tauri's IPC script tries `fetch(convertFileSrc(cmd, 'ipc'))` first and, on any failure (CSP included), sets `customProtocolIpcFailed` and uses `window.ipc.postMessage` for every later call (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tauri-2.11.1/scripts/ipc-protocol.js:24-80`). On that path `can_use_channel_for_response` is false and a `Raw` body is formatted with `format_result` as JSON (`tauri-2.11.1/src/ipc/protocol.rs:337-339, 390-403`). tauri-codegen 2.6.1 does not add `ipc:` to `connect-src` (no match for `ipc.localhost` outside docs).
- Probe (2026-11-25, temporary test removed): `chafa -f sixels` output fed to `TerminalSession::new_with_pixels(100, 30, 900, 600)` gives one run on each covered row, the differ emits them, and `payload_for` returns kind 0. `chafa -f iterm` sends a TIFF (`II*\0`, base64 `SUkqAA`), wezterm keeps it as `EncodedFile`, and `payload_for` returns `UnsupportedFormat`.
- `payload_for` (`src-tauri/src/terminal/image_fetch.rs:93-112` after Revision 2) matches `Rgba8` → kind 0, `EncodedFile` passing `is_allowed_encoding` (`:152-157`) → kind 1, any other `EncodedFile` → bytes copied out of the `image.data()` guard and decoded by `decode_to_rgba` (`:131-150`); it runs inside `spawn_blocking` in `terminal_image` (`src-tauri/src/pty.rs:2489`). Before Revision 2 it held the guard for its whole body and returned `UnsupportedFormat` for every other encoded file.
- `image` 0.24.9 is in `Cargo.lock` (`:2475`) through wezterm-term (`image = "0.24.6"`, default features, `WZ/term/Cargo.toml:25`), with `tiff 0.9.1`, `gif`, `jpeg-decoder`, `png 0.17`, `qoi`, `exr`. wezterm refuses any image with `w*h*4 > 100_000_000` (`WZ/term/src/terminalstate/image.rs:273-280`) and reads dimensions with the same crate before placing an iTerm2 image (`WZ/term/src/terminalstate/iterm.rs:25-38`); it fully decodes an image it downscales (`iterm.rs:104-118`), and image 0.24 fully decodes a WebP even to read its dimensions.
- The locked `image_fetch_tests.rs` asserts `UnsupportedFormat` for a small BMP that wezterm keeps encoded; under D8 that BMP decodes, so that case moves to bytes the decoder rejects.

### Execution order

1. `tauri.conf.json`: `connect-src 'self' ipc: http://ipc.localhost` in `csp` and `devCsp`; nothing else in either policy changes.
2. `src-tauri/Cargo.toml`: `image = "0.24"` as a direct dependency with the features wezterm already enables; `cargo tree -i image` must still show a single 0.24.9 next to the existing 0.25.10, and `Cargo.lock` must gain no new package.
3. `image_fetch.rs`: copy the encoded bytes out of the `image.data()` guard, drop it, and for an `EncodedFile` that is not PNG/JPEG/GIF decode with `image::io::Reader` (guessed format, `Limits` with `max_alloc` at 100,000,000) to RGBA8; build a kind 0 payload with the decoded width and height. A decode error or an RGBA size above 100,000,000 returns `UnsupportedFormat`. PNG/JPEG/GIF keep the kind 1 path.

### Per-phase risk

- [Switching the whole app from `postMessage` to the IPC protocol] → it is Tauri's default transport; the walk adds rows for sessions, panels, Channels (terminal grid) and the updater check, and the CSP test pins the exact `connect-src`.
- [A crafted TIFF or other file costs CPU and memory in the decoder] → the decode runs in `spawn_blocking`, never under the session mutex or the global PTY maps; `max_alloc` bounds the decoder buffer, and Revision 3 bounds concurrency and narrows the formats.
- [Decoded size differs from wezterm's reported size] → the header carries the decoded width and height; slices use fractions, so the draw stays correct.

## Revision 3: Review fixes for the walk fixes (2026-11-25)

### Verified codebase facts (do not re-assume)

These facts describe the code before group 8; the code after it is anchored under Revision 4.

- Review: `handoff/REVIEW-walk1.md` (2 Important, 11 Nit, VERDICT FAIL).
- `terminal_image` (`src-tauri/src/pty.rs:2461-2494`) awaits `spawn_blocking(move || payload_for(&image))` with no concurrency bound; Tokio's blocking pool allows up to 512 threads.
- `decode_to_rgba` (`image_fetch.rs:131-150`) reads dimensions with a first `guessed_reader`, then decodes with a second under `Limits.max_alloc = 100_000_000`; `guessed_reader` (`:123-129`) exposes `format()` on the reader before decoding.
- `release` profile sets `panic = "abort"` (`src-tauri/Cargo.toml:168`), so a decoder panic ends the process.
- `imageCache.ts`: `startFetch` (`:142-159`) starts one fetch per new hash with no cap; the rejection handler counts every failure as retryable (`:152-157`, `isRetryable` `:64-66`); `onFetched` (`:161-205`) parses `buf` with `parseImageHeader` inside `try`, so a non-`ArrayBuffer` becomes `dead` with no trace.
- The over-limit test in `image_fetch_tests.rs` builds a 6000x5000 RGB TIFF: its decoder buffer is 90,000,000 bytes, below `max_alloc`, so without the pre-check the decoder may allocate it before failing on the missing strip.

### Execution order

1. `image_fetch.rs`: a process-wide `tokio::sync::Semaphore` with 2 permits (`DECODE_PERMITS`), exposed as `pub fn decode_slots() -> &'static Semaphore`, and `pub async fn payload_for_bounded(image: Arc<ImageData>) -> Result<Vec<u8>, ImageFetchError>` that acquires a permit, then runs `payload_for` in `spawn_blocking` (join error → `UnsupportedFormat`). `terminal_image` calls it.
2. `decode_to_rgba`: one reader, `with_guessed_format`; refuse unless `format()` is `Tiff`, `Bmp` or `WebP`; decode under `max_alloc`; drop the dimension pre-check.
3. `imageCache.ts`: `MAX_IN_FLIGHT_FETCHES = 2`; a hash referenced while 2 fetches are in flight enters a `queued` state and starts, oldest first, when a fetch settles; a rejection whose message contains `unsupported image format` becomes `dead`; a fetch result that is not an `ArrayBuffer` becomes `dead` and the first one writes one `console.warn`.
4. Docs: the D5/D8 risk lines and these facts match the code after the work lands.

### Per-phase risk

- [A queued fetch starves when the cache is disposed or released] → `releaseAll` and `dispose` clear the queue with the entries; a settle after an epoch change does not start queued work from the old epoch.
- [Semaphore held across a long decode stalls sixel fetches] → 2 permits; a sixel payload is a copy and returns fast once a permit frees.

## Revision 4: Second review fixes (2026-11-25)

### Verified codebase facts (do not re-assume)

- Review: `handoff/REVIEW-walk1-r2.md` (1 Important, 9 Nit, VERDICT FAIL).
- Group 8 code: `DECODE_PERMITS`, `decode_slots()` and `payload_for_bounded` (`src-tauri/src/terminal/image_fetch.rs:14-32`), called by `terminal_image` (`src-tauri/src/pty.rs:2489`); the runtime allowlist (`image_fetch.rs:147-152`); `payload_for` clones a non-allowlisted encoded file under the guard (`:127`).
- `imageCache.ts` after group 8: module-level `warnedNonBuffer` (`:10`); `startFetch`/`drainQueue` (`:149-163`); `isUnsupported` (`:165-168`); `beginFetch` settles return early on an older epoch before decrementing `inFlight` (`:178`, `:184`); non-ArrayBuffer branch (`:198-206`); `releaseAll` sets `inFlight = 0` (`:283`); `requestRetries` (`:132-147`) checks each blocked hash against the live `decoding` count.
- `hideEntry` calls `releaseAll` on a session switch (`src/components/terminal/terminalService.ts:278`), and `setRegionVisible` does on quake close (`:269`).

### Execution order

1. `imageCache.ts`: decrement `inFlight` on every settle before the epoch check (return early only when disposed), run `onFetched`/the failure branch only for the current epoch, then `drainQueue`; remove `inFlight = 0` from `releaseAll`. `drainQueue` drops a queued hash that is not in `visible`. `requestRetries` treats the loop's own restarts as in-flight decodes for the ceiling check. Move the warn flag into the `createImageCache` closure.
2. `image_fetch.rs`: move the permit into the `spawn_blocking` closure; under the `image.data()` guard, return `UnsupportedFormat` before the clone when `image::guess_format` is not TIFF, BMP or WebP.
3. Docs: anchors for the new code after it lands.

### Per-phase risk

- [A settle from an old epoch drains the queue] → `releaseAll` already replaced the queue, so it only starts work queued after the release.
- [Dropping a non-visible queued hash loses it] → its entry is removed, so the next `get` from a paint starts a fresh fetch.

### Code after group 9 (anchors)

- `src-tauri/src/terminal/image_fetch.rs`: `MAX_DECODED_BYTES` `:12`, `DECODE_PERMITS` `:14`, `DECODE_SLOTS`/`decode_slots()` `:16-21`, `payload_for_bounded` (permit moved into the blocking closure) `:24`, `payload_for` (format guessed before the clone) `:116`, `pack` `:145`, `decode_to_rgba` (single reader, Tiff/Bmp/WebP allowlist, `max_alloc`) `:154`, `is_allowed_encoding` `:177`. `terminal_image` calls `payload_for_bounded` at `src-tauri/src/pty.rs:2489`.
- `src/components/terminal/imageCache.ts`: `MAX_IN_FLIGHT_FETCHES` `:7`; `visibilityKnown` `:75` (a queued hash is dropped only once `setVisible` has run, set at `:276`); per-cache `warnedNonBuffer` `:84`; `overCeiling(size, held, restarted)` `:114`; `requestRetries` with its restart count `:132`; `startFetch` `:151`; `drainQueue` `:160`; `isUnsupported` `:169`; `beginFetch` (decrements on every settle, acts only for the current epoch) `:174`; `onFetched` `:204`; `releaseAll` (keeps the in-flight count) `:280`.
- Builder deviation (group 9): `visibilityKnown`. Before the first `setVisible` the cache has no visible set, so the queue keeps every hash; in the app `applyUpdate` calls `setVisible` before any paint fetches.
- `src-tauri/tauri.conf.json:15-16`: `connect-src 'self' ipc: http://ipc.localhost` in `csp` and `devCsp`. `src-tauri/Cargo.toml:15`: `image = "0.24"`.
- Group 10: `beginFetch` success handler feeds old-epoch bytes to a hash the current epoch has queued (`src/components/terminal/imageCache.ts:184`).

## Revision 6: Large TIFF decode (2026-11-25)

### Verified codebase facts (do not re-assume)

- `image-0.24.9/src/codecs/tiff.rs:210-228`: `TiffDecoder::set_limits` gives `decoding_buffer_size = total_bytes_buffer()` and `intermediate_buffer_size = max_alloc - total_bytes_buffer()`; a single-strip image needs an intermediate buffer as large as the image.
- Probe (temporary, removed): `burst.py 2 5000` output placed in a 150x40 session at 1500x800 px gives 2 hashes with runs; `payload_for` returns `UnsupportedFormat`; the same bytes decode with `max_alloc` 1 GB or none (5000x5000 Rgb8, 0.46 s). `burst.py 2 600` works today.
- Constructors: `TiffDecoder::new` (`tiff.rs:36+`), `BmpDecoder::new` (`bmp/decoder.rs:530`), `WebPDecoder::new` (`webp/decoder.rs:131`, decodes the frame on construction); `image::DynamicImage::from_decoder` decodes any `ImageDecoder`.
- `decode_to_rgba` today: `src-tauri/src/terminal/image_fetch.rs:154`.

### Execution order

1. `image_fetch.rs`: `MAX_DECODED_BYTES` stays the image cap; add a working-memory constant of 300,000,000. `decode_to_rgba` builds the decoder for the guessed format (Tiff, Bmp, WebP; anything else `UnsupportedFormat`), checks `w*h*4` against `MAX_DECODED_BYTES`, sets `Limits.max_alloc` to the working constant, decodes with `DynamicImage::from_decoder`, converts to RGBA8.

### Code after group 11 (anchors)

- `src-tauri/src/terminal/image_fetch.rs`: `MAX_DECODER_WORK_BYTES` `:14`; `decode_to_rgba` (per-format decoder from `image::guess_format`) `:156`; `decode_with` (header check against `MAX_DECODED_BYTES`, then `max_alloc` at the working limit, `DynamicImage::from_decoder`) `:169`.
