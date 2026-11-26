## 1. Placements in the snapshot and the differ

- [x] 1.1 Test (Red): `src-tauri/src/terminal/image_runs_tests.rs` (`#[path]` module of `session.rs`): on an 80x24 session with 800x480 pixel geometry, a 64x48 sixel at the origin gives runs on rows 0–2, each column 0 and span 7, row 0 `v0` = 0 and row 2 `v1` = 1, `z` 0, one shared hash; an OSC 1337 inline 64x48 PNG gives runs with the PNG's hash; `ESC[2J` after either leaves no runs; printing text over all covered cells of row 0 leaves row 0 without runs; printing 2 characters at columns 2–3 of row 0 leaves two runs on row 0 (column 0 span 2, column 4 span 3) whose `u` ranges stay contiguous with the original slices; rows without images have empty runs; a hash renders as 64 lowercase hex characters.
- [x] 1.2 Test (Red): `src-tauri/src/terminal/differ_image_tests.rs` (`#[path]` module of `differ.rs`): a row whose runs change while its cells stay equal is resent; a row with unchanged runs is not; `GridRow` JSON omits `images` when empty and includes it when not.
- [x] 1.3 Add `ImageRun`, `GridSnapshot.images` and `GridRow.images` in `src-tauri/src/terminal/types.rs`; update existing struct literals in tests.
- [x] 1.4 Build runs and fill the image registry in `TerminalSession::grid_snapshot` (`session.rs`).
- [x] 1.5 `differ.rs`: hash runs in `hash_row` and copy them into `GridRow`.

## 2. Image fetch command

- [x] 2.1 Test (Red): `src-tauri/src/terminal/image_fetch_tests.rs` (each error case first asserts that a row carries a run with that hash, then matches the error variant: unsupported format, unknown hash, malformed hash, dropped image): the command core returns `[0, 64,0,0,0, 48,0,0,0]` + 12288 RGBA bytes for a visible 64x48 sixel; returns kind 1 + the PNG bytes for an iTerm2 PNG, also when wezterm downscales it (64x48 PNG on a 320x160 geometry or with `width=`), with the native-size bytes; returns an error for an unknown hash, for a malformed hex hash (63 chars, non-hex), for an encoded image whose bytes are not PNG/JPEG/GIF (a BMP small enough that wezterm keeps it encoded: no downscale, no `width`/`height` arguments), and for a registry entry whose `Arc<ImageData>` was dropped (registry unit test with an owned `Arc`); after `ESC[2J` followed by 16 distinct sixels, the first image's hash returns an error.
- [x] 2.2 Add the registry lookup, hash parsing, magic-byte allowlist and payload builder, and the async `terminal_image(session_id, hash)` command (clone the session `Arc`, drop the global maps, copy in `spawn_blocking`) returning `tauri::ipc::Response` in `src-tauri/src/pty.rs`; register it in `src-tauri/src/lib.rs`.

## 3. Frontend cache and rects

- [x] 3.1 Test (Red): `src/components/terminal/imagePlacement.test.ts`: `runRects` for a run at column 3, span 7, `u0..u1` = 0.2..0.9, `v0..v1` = 0..0.4, padding right 4, on a 100x50 image with 10x20 cells at row y 2 gives the expected source and destination rectangles in device pixels. `applyRowImages`: a received row without `images` clears that row's runs; rows not in the update keep theirs; a row-count or `cols` change reshapes; runs starting at or past `cols` are dropped.
- [x] 3.2 Test (Red): `src/components/terminal/imageCache.test.ts` with injected fetch/decode fakes: one fetch per hash while in flight or cached; `ready` callback with the hash; header parse (kind 0 RGBA with width/height, kind 1 encoded); budget 256 MB evicts the least recently drawn (A drawn first, B later, neither in the visible set, C inserted → A dropped, bitmap closed); with A and B (150 MB each) in the visible set, a referenced 10 MB C inserted during an update that touches neither keeps all three; after `setVisible` drops A, the next insertion evicts A; with 512 MB already decoded and referenced, a new referenced image is not decoded (fetch failure state) and is retried after the visible set shrinks; an entry whose decode resolves after its row stopped referencing it enters with the current tick; an evicted hash is fetched again on the next `request`; a decode failure is never retried; a fetch failure is retried at most twice; `dispose()` closes every bitmap, a fetch resolving after `dispose()` never calls `decode` and fires no `ready`, and a decode resolving after `dispose()` closes its bitmap and fires no `ready`; the fetch receives the key the cache was built with.
- [x] 3.3 Add `imagePlacement.ts` and `imageCache.ts`.
- [x] 3.4 `src/lib/types.ts`: `ImageRun` and `GridRow.images?`.
- [x] 3.5 `terminalService.ts`: per-entry cache and `rowImages`; `applyUpdate` calls `applyRowImages` then `setVisible`; the cache and its fetch use the attach key (`${sessionId}::${shellId}` for quake); `paintRow` draws ready runs after glyphs and underline (z ≥ 0), z < 0 before glyphs; `ready(hash)` repaints the rows referencing it, then selection and cursor; dispose the cache in `destroy` and in quake shell removal.

## 5. Review fixes

- [x] 5.1 Test (Red): `image_fetch_tests.rs` — the kind 1 header carries width 64 and height 48 for the 64x48 PNG. `image_runs_tests.rs` (or a unit test of the run builder) — two adjacent slices of one image with different left or top padding form two runs; with equal padding they form one. `imageCache.test.ts` — with 500 MB referenced, two referenced 10 MB fetches resolving before either decode: only the first decodes; a kind 1 entry counts `w*h*4` from its header; a failed fetch retries only on `setVisible`, never from repeated `get` calls within one update, and `setVisible` returns (or reports via `onReady`-compatible callback) the hashes it unblocked; `releaseAll()` closes every bitmap and a later `get` fetches again; a decode that throws synchronously marks the entry failed. `imagePlacement.test.ts` — padding at or above the span returns no rect (skip); `u`/`v` outside 0..1 are clamped. Re-lock.
- [x] 5.2 Backend: kind 1 header carries `dimensions()`; `grid_snapshot` formats each distinct hash and inserts it in the registry once per snapshot (a per-snapshot map from `Arc` pointer to hex); drop the WHAT first sentences at `pty.rs:2458`, `image_fetch.rs:33`, `session.rs:593`, `types.rs:86`.
- [x] 5.3 Frontend cache: reserve bytes at decode start from the header and release on failure; retries driven by `setVisible` only; `setVisible` repaints (through `onReady`) hashes it unblocks; `blockedCount` short-circuit; `releaseAll()`; typed body helper instead of `as ArrayBuffer`/`as BlobPart`; decode wrapper catches synchronous throws.
- [x] 5.4 `imagePlacement.ts`: clamp `u`/`v`, return no rect for a non-positive size.
- [x] 5.5 `terminalService.ts`: `imageSmoothingEnabled = false` around image draws; skip runs without a rect; short-circuit `applyRowImages`/`setVisible` work when neither the old nor the new rows hold runs; a hash-to-rows lookup for `onReady` repaints; call `releaseAll()` when a terminal is hidden and repaint on show.
- [x] 5.6 Docs: `implementation.md` and `design.md` match the code (done with this group's artifact edits).

## 6. Re-review nits

- [x] 6.1 Test (Red): `imageCache.test.ts` — with several hashes blocked by the ceiling, `setVisible` re-requests only as many as fit (one when only one fits), and when a decode finishes and frees or reserves room the cache re-checks blocked hashes without waiting for a grid update. `imagePlacement.test.ts` (or a new pure helper test) — smoothing is chosen by scale: off when the destination is the source size within 1%, on otherwise, and the source rect is snapped to whole pixels when smoothing is on.
- [x] 6.2 `imageCache.ts`: limit unblocking to what fits; re-check blocked hashes after each decode settles.
- [x] 6.3 `terminalService.ts`: smoothing chosen per run by scale with a pixel-snapped source rect; a terminal created hidden releases its bitmaps (or never fetches) until shown; a region-visibility hook (`setRegionVisible("quake", open)`) called by `QuakeTerminal.tsx` on open/close releases quake shell bitmaps and suppresses their image draws while closed, and repaints on open.
- [x] 6.4 `implementation.md` Builder coordination lists `releaseAll()` and `setVisible(): string[]`.

## 7. Walk fixes (Revision 2)

- [x] 7.1 Test (Red): a Rust test that reads `src-tauri/tauri.conf.json` and asserts that the `connect-src` directive of both `csp` and `devCsp` lists exactly `'self'`, `ipc:` and `http://ipc.localhost`. `image_fetch_tests.rs`: an iTerm2 64x48 TIFF (`File=inline=1`, no downscale) returns kind 0, width 64, height 48 and 12288 RGBA bytes whose first pixel matches the fixture; the small BMP case now returns kind 0 with its size; an encoded file the decoder rejects returns `UnsupportedFormat` (through a visible run if wezterm places it, otherwise through `payload_for` on an `ImageData` built from those bytes); an encoded file whose `w*h*4` exceeds 100,000,000 returns `UnsupportedFormat`; PNG, JPEG and GIF still return kind 1. Re-lock.
- [x] 7.2 `src-tauri/tauri.conf.json`: add `ipc: http://ipc.localhost` to `connect-src` in `csp` and `devCsp`.
- [x] 7.3 `src-tauri/Cargo.toml`: `image` 0.24 as a direct dependency, no new package in `Cargo.lock`.
- [x] 7.4 `src-tauri/src/terminal/image_fetch.rs`: decode encoded files other than PNG/JPEG/GIF to RGBA with a 100,000,000-byte limit, outside the `image.data()` guard, and serve them as kind 0.

## 8. Review fixes for the walk fixes (Revision 3)

- [x] 8.1 Test (Red): `image_fetch_tests.rs` — an iTerm2 64x48 JPEG and an iTerm2 64x48 GIF, not downscaled, each return kind 1 with width 64, height 48 and the file bytes; an iTerm2 binary PNM (or another format outside PNG/JPEG/GIF/TIFF/BMP/WebP that wezterm places) returns `UnsupportedFormat`; the over-limit TIFF test claims a decoder buffer above 100,000,000 bytes (e.g. 6000x6000 RGB) and is renamed so it no longer claims "without allocating"; with both permits of `decode_slots()` held, `payload_for_bounded` for a visible TIFF does not complete within 200 ms, and completes with kind 0 after one permit is released. `imageCache.test.ts` — with fetches of A and B pending, a reference to C starts no fetch until A settles, then C is fetched and `onReady(C)` fires after decode; a fetch rejected with `unsupported image format` is never retried by later `setVisible` calls; a fetch resolving to a number array marks the hash dead and `console.warn` is called once for two such hashes; `releaseAll()` drops queued hashes. Re-lock.
- [x] 8.2 `image_fetch.rs`: `decode_slots()` + `payload_for_bounded`; runtime format allowlist `Tiff | Bmp | WebP`; remove the dimension pre-check. `pty.rs` `terminal_image` calls `payload_for_bounded`.
- [x] 8.3 `imageCache.ts`: in-flight cap of 2 with a queue; permanent failure for the unsupported-format error and for a non-`ArrayBuffer` response, with one console warning.
- [x] 8.4 Docs: `design.md` Revision 3 and `implementation.md` Revision 3 match the code after the work lands.

## 9. Second review fixes (Revision 4)

- [x] 9.1 Test (Red): `imageCache.test.ts` — `get(a)`, `get(b)`, `releaseAll()`, then a reference to c starts no fetch of c until a or b settles; with c and d queued, `setVisible({d})` then settling a fetches d and not c; with 290 MB referenced, a 200 MB decode in flight and two 150 MB hashes blocked behind it, the failure of that decode restarts only one of them; two caches each warn once for a non-ArrayBuffer response (the warning is per cache). `image_fetch_tests.rs` — an iTerm2 64x48 WebP returns kind 0, width 64, height 48 and 12288 RGBA bytes. Re-lock.
- [x] 9.2 `imageCache.ts`: in-flight count across epochs, queue drops non-visible hashes, restart accounting in `requestRetries`, per-cache warn flag.
- [x] 9.3 `image_fetch.rs`: permit inside the blocking task; format guessed before the clone.
- [x] 9.4 Docs: `implementation.md` Revision 4 anchors match the code after the work lands; 8.4 checked.

## 10. Third review nit (Revision 5)

- [x] 10.1 Test (Red): `imageCache.test.ts` — after `releaseAll()`, an old fetch of a hash that the current epoch has queued is used: one decode, `onReady`, no second fetch. Re-lock.
- [x] 10.2 `imageCache.ts`: an old-epoch success uses its bytes when the same hash is `queued` in the current epoch.

## 11. Walk 2 fix: large TIFFs (Revision 6)

- [x] 11.1 Test (Red): `image_fetch_tests.rs` — a deflate-compressed 5000x5000 RGB TIFF written as one strip, placed visibly at its native pixel size (`width=5000px;height=5000px`), returns kind 0 with width 5000, height 5000 and 100,000,000 RGBA bytes; the over-limit header test still returns `UnsupportedFormat` fast. Re-lock.
- [x] 11.2 `image_fetch.rs`: per-format decoder, header size check against `MAX_DECODED_BYTES`, 300,000,000-byte working limit.

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- [x] 4.2 `~/.claude/skills/work/scripts/work-gates.sh mufdidev/fri-609-git-panel-agent-actions terminal-inline-images` passes.
- [x] 4.3 Manual walk: in the main terminal, `img2sixel` (or `chafa -f sixel`) on a PNG draws the image in place; `chafa -f iterm` on a PNG draws it; `clear` removes it.
- [x] 4.4 Manual walk: in yazi, moving across image files shows each preview and replaces the previous one; leaving yazi leaves no image behind.
- [x] 4.5 Manual walk: after printing an image, run `seq 100` and scroll up: the image is drawn in its rows in the scrollback, with no seams between rows; resizing the window and zooming the font with an image on screen keep the app usable; switching to another session and back redraws the image.
- [x] 4.6 Manual walk: the same `chafa -f sixels` in a quake shell draws the image; a GIF via `chafa -f iterm` shows its first frame.
- [x] 4.7 Manual walk: with the IPC protocol enabled, sessions open and type, the side panels load, the terminal scrolls, and Settings › About checks for updates without errors; images from `chafa` appear, which they cannot under the postMessage fallback (the release build has no webview console to check for "IPC custom protocol failed").
- [x] 4.8 Manual walk: `burst.py` (work folder) prints 30 distinct deflate-compressed 5000x5000 TIFFs at their native pixel size, 2 columns apart (`doNotMoveCursor=1`); up to the 512 MB frontend ceiling (about 5 of the 100 MB images) are drawn and the rest stay blank by design; the app stays usable; the main process (`ps -o rss= -p $(pgrep -o -x nergal)`) stays below about 1 GB (baseline plus 2 decodes of about 300 MB), and the web process (sum of `ps -o rss= -C WebKitWebProcess`) stays below about 1 GB (baseline plus the 512 MB ceiling plus one image).
- [x] 4.9 Manual walk: with 2 large TIFFs on screen in a quake shell, closing and opening the quake 10 times quickly brings the images back within about 2 s of the last open.
