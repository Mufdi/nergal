
## Frontend review

Static review of the canvas path. Device-pixel math is consistent: the canvas is sized in device pixels and `runRects` uses `metrics.cellWidth/Height` directly.

### 🔴 Blocking

- None.

### 🟡 Important

- `src/components/terminal/imagePlacement.ts:29-30` `dw` and `dh` are not clamped. Padding at or above the cell span gives zero or negative size. A negative destination size makes `drawImage` mirror the slice instead of skipping it. Fix: skip the run when `dw <= 0 || dh <= 0` (or `sw <= 0 || sh <= 0`), and clamp u/v to 0..1.
- `src/components/terminal/terminalService.ts:1471` Image smoothing stays at the default (bilinear). Each row draws a fractional source slice (`sy = v0*imgH`, `sh = imgH/rows`) scaled to a cell. Bilinear sampling reads the neighbour slice, so seams can show between rows. Fix: floor or round the source rect to whole pixels, or set `imageSmoothingEnabled = false` for 1:1 sixel placement. Decide per D5 and cover it in walk step 4.5.
- `src/components/terminal/imageCache.ts:110-123` Byte accounting only counts decoded images. In-flight decodes add to `total` only after `decode` resolves. With 24 distinct images arriving together, every `onFetched` sees the old `total`. The 512 MB ceiling and the 256 MB eviction are both bypassed. Fix: reserve `size` in `total` when decode starts and release it on failure.
- `src/components/terminal/terminalService.ts:539,556` The cache is released only on destroy. A hidden or switched-away terminal keeps up to 256 MB of bitmaps, per session and per quake shell. Ten sessions with images can hold 2.5 GB of GPU-backed memory. Fix: trim to zero unreferenced entries on hide, or document this as accepted in design D4.
- `src/components/terminal/terminalService.ts:1358-1359` Every grid update allocates a new `ImageRun[][]`, a `flatMap` array and a `Set`, even when no image exists. This runs on the hot path of every PTY burst. Fix: short-circuit when the previous and new rows have no runs, and reuse `entry.rowImages` and the empty set.
- `src/components/terminal/terminalService.ts:1467` `images.get()` starts a fetch from inside the paint path. A `failed` entry refetches on each repaint, up to the retry limit, so one scroll burst can burn the retries. Fix: count a retry per grid update, or delay retries, so rapid repaints do not exhaust them.

### 🟢 Nits

- `src/components/terminal/imageCache.ts:156` `setVisible` scans all entries on every grid update when `total < IMAGE_CEILING_BYTES`, which is nearly always. Track a `blockedCount` and skip the scan at zero.
- `src/components/terminal/terminalService.ts:431` Cast `header.body.buffer as ArrayBuffer` and `as BlobPart` hide the `ArrayBufferLike` type. Narrow with a typed helper in `imageCache.ts`.
- `src/components/terminal/terminalService.ts:1475-1485` `repaintImageRows` scans `rowImages` per row with `some` for each ready image. With 24 rows streaming this is 24 scans per `onReady`; cheap, but a hash-to-rows index would remove it.
- `src/components/terminal/terminalService.ts:1459` The cursor cell draws opaque over an image. This matches text cursors, so confirm it is intended on image rows.

Checked and fine: `close()` on eviction and dispose; late decode after dispose closes its bitmap; row repaint clears the stale image through the full-row background fill; draw order matches D5 (glyphs and underline, then `z >= 0`, then selection, then cursor); `ImageRun` and `GridRow.images` types have no `any`; comments are WHY-only.

Verdict: PASS (0 blocking, 6 important, 4 nits)

## Adversarial review

reviewer: adversarial · reviewed_at: 2026-11-24 · scope: `git diff main -- src src-tauri/src` plus the untracked image modules and tests, against the change artifacts.

### Diff scope

- Files changed: 18 (9 tracked, 9 new) · LOC: about +1540 / -9.
- `docs/shortcuts.md` also differs in the working tree; it is outside this change (see Nit 5).

### Pass 1 — Bugs

- [Important] `src-tauri/src/terminal/image_fetch.rs:110` and `src/components/terminal/imageCache.ts:110` — Encoded images are counted as 0 bytes. The backend writes width and height 0 for kind 1. The cache computes `size` from that header, so every iTerm2 PNG, JPEG or GIF adds 0 to `total`. The 256 MB budget and the 512 MB ceiling never apply to them. A yazi session or `chafa -f iterm` on many PNGs grows without bound. This breaks spec.md:69 ("counted as width × height × 4 bytes per bitmap"). The plan caused it: implementation.md:24 specifies "kind 1, width/height 0". → Send the real dimensions from `ImageDataType::dimensions()` (termwiz `image.rs:344`). Also account the decoded `bitmap.width * bitmap.height * 4` on insert. Add a cache test with a kind 1 header.
- [Important] `src-tauri/src/terminal/session.rs:534-536` and `session.rs:602` — Snapshot cost scales with image cells. Each image cell formats a 64-char hex string with 32 `format!` calls. Each image cell also inserts into the registry HashMap. This runs on every emitter tick, at up to 120 Hz, under the session mutex. A standalone `-O` microbenchmark of that per-cell work measured 2.8 ms for 1,920 cells. It measured 25.7 ms for 17,500 cells, a maximized terminal covered by an image. The PTY reader waits on that mutex, so output stalls while an image is on screen. This breaks design goal "a large image does not block the paint loop". → Memoize the hex and the registry insert per distinct `[u8; 32]` within one snapshot. A small local map or a last-hash check suffices. Compare runs by the raw hash, not the String.

### Pass 2 — Security

- [Important] `src/components/terminal/imageCache.ts:112-124` — The 512 MB ceiling ignores in-flight decodes. The check reads `total`, which grows only after a decode resolves. One paint pass starts a fetch for every new visible hash. Each `onFetched` then sees the same old `total` and starts its decode. Hostile output can show many distinct images at once, for example 24 one-row sixels near wezterm's 100 MB cap. Sixel run-length encoding makes that input a few KB. The renderer then decodes gigabytes, and the backend copies each image in parallel `spawn_blocking` tasks. This breaks spec.md:69 ("SHALL NOT decode a new image" above the ceiling). The frontend review lists the same root cause; this pass confirms it independently. → Reserve `size` in a pending counter before `decode`, and release it on failure. Check `total + pending + size` against the ceiling. Consider capping concurrent fetches per terminal.
- No finding on the command surface. `terminal_image` accepts any session id, like `terminal_attach` and `terminal_input` already do. It serves only hashes a snapshot of that terminal registered. Lock order is safe: global maps are dropped before the session lock, and no guard crosses the await.

### Pass 3 — Compliance (vs specs/**, design.md, implementation.md)

- [Important] spec.md:4, "cells merge into a run only when their left and top padding match" — The path exists at `session.rs:612-613`. No test exercises non-zero padding. Every Rust fixture is sixel or iTerm2, which always have zero padding. → Add a unit test of `push_image_cell` with hand-built `ImageCell` slices that differ in `pad_left`.
- Acceptance criteria:
  - [x] Sixel image becomes placements — `image_runs_tests.rs::sixel_becomes_one_run_per_covered_row`.
  - [x] iTerm2 image becomes placements — `image_runs_tests.rs::iterm2_png_becomes_runs_with_the_png_hash`.
  - [x] Overwriting the cells removes the placement — `clear_screen_removes_*`, `text_over_every_covered_cell_*`.
  - [x] Image rows resend when only the image changes — `differ_image_tests.rs::row_whose_runs_change_with_equal_cells_is_resent_alone`.
  - [x] Fetch a sixel image / an iTerm2 PNG / unsupported format / unknown hash — `image_fetch_tests.rs`.
  - [x] Image appears after decode — cache `ready` test plus walk 4.3.
  - [x] One fetch per image per terminal — `imageCache.test.ts` "fetches once per hash".
  - [x] Images scroll with their lines — walk 4.5.
  - [x] Budget evicts the least recently drawn image — `imageCache.test.ts` budget eviction.
  - [.] Referenced images are never evicted — the test passes, but only for kind 0; kind 1 accounting is broken (Pass 1).
  - [x] Evicted image is fetched again — `imageCache.test.ts` "fetches an evicted hash again".

### Nits

- [Nit] `src/components/terminal/terminalService.ts:435` — `new ImageData` throws synchronously on a length mismatch or a zero dimension. The throw escapes `opts.decode(header).then`, so the entry stays `loading` forever. It also surfaces as an unhandled rejection. → Make the decode callback `async`, or wrap the call in `Promise.resolve().then`.
- [Nit] `src/components/terminal/imageCache.ts:156` — A `blocked` or `failed` entry retries only when its row repaints. When the visible set shrinks, nothing repaints the blocked row. A static row keeps showing text after the cache has room. → Have `setVisible` report cleared hashes, and repaint their rows.
- [Nit] WHAT sentences in new doc comments: `pty.rs:2458` ("Raw bytes of an inline image…"), `image_fetch.rs:33` ("Maps image hashes…"), `session.rs:593` ("Extends the open run…"), `types.rs:86` ("A horizontal strip…"). The WHY halves can stay. → Drop the first sentence of each.
- [Nit] Doc drift: `implementation.md:24` gives `image_payload(&mut TerminalSession or registry) -> Result<Vec<u8>, String>`. The code is `image_fetch.rs:83`, taking `&TerminalSession` and returning `ImageFetchError`. `design.md:81` says `z < 0` runs draw "before the glyphs". The per-cell background fill then covers them (`terminalService.ts:1419`). Evidence records the second point, but design.md does not. → Align both documents with the code.
- [Nit] `docs/shortcuts.md:139` — An unrelated vault-path edit sits in the working tree. → Keep it out of the FRI-611 commit.

### Harness entropy check

- Speculative abstractions: no. `z` and multi-run cells are justified by FRI-613 in design D1.
- WHAT comments: four, listed in Nits.
- New dependencies: none.

Verdict: FAIL — three Important bugs and one Important compliance gap. Kind 1 images escape the budget entirely. The ceiling ignores concurrent decodes. Per-cell hex formatting stalls the session mutex while an image is on screen. All three ship a defect a user or hostile output can hit.

## Re-review (fix delta)

reviewer: adversarial · reviewed_at: 2026-11-24 · scope: `git diff 71ee2b9 f2db630 -- src src-tauri/src`. Ran `vitest` on imageCache and imagePlacement (36 pass) and `cargo test --lib image` (34 pass).

### Prior findings, fixed

- FE Important 1, clamp and skip: `imagePlacement.ts:24-32`; `terminalService.ts:1495` skips a null rect.
- FE Important 2, smoothing: `terminalService.ts:1489` and `:1499`; recorded in design D5.
- FE Important 3 and Adversarial Pass 2, in-flight bytes: reserved at `imageCache.ts:153-155`, ceiling at `:107-110`.
- Reservation release on failure: `imageCache.ts:158-163`, covering both sync throw and rejection.
- FE Important 4, hidden terminals: `terminalService.ts:259-263` calls `releaseAll` (`imageCache.ts:216-226`); show repaints at `:345-349`.
- FE Important 5, hot path: `terminalService.ts:1365-1370` skips all image work when no rows hold runs.
- FE Important 6, paint-path retries: `get` no longer fetches non-empty entries (`imageCache.ts:191`); retries live in `setVisible`.
- FE nits: `retryable` counter at `imageCache.ts:64,79-88,201`; typed helpers at `:47-54`; hash index at `terminalService.ts:1474-1511`.
- FE nit, cursor over images: D5 (`design.md:81`) states the cursor draws last, so it is intended.
- Adversarial Pass 1, kind 1 size: `image_fetch.rs:106-113` sends `dimensions()`; test at `image_fetch_tests.rs:102`.
- Adversarial Pass 1, snapshot cost: per-snapshot pointer map at `session.rs:528,537-541`.
- Adversarial Pass 3, padding: `image_runs_tests.rs:191-217` covers left, top and equal padding.
- Adversarial nits: sync throw at `imageCache.ts:165-171`; unblocked rows repaint through `onReady` (`:209`, `:179`).
- Adversarial nits: WHAT first sentences dropped at `pty.rs:2458`, `image_fetch.rs:33`, `session.rs:603`, `types.rs:86`.

### Important

- None.

### Nits

- [Nit] `QuakeTerminal.tsx:66-77` — Closing quake only hides the overlay; the shell container stays `flex`. It keeps its bitmaps and keeps fetching while closed. This contradicts D4 "a hidden terminal releases its bitmaps". Bounded to one shell. → Record the exception in D4, or release on close and repaint on reopen (`:100-104`).
- [Nit] `terminalService.ts:522` — A terminal created while another became active hides itself without `releaseAll`. Its first `paintAll` (`:511`) already started fetches. `hideEntry` then early-returns (`:260`). → Call `hideEntry`-style release there.
- [Nit] `imageCache.ts:204-212` — `decoding` stays 0 during the loop, so every blocked hash refetches at once. Only one decodes; the rest re-block, each after a full payload copy. Also, a blocked hash waits for the next grid update after a decode settles. → Unblock one hash per pass when `decoding === 0`.
- [Nit] `terminalService.ts:1489` — Smoothing is off for every image, including downscaled iTerm2 photos. Nearest-neighbour downscaling aliases visibly. → Check a large photo in walk 4.3, or enable smoothing when the scale is below 1.
- [Nit] `implementation.md:39` — The cache interface omits `releaseAll` and the `string[]` return of `setVisible`. → Align the interface line with `imageCache.ts:28-35`.

Checked and fine:

- Counters: `put`/`drop` keep `retryable` exact; `total` never goes negative.
- Eviction touches only `ready` entries, so reservations stay.
- `releaseAll` bumps `epoch`; late fetches are ignored and late bitmaps are closed.
- No entry stays `loading` or `decoding`.
- Hidden paints skip `get` (`terminalService.ts:1487`); after `releaseAll`, `setVisible` has nothing to retry.
- The short-circuit cannot skip clearing: `rowsByHash` is rebuilt from `rowImages`, so a removed last run still runs `applyRowImages`.
- Smoothing is restored after the loop, and no `drawImage` can throw on a closed bitmap.
- Pointer keys: `lines_in_phys_range` returns owned lines, so every `Arc` outlives the snapshot. No address is reused.
- The `#[cfg(test)]` wrapper derives the same hex as production, so the two cannot diverge.
- `dimensions()` reads only the header. A failure maps to `UnsupportedFormat`, retried twice like any fetch error.
- iTerm2 input is capped by wezterm's `check_image_dimensions`, so the lone-decode overshoot stays one image.

Verdict: PASS


## Resolution (orchestrator, 2026-11-24)

- Frontend review: all 6 Important and 4 nits fixed in group 5 (`f2db630`), confirmed by the re-review.
- Adversarial review: the 3 Important bugs (kind 1 sizes, concurrent decodes past the ceiling, per-cell hex formatting), the compliance gap (padding merge tests) and the 5 nits fixed in group 5, confirmed by the re-review. `docs/shortcuts.md` stays out of every commit.
- Re-review nits 1–5 fixed in group 6: quake overlay region visibility, terminals created hidden, unblocking only what fits plus a re-check after each decode, smoothing by scale with snapped source rects, and the interface list in implementation.md.
