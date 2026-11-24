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
