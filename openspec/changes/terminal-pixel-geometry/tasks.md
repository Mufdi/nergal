## 1. Backend pixel geometry

- [x] 1.1 Test (Red): requirement "The emulator never gets less than one pixel per cell" — scenarios "Missing geometry uses the fallback cell", "Geometry below one pixel per cell uses the fallback cell", "A huge column count saturates", "Sixel image on a terminal created without pixel geometry" and "iTerm2 image with less than one pixel per cell" (the sixel test reproduces FRI-597 at runtime with `attempt to divide by zero`; the iTerm2 test fails to compile before the fix because no pixel arguments exist yet). A 0 × 0 iTerm2 image may be added as a non-Red regression test (wezterm drops it today)
- [x] 1.2 Test (Red): requirement "Pixel geometry reaches the emulator and the PTY", backend scenarios — "Spawn with pixel geometry", "Cell size report follows the geometry", "Resize carries the new geometry" (emulator answers via a captured writer; `ws_xpixel`/`ws_ypixel` via `master.get_size()` on a real PTY inserted into `PtyManager::new()`, then `resize_pty`), and "A resize with an unchanged size changes nothing" (the pure size-changed predicate, plus a scroll region set with `CSI 5;10 r` that survives the no-op resize)
- [x] 1.3 Add the fallback cell (8 × 16) and the saturating clamp function in `src-tauri/src/terminal/session.rs`; use it in `TerminalSession::with_config` and `resize` with optional pixel arguments
- [x] 1.4 In `src-tauri/src/pty.rs`, build `PtySize` and the emulator size from the same clamp; thread optional `pixel_width` / `pixel_height` through `start_claude_session`, `spawn_aux_shell`, `resize_session_terminal`, `spawn_pty`, `resize_pty` and `build_terminal_session`
- [x] 1.5 `TerminalSession` stores its clamped pixel size; `resize_pty` skips the master resize, the emulator resize and `differ.invalidate()` when the predicate says the size is unchanged

## 2. Frontend pixel geometry

- [x] 2.1 Test (Red): frontend scenarios "A cell size change resends the geometry and resizes the canvas", "An unchanged geometry sends nothing", "A failed resize is sent again", "The post-spawn resize uses the current size", "Re-attaching to a live PTY applies the current geometry", against pure functions in `src/components/terminal/pixelGeometry.ts` (geometry, `u16` clamp, and the send-or-skip decision with payload keys `sessionId`, `cols`, `rows`, `pixelWidth`, `pixelHeight`)
- [x] 2.2 Add `src/components/terminal/pixelGeometry.ts` with the geometry function and the decision function
- [x] 2.3 In `terminalService.ts`: both spawn callbacks send `pixelWidth` / `pixelHeight`; after every spawn, both regions call `resize_session_terminal` with the geometry read from the entry after the await (replaces the quake-only call and its comment); the entry records the last sent geometry only when the invoke resolves
- [x] 2.4 In `fit`: skip a container with no layout (hidden, 0 × 0 rect) instead of falling back to 80 × 24; otherwise always sync `entry.cols`, `entry.rows`, grid and canvas (`sizeCanvas` backing size from `pixelGeometry`) to the computed values; the decision function gates only the IPC call

## 3. Log rotation and ownership

- [x] 3.1 Test (Red): scenarios "Restart after a crash keeps the crashed run's output", "Only one previous generation is kept", "First start without a previous log", "A second launch leaves the running instance's log alone", "The startup attempt never waits", "A relaunched primary waits for the exiting parent", "The retry gives up after 3 seconds", "Children do not inherit the lock" (against the extracted functions on a `tempfile` dir; never `dup2`, never the real cache dir)
- [x] 3.2 Extract `try_lock_log`, `retry_lock_log` and `rotate_log` in `src-tauri/src/lib.rs` (Linux-gated, non-Linux stubs empty); the lock file is a `std::fs::File`, returned as a guard
- [x] 3.3 `run()`: one non-blocking attempt at start; on success rotate, open with `truncate(true)` and `dup2`, and keep the guard for the process lifetime
- [x] 3.4 `setup`: when the startup attempt failed, retry on a background thread for up to 3 seconds, then rotate, open and `dup2`; update the `dup2` SAFETY comment

## 5. Adversarial review fixes

- [x] 5.1 Test (Red): scenarios "Answers that resolve out of order never leave a stale size", "A failure does not clear a newer request" (pure functions in `pixelGeometry.ts` for the request bookkeeping), "Zero columns or rows are rejected" (`resize_pty` returns an error and the master size is unchanged; spawn helpers reject 0 too)
- [x] 5.2 Keep the log lock guard for the process lifetime: the guard must not live only in the `setup` closure (Tauri calls `setup` once as `FnOnce` and drops it on return); hold it in Tauri managed state or a process-lifetime owner, for both the startup and the retry paths
- [x] 5.3 `sendResize` records the requested geometry when it sends, and on failure clears it only when it is still the latest request
- [x] 5.4 Reject 0 columns or 0 rows in `resize_session_terminal`, `start_claude_session` and `spawn_aux_shell` before touching the PTY or the emulator
- [x] 5.5 Remove the duplicate size predicate in `resize_pty` (use the `bool` from `resize_with_pixels`, or drop the `bool`), and remove `TerminalSession::resize(cols, rows)` if nothing in production calls it (it resets pixels to the fallback cell)

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit && pnpm test` green (format touched Rust files with `rustfmt --edition 2024 <files>`, never bare `cargo fmt`)
- [x] 4.2 Manual (user, installed `.deb`): open yazi in the quake and move over several images; Nergal stays open
- [x] 4.3 Manual (user): inside a quake shell, `printf '\e[14t'; cat -v` shows the canvas size in pixels, not `0;0`
- [x] 4.4 Manual (user): with Nergal open, launch it again from the launcher; the window focuses at once and `~/.cache/nergal/nergal.log` keeps the first instance's lines. Quit and relaunch; `nergal.log.1` holds the previous run
