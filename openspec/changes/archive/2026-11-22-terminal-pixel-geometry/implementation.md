# Implementation Plan: terminal-pixel-geometry

> Grounded in current codebase, symbols verified 2026-11-22. The crash was reproduced with a unit test (sixel bytes into `TerminalSession::new(80, 24, sink)` → `attempt to divide by zero` at `wezterm-term .../terminalstate/image.rs:103:21`), then the spike was deleted.

## Verified codebase facts (do not re-assume)

- `TerminalSession::with_config` builds `TerminalSize { pixel_width: 0, pixel_height: 0, dpi: 0 }` — `src-tauri/src/terminal/session.rs:96-127`. `TerminalSession::resize(cols, rows)` does the same — `session.rs:175-186`.
- wezterm-term (git tag `20240203-110809-5046fc22`, checkout under `~/.cargo/git/checkouts/wezterm-26cdb9e734a97642/5046fc2/`): `assign_image_to_cells` computes `cell_pixel_width = self.pixel_width / physical_cols` (`term/src/terminalstate/image.rs:72-73`) and divides by it at `:103` and `:119`. `CSI 14 t` answers `4;<pixel_height>;<pixel_width>t` and `CSI 16 t` answers `6;<pixel_height/rows>;<pixel_width/cols>t` (`term/src/terminalstate/mod.rs:2029-2048`; formats in `termwiz/src/escape/csi.rs:503-536`). DA1 advertises sixel (`mod.rs:1283-1288`).
- Single construction point for app sessions: `build_terminal_session(cols, rows, writer)` — `src-tauri/src/pty.rs:138-144`. Called by `spawn_pty` at `pty.rs:474` and by tests at `pty.rs:2239`, `:2312`.
- `spawn_pty(pty_id, cols, rows, cwd, session_id, agent_session, shell_ready_tx)` opens the PTY with `PtySize { pixel_width: 0, pixel_height: 0 }` — `pty.rs:422-442`. Callers: `start_claude_session` (`pty.rs:705`) and `spawn_aux_shell` (`pty.rs:1113`).
- `resize_pty(state, pty_id, cols, rows)` resizes the PTY master (`PtySize` zeros, `pty.rs:567-572`) and the session (`guard.resize(cols, rows)`, `pty.rs:578`). Only caller: `resize_session_terminal(state, session_id, cols, rows)` — `pty.rs:1495-1509`.
- Command signatures: `start_claude_session(app, state, agents, db, session_id, cwd, cols, rows, resume)` — `pty.rs:665-675`; `spawn_aux_shell(app, state, db, session_id, shell_id, cwd, shell_cwd, base_dir, cols, rows, command, autorun)` — `pty.rs:1041-1054`.
- PTY test harness: `build_test_pty_instance` opens a real PTY — `pty.rs:2290-2300`; tests module starts at `pty.rs:1993`. `portable_pty::MasterPty::get_size()` returns the `PtySize` incl. pixel fields.
- Session test helpers with a captured writer (for answerbacks): `session_compat_tests.rs:1-40` (`CapturedWriter::drain` polls up to 500 ms because wezterm writes answerbacks from a background thread).
- Frontend: `FontMetrics.cellWidth`/`cellHeight` are device pixels, integer ≥ 1 (`src/components/terminal/fontAtlas.ts:13-66`; `cssWidth = cellWidth / dpr`). `sizeCanvas` sets `canvas.width = cols * metrics.cellWidth` (`terminalService.ts:1170-1180`).
- Frontend spawn: `showTerminal` gets `spawn: (cols, rows) => Promise` (`terminalService.ts:304`), called at `:436`. Center passes `cols, rows` to `start_claude_session` (`:253-261`); quake passes them to `spawn_aux_shell` (`:284-296`). Quake post-spawn resize at `:442-444`.
- `fit(entry)` returns early when `cols`/`rows` are unchanged (`terminalService.ts:1182-1196`). The theme/font restyle path calls `fit(entry)` after swapping `entry.metrics` (`terminalService.ts:175-186`), so a font change with unchanged cols/rows never reaches the backend today.
- Headless autorun shells spawn from `src/stores/quake.ts:142-151` with a 200 × 50 placeholder and no canvas: they must keep working without pixel params (fallback cell).
- Pure, vitest-tested helpers live next to the service (`mouseButtons.ts` + `mouseButtons.test.ts`, `passThrough.ts` + `.test.ts`). `pnpm test` runs vitest.
- Log redirect: `redirect_journald_stdio_to_logfile()` opens `<cache>/nergal/nergal.log` with `truncate(true)` and `dup2`s it onto stdout/stderr — `src-tauri/src/lib.rs:56-103`; non-Linux stub at `:105-106`; called at `lib.rs:261`. Readers of `nergal.log` (diagnostics, updater) at `updater.rs:376-383`, `:437`, `lib.rs:108-120` read only `nergal.log`.

- `start_claude_session` returns an existing PTY early through `reserve_session_pty` (`pty.rs:697-701`) and `spawn_aux_shell` returns early when `session_ptys` already holds the `term_id` (`pty.rs:1098-1110`); neither resizes. The frontend comment at `terminalService.ts:438-441` assumes the center PTY "was just created at exactly these dims", which is false on re-attach.
- wezterm `iterm.rs:42-48` drops an OSC 1337 image when `pixel_width == 0 || pixel_height == 0` (no panic); a nonzero geometry below one pixel per cell (e.g. 10 × 10 on 80 × 24) reaches `image.rs:103` and panics. Sixel (`sixel.rs`) never checks the pixel size.
- `PtyManager::new()` (`pty.rs:147`) is used by tests (`pty.rs:2215`, `:2327`, …) together with `build_test_pty_instance`, so a test can insert a real `PtyInstance` and call `resize_pty`, then read `master.get_size()`.
- `setup` (`lib.rs:699`) runs only in the primary instance: the single-instance plugin exits a second launch before it (`lib.rs:138-143` doc comment, BUG-39). In-app relaunch: `SettingsPanel.tsx:2315` → `relaunch()`.
- The detached `nergal post-session` runner is spawned at `obsidian/post_session.rs:119`; any non-CLOEXEC fd would leak into it.

## Execution order

1. Backend geometry (`session.rs`, then `pty.rs`).
2. Frontend geometry (pure helper + `terminalService.ts`).
3. Log rotation (`lib.rs`).

Groups 1 and 3 are independent. Group 2 depends on the command parameter names from group 1 (`pixelWidth`, `pixelHeight` in camelCase on the JS side; Tauri maps them to `pixel_width`, `pixel_height`).

## Plan

### 1. Backend geometry
- `session.rs`: fallback cell constant (8 × 16) and one pure function `(cols, rows, Option<u16>, Option<u16>) → (pixel_width, pixel_height)`: missing or below one pixel per cell → `cols × 8` / `rows × 16`; computed in `usize`, saturated at 65 535. `with_config` and `resize` accept the optional pixel arguments and use it (keep a cols/rows-only entry point mapping to `None` if that keeps existing tests compiling).
- `pty.rs`: one helper builds the `PtySize` from the same function, used by `spawn_pty` and `resize_pty`, so PTY and emulator always agree. Thread `pixel_width: Option<u16>`, `pixel_height: Option<u16>` through `start_claude_session`, `spawn_aux_shell`, `resize_session_terminal`, `spawn_pty`, `resize_pty`, `build_terminal_session`.
- `resize_pty`: an unchanged size is a full no-op. wezterm's `TerminalState::resize` is **not** idempotent: it always resets the scroll margins to the full screen and repositions the cursor (`term/src/terminalstate/mod.rs:878-883`). So `TerminalSession` stores its clamped pixel size next to `cols`/`rows`, and a pure predicate (stored size vs requested clamped size) decides. Unchanged → skip the master resize (no `SIGWINCH`), the emulator resize and `differ.invalidate()`. The predicate is the test seam for "no `SIGWINCH`"; the scroll-region check (`CSI 5;10 r` before, still active after) covers the emulator half.

### 2. Frontend geometry
- New `src/components/terminal/pixelGeometry.ts` (vitest-tested):
  - `pixelGeometry(cols, rows, metrics) → { pixelWidth, pixelHeight }` (`cols × cellWidth`, `rows × cellHeight`, clamped to 65 535).
  - a decision function: last sent `{cols, rows, pixelWidth, pixelHeight} | null` + new cols, rows, metrics → the `resize_session_terminal` payload, or `null` when nothing changed.
- `terminalService.ts`:
  - `showTerminal`'s `spawn` callback receives the geometry; both spawn calls send `pixelWidth` / `pixelHeight`.
  - After `await spawn(...)`, call `resize_session_terminal` with the geometry for **both** regions (replaces the quake-only call at `:442-444`; fix its comment), and record it as the last sent geometry on the entry.
  - `fit`: always bring `entry.cols`, `entry.rows`, the grid and the canvas in line with the computed cols, rows and current metrics (`applyUpdate` at `terminalService.ts:1243-1247` rewrites `entry.cols` from the backend and relies on the next `fit` to correct it). The decision function only gates the IPC call. `sizeCanvas` takes its backing size from `pixelGeometry`, so the pure tests cover the canvas size too.
  - The last sent geometry is recorded only when the invoke resolves; a rejected call leaves it unchanged, so the next `fit` retries.
  - The post-spawn resize reads `entry.cols`, `entry.rows` and `entry.metrics` after `await spawn(...)`, never the values captured before it. Today nothing can change them during the await (the entry joins `entries` only after attach, about `:467`); this is defensive.
  - `fit` skips a container with no layout (`display: none` gives a 0 × 0 rect). Today `restyleTerminals` fits every entry, and `computeCols` falls back to 80 × 24 for hidden ones (`:1153-1154`), so a font change resizes hidden terminals to 80 × 24. With the cell-size resend that would also send them `SIGWINCH`. The next `showTerminal` fits them correctly.

### 3. Log rotation and ownership
- Extract from `redirect_journald_stdio_to_logfile` (Linux-gated; non-Linux stubs stay empty), all taking the log dir so tests use a `tempfile` dir and never `dup2`:
  - `try_lock_log(dir) -> Option<File>`: open/create `nergal.log.lock` with `std::fs::OpenOptions` (close-on-exec by default), one `libc::flock(fd, LOCK_EX | LOCK_NB)`.
  - `retry_lock_log(dir, timeout) -> Option<File>`: poll `try_lock_log` (e.g. every 50 ms) until the timeout.
  - `rotate_log(dir)`: rename `nergal.log` → `nergal.log.1`; ignore `NotFound`, print other errors to stderr and continue.
- `run()`: at start, when `JOURNAL_STREAM` is set, `try_lock_log`. Success → `rotate_log`, open `nergal.log` with `truncate(true)`, `dup2` (today's code). Keep the returned `File` alive for the process lifetime (held by `run()` or moved into managed state; never a `static` inside the extracted functions).
- In `setup` (primary only): if the startup attempt failed, spawn a thread that runs `retry_lock_log(dir, 3 s)`; on success rotate, open and `dup2`, and keep the guard alive. Update the `dup2` SAFETY comment: it may now run after threads exist; `dup2` is atomic per fd.

## Per-phase risk

- [A missed caller keeps passing zeros] → The clamp lives inside `TerminalSession`, so every path is safe even if a caller forgets; the spec's "no panic" scenarios run against `TerminalSession` directly.
- [Tauri arg naming mismatch silently drops the params] → Optional params arrive as `None` and fall back without error; the frontend test asserts the invoke payload keys `pixelWidth`/`pixelHeight`.
- [Changing `resize` signature breaks the 10+ existing session tests] → Keep the cols/rows-only call working, or update callers mechanically; never edit locked Red tests.
- [Rename fails on a read-only cache dir] → Falls through to the existing open path, which already reports its own error.
- [Lock fd leaks into children and keeps the lock after Nergal dies] → `std::fs::File` only (close-on-exec); spec scenario "Children do not inherit the lock" tests it with a real child process.
- [Tests share a global lock guard] → The extracted functions return the guard; tests hold two guards from two `try_lock_log` calls on the same temp dir (flock locks are per open file description, so a second open in the same process conflicts).

## Verification

- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit && pnpm test` (`cargo fmt --check` only; format touched files with `rustfmt --edition 2024 <files>`).
- Manual: install the `.deb`, open yazi in the quake, move over several images: Nergal stays up. `printf '\e[14t'` inside a shell answers the canvas size. After killing Nergal, `~/.cache/nergal/nergal.log.1` holds the previous run.
