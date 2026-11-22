## Context

FRI-597: yazi previewed an image in the quake and Nergal closed. Reproduced in a unit test: feeding a 64 × 48 sixel (generated with ImageMagick) into a `TerminalSession` panics with `attempt to divide by zero` at `wezterm-term/src/terminalstate/image.rs:103:21` (tag `20240203-110809-5046fc22`).

The cause is in Nergal, not in wezterm-term. `TerminalSession` passes `pixel_width: 0` and `pixel_height: 0` to `Terminal::new` and `Terminal::resize` (`src-tauri/src/terminal/session.rs:104-105`, `:179-180`). `assign_image_to_cells` computes `cell_pixel_width = pixel_width / cols` (`image.rs:72`), which is 0, and then divides by it (`image.rs:103`). The release profile uses `panic = "abort"` (`src-tauri/Cargo.toml:167`), so the panic ends the process. The panic message went to stderr, which is `nergal.log`, but the relaunch truncated it (`src-tauri/src/lib.rs:71-75`).

The DA1 answer advertises sixel (`terminalstate/mod.rs:1283`), so image-capable apps choose sixel. FRI-600 will draw the images, so sixel stays advertised.

## Goals / Non-Goals

**Goals:**
- The pixel geometry Nergal gives wezterm-term can never cause a division by zero while it places an inline image, whatever the frontend sends.
- Apps that ask for the pixel size get the real canvas geometry, so FRI-600 can size images correctly.
- A future crash leaves its output on disk after the relaunch.

**Non-Goals:**
- Drawing images on the canvas (FRI-600).
- Changing `panic = "abort"` or catching panics around the emulator. Other wezterm-term panics stay possible; this change removes the known one.
- Enabling the kitty graphics protocol.
- Reacting to a `devicePixelRatio` change (moving the window to a monitor with another scale). It predates this change: metrics, canvas and the reported geometry all stay at the old ratio until the next restyle.

## Decisions

### D1. Send the real pixel geometry, and clamp in the backend

The frontend sends `pixel_width = cols × metrics.cellWidth` and `pixel_height = rows × metrics.cellHeight` (device pixels, the canvas backing size from `sizeCanvas`). `TerminalSession` clamps every geometry it hands to wezterm-term: an axis below one pixel per cell uses a fallback cell of 8 × 16.

Alternatives:
- **Clamp only, with a fixed fallback cell.** Removes the crash with a backend-only diff. Apps would get a made-up cell size, and FRI-600 would draw images at the wrong scale. Rejected as the only fix; kept as the safety floor.
- **Stop advertising sixel.** yazi would fall back to text previews and never send an image. An iTerm2 image (OSC 1337) would still crash when the geometry is nonzero but below one pixel per cell: wezterm drops it at exactly 0 × 0 (`iterm.rs:42-48`) but not at, say, 10 × 10 on 80 × 24. It also contradicts the user's decision to draw images (FRI-600). Rejected.
- **Catch the panic (`catch_unwind`, or `panic = "unwind"`).** Hides the cause and changes the release profile for the whole app. Rejected for this change.

Device pixels, not CSS pixels: the canvas backing store is `cols × cellWidth` device pixels (`terminalService.ts:1176`), and FRI-600 will draw in that space. Real terminals (kitty, wezterm) also report device pixels.

The clamp computes in `usize` and saturates at 65 535 for both `PtySize` (`u16`) and `TerminalSize`, so an absurd column count from IPC cannot overflow. One pure function builds the size for both the PTY and the emulator; `spawn_pty` and `resize_pty` call it.

An unchanged size is a full no-op in `resize_pty`: no master resize and no emulator resize. `TerminalSession` stores its clamped pixel size, and a pure predicate compares it with the request. The emulator half matters because wezterm's `TerminalState::resize` always resets the scroll margins and repositions the cursor (`term/src/terminalstate/mod.rs:878-883`); a needless resize on re-attach would break the scroll region of a running TUI that gets no `SIGWINCH` to redraw. So the frontend can send the geometry after every spawn or re-attach safely. A real cell-size change (font change) does send `SIGWINCH` with unchanged cols and rows; Claude Code redraws once. Accepted.

### D2. Pixel parameters are optional on the IPC commands

`start_claude_session`, `spawn_aux_shell` and `resize_session_terminal` gain `pixel_width: Option<u16>` and `pixel_height: Option<u16>`. A caller that omits them gets the fallback cell through D1. This keeps any other caller (MCP-driven spawns, tests) working without changes.

A spawn can return an existing PTY (`reserve_session_pty` at `pty.rs:697-701`, the `session_ptys` check in `spawn_aux_shell` near `pty.rs:1105`) without applying the new geometry. So the frontend calls `resize_session_terminal` with the geometry after every spawn, in both regions, and the no-op check above keeps that cheap. The frontend decides what to send through a pure function (previous sent geometry + new cols, rows and metrics → payload or nothing), which vitest covers without the DOM.

### D3. One previous log generation

Before opening `nergal.log` with `truncate(true)`, rename it to `nergal.log.1`. One generation is enough to read a crash after the relaunch. A size-capped rotation like `post-session.log` is not needed, because the file restarts on every launch.

Alternative: open in append mode. The file would grow without bound across launches. Rejected.

### D4. Only the process that holds the log lock touches the log

`redirect_journald_stdio_to_logfile()` runs first in `run()` (`src-tauri/src/lib.rs:261`), before the single-instance plugin hands a second launch to the running instance and exits it (`lib.rs:384`; `setup` at `lib.rs:699` runs only in the primary instance). Today a second launch truncates the live instance's log, and with D3 alone it would also rotate it away.

So the log has an owner, set by `flock(LOCK_EX | LOCK_NB)` on `nergal.log.lock`:
- At the start of `run()`, one non-blocking attempt. Success → rotate and redirect, as early as today. Failure → do nothing yet; a second launch reaches the handoff without delay.
- In `setup` (primary only), a process that failed its attempt retries for up to 3 seconds on a background thread, then rotates and redirects. This covers the in-app relaunch (`SettingsPanel.tsx:2315` → `relaunch()`), where the child starts before the parent exits and the parent still holds the lock.
- The lock file is a `std::fs::File` (close-on-exec), returned as a guard that `run()` keeps for the process lifetime. PTY children and the detached `nergal post-session` runner (`obsidian/post_session.rs:119`) never inherit it.

Alternatives:
- **Retry at the start of `run()`.** Every second launch (launcher click, `nergal://` link) would wait 3 seconds before the handoff. Rejected.
- **Move the whole redirect after the single-instance check.** Early startup output (PATH augmentation, plugin init) would miss the file in the normal case. Rejected; only the failed case is deferred.
- **Lock `nergal.log` itself.** The rename in D3 swaps the inode, so a lock on the log file would not stop a racing process that opens the new path. A separate lock file keeps one stable inode. Chosen.

## Risks / Trade-offs

- [The geometry the PTY reports is the canvas size, not the container size] → Same as kitty and wezterm, which report the text area. Apps use it to size images in cells.
- [A font change that keeps cols and rows does not call resize or `sizeCanvas` today (`fit` returns early, `terminalService.ts:1182-1196`)] → `fit` treats a cell-size change like a size change: it runs `sizeCanvas` and resends, so the canvas backing size equals the geometry sent.
- [`u16` overflow for very large canvases] → 65 535 device pixels is far above any real canvas; the frontend clamps to `u16` before sending.
- [Other wezterm-term panics still close the app] → Accepted; out of scope (see Non-Goals). The kept log makes the next one diagnosable.
- [Images are now decoded and kept in cells without being drawn] → Before, a sixel crashed and an iTerm2 image at 0 × 0 was dropped. Now wezterm decodes them (up to its own `check_image_dimensions` limit) and keeps them in cells and scrollback until they scroll out. Accepted until FRI-600 draws them; FRI-600 owns any memory cap.
- [A relaunch whose parent takes more than 3 seconds to exit] → The child keeps its inherited stdout and stderr, which point at the parent's `nergal.log`, and does not own the lock. The next launch owns the log again. Rare; accepted.
- [The `redirect` now can run after threads exist (setup retry)] → `dup2` is atomic per fd; a line written during the swap lands in either the inherited target or the new log. Update the SAFETY comment accordingly.
- [Two launches right after a crash] → If both start before any instance is up, the loser of the single-instance race can rotate the crash log to `.1`, write a few lines and exit; the primary then rotates again on its retry, so `.1` holds the loser's short log. Rare; accepted.
