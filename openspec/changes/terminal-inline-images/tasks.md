## 1. Placements in the snapshot and the differ

- [ ] 1.1 Test (Red): `src-tauri/src/terminal/image_runs_tests.rs` (`#[path]` module of `session.rs`): on an 80x24 session with 800x480 pixel geometry, a 64x48 sixel at the origin gives runs on rows 0–2, each column 0 and span 7, row 0 `v0` = 0 and row 2 `v1` = 1, `z` 0, one shared hash; an OSC 1337 inline 64x48 PNG gives runs with the PNG's hash; `ESC[2J` after either leaves no runs; printing text over all covered cells of row 0 leaves row 0 without runs; printing 2 characters at columns 2–3 of row 0 leaves two runs on row 0 (column 0 span 2, column 4 span 3) whose `u` ranges stay contiguous with the original slices; rows without images have empty runs; a hash renders as 64 lowercase hex characters.
- [ ] 1.2 Test (Red): `src-tauri/src/terminal/differ_image_tests.rs` (`#[path]` module of `differ.rs`): a row whose runs change while its cells stay equal is resent; a row with unchanged runs is not; `GridRow` JSON omits `images` when empty and includes it when not.
- [ ] 1.3 Add `ImageRun`, `GridSnapshot.images` and `GridRow.images` in `src-tauri/src/terminal/types.rs`; update existing struct literals in tests.
- [ ] 1.4 Build runs and fill the image registry in `TerminalSession::grid_snapshot` (`session.rs`).
- [ ] 1.5 `differ.rs`: hash runs in `hash_row` and copy them into `GridRow`.

## 2. Image fetch command

- [ ] 2.1 Test (Red): `src-tauri/src/terminal/image_fetch_tests.rs`: the command core returns `[0, 64,0,0,0, 48,0,0,0]` + 12288 RGBA bytes for a visible 64x48 sixel; returns kind 1 + the PNG bytes for an iTerm2 PNG; returns an error for an unknown hash, for a malformed hex hash (63 chars, non-hex), for an encoded image whose bytes are not PNG/JPEG/GIF (e.g. a BMP via OSC 1337), and for a registry entry whose `Arc<ImageData>` was dropped (registry unit test with an owned `Arc`); after `ESC[2J` followed by 16 distinct sixels, the first image's hash returns an error.
- [ ] 2.2 Add the registry lookup, hash parsing, magic-byte allowlist and payload builder, and the async `terminal_image(session_id, hash)` command (clone the session `Arc`, drop the global maps, copy in `spawn_blocking`) returning `tauri::ipc::Response` in `src-tauri/src/pty.rs`; register it in `src-tauri/src/lib.rs`.

## 3. Frontend cache and rects

- [ ] 3.1 Test (Red): `src/components/terminal/imagePlacement.test.ts`: `runRects` for a run at column 3, span 7, `u0..u1` = 0.2..0.9, `v0..v1` = 0..0.4, padding right 4, on a 100x50 image with 10x20 cells at row y 2 gives the expected source and destination rectangles in device pixels. `applyRowImages`: a received row without `images` clears that row's runs; rows not in the update keep theirs; a row-count or `cols` change reshapes; runs starting at or past `cols` are dropped.
- [ ] 3.2 Test (Red): `src/components/terminal/imageCache.test.ts` with injected fetch/decode fakes: one fetch per hash while in flight or cached; `ready` callback with the hash; header parse (kind 0 RGBA with width/height, kind 1 encoded); budget 256 MB evicts the least recently drawn (A drawn first, B later, C inserted in a later pass → A dropped, bitmap closed); two 150 MB images touched in the same pass are both kept until the next pass; an evicted hash is fetched again on the next `request`; a decode failure is never retried; a fetch failure is retried at most twice; `dispose()` closes every bitmap, and a fetch resolving after `dispose()` closes its bitmap and fires no `ready`.
- [ ] 3.3 Add `imagePlacement.ts` and `imageCache.ts`.
- [ ] 3.4 `src/lib/types.ts`: `ImageRun` and `GridRow.images?`.
- [ ] 3.5 `terminalService.ts`: per-entry cache and `rowImages`; `applyUpdate` calls `applyRowImages`; each paint pass calls `beginPass()`; `paintRow` draws ready runs after glyphs and underline (z ≥ 0), z < 0 before glyphs; `ready(hash)` repaints the rows referencing it, then selection and cursor; dispose the cache in `destroy` and in quake shell removal.

## 4. Verification

- [ ] 4.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- [ ] 4.2 `~/.claude/skills/work/scripts/work-gates.sh mufdidev/fri-609-git-panel-agent-actions terminal-inline-images` passes.
- [ ] 4.3 Manual walk: in the main terminal, `img2sixel` (or `chafa -f sixel`) on a PNG draws the image in place; `chafa -f iterm` on a PNG draws it; `clear` removes it.
- [ ] 4.4 Manual walk: in yazi, moving across image files shows each preview and replaces the previous one; leaving yazi leaves no image behind.
- [ ] 4.5 Manual walk: after printing an image, run `seq 100` and scroll up: the image is drawn in its rows in the scrollback; resizing the window and zooming the font with an image on screen keep the app usable.
- [ ] 4.6 Manual walk: the same `img2sixel` in a quake shell draws the image; a GIF via `chafa -f iterm` shows its first frame.
