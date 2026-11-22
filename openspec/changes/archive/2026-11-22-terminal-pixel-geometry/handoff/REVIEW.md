---
type: review
feature: terminal-pixel-geometry
reviewer: adversarial
reviewed_at: 2026-11-22T19:59:14-03:00
verdict: FAIL
---

# Review — terminal-pixel-geometry

> Rules: review only the diff and the artifacts named below. Skip anything a gate or CI already enforces (types, lint, formatting, test pass/fail). Tag every finding with its pass and its severity. **Important** = would ship a defect, a vulnerability, or a divergence from spec/plan. **Nit** = everything else; **max 5 nits**, drop the rest.

## Diff scope

- Files changed: 11 (5 modified, 6 new) · LOC: +362 / -78 in modified files, +577 in new files

## Pass 1 — Bugs

- [Important] `src-tauri/src/lib.rs:350` and `:789-792` — The log lock is released right after `setup` returns. The `move` setup closure captures `log_guard`, so `run()` no longer owns it. Tauri stores `setup` as a `Box<dyn FnOnce>` and calls it once (`tauri-2.11.1/src/app.rs:2522-2523`). The closure then drops its captures, the Arc count reaches 0, and the `File` closes. On the retry path the retry thread holds the only other Arc. It drops that Arc at thread exit, right after `*slot = Some(lock)` (`lib.rs:181`). I confirmed the drop order with a standalone `rustc` program that copies this shape. Failure scenario: Nergal runs for a minute, then the user clicks the launcher again. The second process wins `try_lock_log` and renames the live log to `nergal.log.1`. It then truncates a new `nergal.log`, writes a few lines and hands off. The live instance keeps writing into `nergal.log.1`. Diagnostics read `nergal.log` and show only the second launch's lines. A crash after that loses its output on the next start, because that start renames the stub log over `.1`. This breaks "Only one process owns the log" and its clause "SHALL hold the lock for its whole life". It also defeats D3. Task 4.4 would catch it by hand, but no automated test covers the guard lifetime. → Fix: move only a clone into the closure. Write `let retry_guard = Arc::clone(&log_guard);` before the builder, use `retry_guard` inside `setup`, and keep `log_guard` alive in `run()`, whose `.run()` never returns. Alternatively, call `app.manage(log_guard)` in `setup`.
- [Important] `src/components/terminal/terminalService.ts:1192-1212` with `:1228` — `lastSent` records the order in which calls resolve, not the size the backend ends at. `fit` fires `sendResize` without awaiting it, so several calls can be in flight at once. Failure scenario during a drag that goes back and forth: fit A sends 100 cols and fit B sends 90 cols. A resolves and sets `lastSent` to 100. Fit C computes 100 again, `nextResize` matches `lastSent`, and C sends nothing. Then B resolves and sets `lastSent` to 90. The backend stays at 90 while the container fits 100. `applyUpdate` (`:1275-1280`) then shrinks the canvas to the backend's 90 cols. Its comment says it "relies on the next fit() cycle", but no fit comes until the next container resize. The old code compared against `entry.cols`, which `fit` sets synchronously, so the last fit always sent. → Fix: gate on the last requested geometry, set before the invoke. On rejection, reset it only if it still equals the rejected payload. Or tag each send with a sequence number and record only the latest one.

## Pass 2 — Security

- [Nit] `src-tauri/src/terminal/session.rs:71-77` — The clamp guards the pixel axes but not a `cols` or `rows` of 0. A `resize_session_terminal` call with `cols: 0` gives wezterm `physical_cols = 0`. `CSI 16 t` (`terminalstate/mod.rs:2034`) and `assign_image_to_cells` (`image.rs:72`) then divide by zero and abort the app. The current frontend never sends 0, because `computeCols` floors at 10 × 3. The design goal says "whatever the frontend sends", though. → Fix: reject `cols == 0 || rows == 0` in `resize_pty` and `spawn_pty`, or clamp both counts to at least 1.

## Pass 3 — Compliance (vs specs/**, design.md, implementation.md, project CLAUDE.md conventions)

- [Important] `src-tauri/src/lib.rs:350` — app-diagnostics-log, requirement "Only one process owns the log": "The owner SHALL hold the lock for its whole life". The code releases it after `setup` (see Pass 1). Scenario "A second launch leaves the running instance's log alone" passes in `log_owner_tests.rs:62-73`, but only because that test holds the guard itself. The real `run()` path is not covered.
- [Nit] `src/components/terminal/pixelGeometry.test.ts:63-79` — Scenario "The post-spawn resize uses the current size" is tautological. The test mutates a local object and passes its values to `nextResize`. It never touches `showTerminal`, which is what makes the call read `entry.cols` after the await (`terminalService.ts:453`). A regression back to the captured `cols` would still pass.
- [Nit] `src-tauri/src/pty_pixel_tests.rs` — Scenario "A resize with an unchanged size changes nothing": no test calls `resize_pty` with an unchanged size and checks that the master is untouched. Only the shared predicate is tested, on `TerminalSession` (`pixel_geometry_tests.rs:112-130`). The skip at `pty.rs:605` has no test of its own.
- Acceptance criteria:
  - [x] Spawn with pixel geometry — `pty.rs:464`, `:497-503`; `pixel_geometry_tests.rs:94`, `pty_pixel_tests.rs:82`
  - [x] Cell size report follows the geometry — `pixel_geometry_tests.rs:100`
  - [x] Resize carries the new geometry — `pty.rs:608-612`; `pty_pixel_tests.rs:95`
  - [x] A resize with an unchanged size changes nothing — `pty.rs:605`; `pixel_geometry_tests.rs:113` (emulator half only, see Nit)
  - [x] A cell size change resends the geometry and resizes the canvas — `terminalService.ts:1214-1229`; `pixelGeometry.test.ts:26`
  - [x] An unchanged geometry sends nothing — `pixelGeometry.ts:37-45`; `pixelGeometry.test.ts:44`
  - [x] A failed resize is sent again — `terminalService.ts:1201-1211`; `pixelGeometry.test.ts:50`
  - [x] The post-spawn resize uses the current size — `terminalService.ts:453`; `pixelGeometry.test.ts:63` (weak, see Nit)
  - [x] Re-attaching to a live PTY applies the current geometry — `terminalService.ts:453`; `pixelGeometry.test.ts:81`
  - [x] Missing geometry, below one pixel per cell, huge column count — `session.rs:65-83`; `pixel_geometry_tests.rs:63-91`
  - [x] Sixel and iTerm2 images do not panic — `sixel_zero_pixel_tests.rs:16`, `pixel_geometry_tests.rs:142`
  - [x] Restart keeps the crashed run's output, one generation, first start — `lib.rs` `rotate_log`; `log_owner_tests.rs:16-60`
  - [.] A second launch leaves the running instance's log alone — the unit test passes; the real process drops the lock (Important above)
  - [x] The startup attempt never waits, relaunched primary waits, retry gives up — `log_owner_tests.rs:76-131`
  - [x] Children do not inherit the lock — `log_owner_tests.rs:134`

## Harness entropy check

- Speculative abstractions? Two minor overlaps. `resize_pty` calls `size_changed` (`pty.rs:605`), then `resize_with_pixels` checks it again (`session.rs:247`) and returns a `bool` nobody reads. `TerminalSession::resize(cols, rows)` (`session.rs:234`) is now used only by a test. It skips the no-op predicate and resets the pixels to the fallback cell. A future caller would put the emulator out of step with the PTY.
- WHAT-comments? The "API assumed for the Builder" headers in the four Rust test files and `pixelGeometry.test.ts:1-7` are handoff notes for the builder. They describe the API, not a reason, so they break the WHY-only rule.
- New deps without user confirm? No.

## Verdict

FAIL — The pixel geometry half is sound. The clamp, the no-op predicate, PTY and emulator agreement, and lock ordering (instances, then session, then differ) all hold. The log ownership half does not meet its own requirement. The lock guard moves into the `FnOnce` setup closure, so it is dropped right after startup. Any later launch then rotates the live log away. The frontend `lastSent` gate also adds a resize race that the old always-send path did not have. A drag can leave the backend at a stale column count.

## Re-review (group 5)

Scope: the group 5 fixes only, on the working tree against `fe2872b`. Reviewed `lib.rs` (`LogOwner`), `pixelGeometry.ts` (`recordRequest`, `afterResizeFailure`), `terminalService.ts` (`sendResize`, `lastRequested`), `pty.rs` (`validate_grid_size`, `resize_pty`) and `session.rs`. Also read the new locked tests `resizeTracker.test.ts` and `pty_zero_size_tests.rs`.

### Resolved

- Log guard lifetime — `src-tauri/src/lib.rs:797`. `app.manage(LogOwner { .. })` puts an Arc clone in managed state. Managed state lives until the app exits. The retry thread writes its lock into the same `Mutex` slot, so the managed Arc keeps both paths alive. `LogOwner` is not cfg-gated, so it also compiles on non-Linux targets. The Important finding is fixed.
- Resize race — `src/components/terminal/terminalService.ts:1192-1209`. `lastRequested` is now set before the invoke. A later fit compares against the newest request, not the newest answer. The A=100, B=90, C=100 trace now sends C, and the backend ends at 100. On rejection, `afterResizeFailure` clears the value only when it equals the failed request. The comparison is by value, so a matching newer request can be cleared too. That costs one extra no-op send and nothing else. Once every call has settled, `lastRequested` is either the backend size or `null`. The Important finding is fixed.
- Zero grid — `src-tauri/src/pty.rs:157`. `start_claude_session` (`:723`), `spawn_aux_shell` (`:1107`), `resize_session_terminal` (`:1558`) and `resize_pty` (`:605`) all reject 0 first. Each check runs before any PTY or emulator access. The Security nit is fixed.
- `TerminalSession::resize(cols, rows)` is gone. No production or test caller remains, and the one test now uses `resize_with_pixels`.

### Remaining

- [Nit] `src-tauri/src/pty.rs:615-624` — Task 5.5 asked for one size predicate, but `resize_pty` still checks twice. It calls `size_changed`, resizes the master, then branches on the `bool` from `resize_with_pixels`. The session lock is held throughout, so that second branch can never be taken. It is dead code. If it ever were taken, it would skip `differ.invalidate()` after a real master resize. → Drop the `size_changed` call and branch once on the `bool`. If the master resize then fails, undo the emulator resize. Otherwise, keep `size_changed` and call an infallible apply.
- [Nit] `pty_zero_size_tests.rs:1-5` and `resizeTracker.test.ts:1-5` repeat the "API assumed for the Builder" WHAT-comment headers.

No new Important findings.

### Verdict (group 5)

PASS WITH FINDINGS — All three Important findings and the Security nit are fixed. Only two nits remain.
