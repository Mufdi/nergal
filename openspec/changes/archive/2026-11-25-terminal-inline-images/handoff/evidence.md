# Evidence: terminal-inline-images (FRI-611)

## Baseline

Base `86d90f2` (main after FRI-609, 2026-11-24; same code as the FRI-609 final check): clippy clean; `cargo test` 1314 passed, 1 ignored; tsc clean; `pnpm test` 56 files, 652 tests. No pre-existing failures.

## Plan review

iprev with an independent opus evaluator, 3 rounds (`handoff/iprev-r1.md` … `iprev-r3.md`):
- Round 1: 14 findings (2 high: the fetch command on the main thread holding the global PTY maps; a decode-failure fetch loop at emit rate). All applied.
- Round 2: 5 findings (1 medium blocking: pass-based eviction does not match partial repaints; quake fetch key). All applied.
- Round 3: APPROVED with 5 items; items 1–3 (stale "pass" wording, scenario precondition, hard 512 MB ceiling) required before the build, all 5 applied.

## Build (groups 1–3)

Red tests by a separate sonnet test-writer; builders: sonnet backend (groups 1–2) and sonnet frontend (group 3) in parallel, on disjoint files.

Red conflicts and rulings:
- `imageCache.test.ts` used `Array.prototype.at` (TS2550 with lib ES2021). Ruling: the test was wrong, not the project config. The test-writer replaced it with index access, typed the bitmap fake's `close` as `Mock<() => void>`, and re-locked.
- `image_runs_tests.rs` index loops failed `clippy::needless_range_loop`; the backend builder had silenced it with an `allow` on the `mod` line. Ruling: test-writer rewrote the loops with iterators and re-locked; the `allow` is removed.

Builder deviations: `GridSnapshot`/`GridUpdate`/`GridRow` lost `Eq` (f32 fields); `ImageRegistry::prune()` runs once per snapshot, so a dropped image returns `UnknownHash` (tests accept it); run merging compares f32 slice edges exactly (wezterm builds adjacent slices from the same `xpos`); z < 0 runs paint before the glyph loop, so per-cell backgrounds cover them (nothing emits z < 0 until FRI-613). Orchestrator fix: `CachedBitmap.close` typed `() => void` instead of `Function`.

Gate 6b WARN: one line, `image_runs_tests.rs:76` `.iter().enumerate().skip(3)` — an iterator adaptor, not a skipped test. No suppression added.

Verification (2026-11-24): clippy clean; `cargo test` 1339 passed, 1 ignored; rustfmt check clean; tsc clean; `pnpm test` 58 files, 676 tests; work-gates 6a PASS, 6b WARN (justified above), 6c PASS, Red locks PASS (5 files).

## Review fixes (groups 5 and 6)

Reviewers: adversarial (opus) FAIL with 3 Important bugs + 1 compliance gap + 5 nits; frontend (sonnet) PASS with 6 Important + 4 nits. All fixed in group 5 (`f2db630`). Focused opus re-review of the fix delta: PASS, 0 Important, 5 nits, all fixed in group 6.

Red conflicts and rulings:
- Group 5: the locked ceiling tests disagreed under a plain `total + size > 512 MB` rule. Ruling: the rule is "no decode starts at ≥ 512 MB held; with a decode in flight, none starts that would pass 512 MB"; a lone decode may cross by at most one image. Recorded in design D4 and the spec.
- Group 6: the two-fit case of "re-requests as many blocked hashes as fit" could not pass (the unreferenced image is excluded from `held`, so both cases had the same `held`). Ruling: the test was wrong; the test-writer varied a referenced size instead and re-locked.

Builder deviations: a terminal created in the background skips image draws through the region's active-id check instead of releasing after its first paint; `setRegionVisible(region, visible)` added for the quake overlay, which hides with `visibility`.

Gate 6b WARN (unchanged): `image_runs_tests.rs` `.iter().enumerate().skip(3)` is an iterator adaptor, not a skipped test.

Verification (2026-11-24): clippy clean; `cargo test` 1343 passed, 1 ignored; tsc clean; `pnpm test` 58 files, 699 tests; work-gates 6a/6c PASS, 6b WARN (justified), Red locks PASS (5 files).

## Walk 1 fixes (group 7, 2026-11-25)

Walk 1 on the installed deb: F1-F8 all failed, no image drawn for sixel or iTerm2. A temporary probe test (removed) fed real `chafa -f sixels` and `chafa -f iterm` output through `TerminalSession`, the differ and `payload_for`. Sixel: runs on every covered row, kind 0 payload. iTerm2: chafa sends TIFF, `payload_for` returned `UnsupportedFormat`. The frontend failure came from the CSP: `connect-src 'self'` blocks Tauri's IPC protocol, so the raw response reached `imageCache` as a JSON number array. User decisions: add the IPC sources to the CSP; decode other encoded formats in Rust (design Revision 2, D7/D8).

Red (sonnet test-writer): `csp_tests.rs` (new) and `image_fetch_tests.rs` (TIFF and BMP decode to kind 0, truncated TIFF and over-limit TIFF stay `UnsupportedFormat`). 4 new tests failed for the intended reason, and 2 guards passed. Re-locked.

Builder (sonnet): `tauri.conf.json` connect-src in csp and devCsp; `image = "0.24"` (Cargo.lock adds only the reference to the existing 0.24.9); `payload_for` copies the bytes out of the guard, pre-checks dimensions, decodes under `Limits.max_alloc` 100,000,000 and serves kind 0.

Verification (2026-11-25): clippy `--all-targets -D warnings` clean; `cargo test` 1348 passed, 1 ignored; rustfmt check clean on changed files; tsc clean; `pnpm test` 699 passed; work-gates 6a/6c PASS, Red locks PASS (6 files), 5 WARN (Cargo manifests changed; `image 0.24.9` was already in the lock through wezterm-term, so no new package), 6b WARN (unchanged iterator `.skip(3)`, justified earlier).

## Review of the walk fixes (group 8, 2026-11-25)

Reviewer (opus, adversarial + security, `handoff/REVIEW-walk1.md`): FAIL, 2 Important (unbounded concurrent decodes; no JPEG/GIF kind 1 tests) + 11 Nit. D7 root cause confirmed. Finding 13 (`docs/shortcuts.md`) not acted on: an unrelated user edit that is never committed. The rest is answered by design/implementation Revision 3 and group 8.

Red (sonnet test-writer): Rust JPEG/GIF kind 1, PNM refused, over-limit TIFF at a 108 MB decoder buffer (renamed), `payload_for_bounded` waits for a slot; TS queue of 2 in flight, unsupported format not retried, non-ArrayBuffer dead with one warning, `releaseAll` drops the queue. 7 TS tests failed for the intended reason; Rust failed to compile only on the missing `decode_slots`/`payload_for_bounded`. Test-writer added `flush()` calls to the `blockedBehindDecode` helper so the cap does not queue the existing byte-reservation fetches. Re-locked.

Builder (sonnet): `decode_slots()` (2 permits) + `payload_for_bounded`, used by `terminal_image`; runtime format allowlist Tiff/Bmp/WebP; pre-check removed; `imageCache.ts` queue with `MAX_IN_FLIGHT_FETCHES = 2`, unsupported-format and non-ArrayBuffer permanent, one console warning. Orchestrator fix: the `DECODE_PERMITS` comment named panics; it now gives the memory reason.

Verification (2026-11-25): clippy clean; `cargo test` 1352 + 1 + 1 passed, 1 ignored; rustfmt check clean; tsc clean; `pnpm test` 706 passed; work-gates 6a/6c PASS, Red locks PASS (6 files), 5 WARN (justified above), 6b WARN (unchanged iterator `.skip(3)`).

## Second review of the walk fixes (group 9, 2026-11-25)

Re-review (opus, `handoff/REVIEW-walk1-r2.md`): FAIL, 1 Important (`releaseAll` reset the in-flight count while old requests still ran, so hide/show stacked orphan decodes on the global semaphore) + 9 Nit. Prior findings: 9 resolved, 3 partial, 13 not acted on by decision. Answered by design/implementation Revision 4 and group 9. Nit "renumber Verification": kept as group 4, because the config rule requires Verification last and walk ids 4.x are cited across the docs.

Red (sonnet test-writer): 4 TS tests (count across `releaseAll`, queue drops non-visible, one restart of two blocked hashes, one warning per cache) + Rust WebP guard. Conflict and ruling: the Builder reported the restart test's sizes could not block both hashes. Ruling: the test was wrong; the test-writer rebuilt it (x decode rejects and frees 200 MB) and re-locked. Orchestrator mutation check: with `restarted` removed from `overCeiling` the test fails; restored byte-identical (`cmp`).

Builder (sonnet): `inFlight` decremented on every settle, kept across `releaseAll`; `drainQueue` drops non-visible queued hashes; `requestRetries` counts its restarts; per-cache warn flag; permit inside the blocking closure; format guessed before the clone. Deviation: `visibilityKnown` (see implementation.md "Code after group 9").

Verification (2026-11-25): clippy clean; `cargo test` 1353 + 1 + 1 passed, 1 ignored; rustfmt check clean; tsc clean; `pnpm test` 710 passed; work-gates 6a/6c PASS, Red locks PASS (6 files), 5 WARN and 6b WARN (justified above). Comment audit: three added comments, all WHY.

## Third review (group 10, 2026-11-25)

Re-review (opus, `handoff/REVIEW-walk1-r3.md`): PASS, 0 Important, 4 Nit. Nits 2-4 (walk rows 4.7/4.8 pass conditions, task 9.1 sizes) fixed in tasks.md. Nit 1: Red test by the sonnet test-writer (old-epoch bytes reused for a queued hash; failed with 0 decodes), then a one-line orchestrator fix at `imageCache.ts:184` (XS, main thread). Verification: `pnpm test` 711 passed; tsc clean; Red locks PASS (6 files). Rust untouched since group 9 (clippy clean, cargo test 1355 passed).

## Walk 2 (group 11, 2026-11-25)

Walk 2 on deb 19:51: F1-F8 and G1 PASS; G2 and G3 (`burst.py`, 5000x5000 deflate TIFFs at native size) drew nothing. Probe (temporary, removed): wezterm places them; `payload_for` returned `UnsupportedFormat` in under 1 ms; the same bytes decode with a larger `max_alloc`. Cause: image 0.24's `TiffDecoder::set_limits` splits `max_alloc` between the output and the strip buffer (`image-0.24.9/src/codecs/tiff.rs:210-228`). Fix (design Revision 6, orchestrator, S): per-format decoder, header check `w*h*4 <= 100,000,000`, working limit 300,000,000. Red by the sonnet test-writer (`large_single_strip_tiff_at_the_pixel_cap_decodes_to_full_rgba`, failed with `UnsupportedFormat`; about 13 s in debug, mostly fixture building). Release probe on the real `burst.py` output: kind 0, 100,000,009 bytes, 0.18-0.21 s per image.

Verification (2026-11-25): clippy clean; `cargo test` 1354 + 1 + 1 passed, 1 ignored; rustfmt check clean; Red locks PASS (6 files); work-gates 6a/6c PASS, 5/6b WARN (justified above). Frontend untouched since group 10 (`pnpm test` 711).

## Walk 3 (2026-11-25)

Deb 20:58. User walk: F1-F8 and G1 PASS (walk 2); G2 PASS (about 5 of 30 blocks drawn by design; with blocks on screen the app process was 216,772 KB and WebKitWebProcess 614,920 KB, both under 1 GB); G3 PASS (images back in about 3 s after 10 quake toggles; accepted for 100 MB images). Note: `pgrep -o -x nergal` picked the `nergal mcp` server of the session; measured by `ps -eo pid,rss,etime,args | grep '[n]ergal'`.
