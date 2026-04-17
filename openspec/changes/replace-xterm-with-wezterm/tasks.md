## 1. Backend: VT core with wezterm-term

- [x] 1.1 Add `wezterm-term` and `termwiz` as git dependencies in `src-tauri/Cargo.toml` pinned to wezterm tag `20240203-110809-5046fc22` (last stable release; crates are not published on crates.io, only live inside the wezterm workspace)
- [x] 1.2 Create module `src-tauri/src/terminal/mod.rs` exposing `TerminalSession` and `CluihudTerminalConfig` (no `TerminalManager` yet — Phase 2 adds the multi-session manager once IPC lands)
- [x] 1.3 Implement `TerminalSession`: holds one `wezterm_term::Terminal` instance + size. Writer is accepted as `Box<dyn Write + Send>` and handed to wezterm; Phase 2 will share it with `pty_write` via an `Arc<Mutex<_>>` adapter
- [x] 1.4 Implement `TerminalSession::advance_bytes(&mut self, bytes: &[u8])` that forwards to the inner terminal
- [x] 1.5 Implement `TerminalSession::resize(&mut self, cols: u16, rows: u16)` that resizes the emulator model. PTY ioctl resize stays on `pty.rs` side — Phase 2 composes them
- [x] 1.6 Implement `TerminalSession::grid_snapshot(&self) -> GridSnapshot` returning the full grid + cursor + title
- [x] 1.7 Unit tests: plain text, CRLF, SGR colors, clear + home, OSC 8 hyperlinks, resize, OSC window title — all passing (7/7)

## 2. Backend: grid diffing and emission

- [x] 2.1 Define `GridUpdate`, `GridRow`, and `TerminalKeyEvent` serde types in `src-tauri/src/terminal/types.rs` (camelCase for JS consumption). `CellSnapshot` from Phase 1 is reused as the cell representation.
- [x] 2.2 Implement `GridDiffer` in `differ.rs` that keeps per-row content hashes + last cursor + last title, returns only changed rows, and supports `invalidate()` for forced full resends
- [x] 2.3 `TerminalHandle::spawn_emitter` launches a `tauri::async_runtime::spawn` task: awaits the `Notify`, sleeps 8ms to coalesce, snapshots the session, diffs, emits `terminal:grid-update` if there is a change
- [x] 2.4 Reader thread in `pty.rs:spawn_pty` feeds chunks into `TerminalSession::advance_bytes` and calls `notify_one()` in parallel with the legacy `pty:output` emission (dual-emission per migration plan)
- [x] 2.5 Add `terminal_get_full_grid` Tauri command that invalidates the differ and returns a complete `GridUpdate` for the session
- [x] 2.6 Differ-level tests cover: first-call emits all rows, identical snapshot emits nothing, single-row change, cursor-only move, invalidate forces resend, row-count change. End-to-end coalescing behavior validated manually once the frontend consumer lands in Phase 4.

## 3. Backend: input encoding via wezterm

- [ ] 3.1 Define `TerminalKeyEvent` serde type: `{ code, key, text?, ctrl, shift, alt, meta }`
- [ ] 3.2 Implement a mapping `TerminalKeyEvent -> (wezterm_term::KeyCode, Modifiers)` covering: printable keys, arrows, function keys, Enter, Tab, Backspace, Delete, Home/End, PageUp/Down, Escape
- [ ] 3.3 Implement new command `terminal_input(session_id, event)` that calls `session.key_down(code, mods)` — wezterm encodes and writes to its registered callback (wire the PTY writer as the sink)
- [ ] 3.4 Enable Kitty keyboard protocol at terminal init: `Terminal::set_config(TerminalConfiguration { enable_kitty_keyboard: true, ... })`
- [ ] 3.5 Expose config toggle `terminal.kitty_keyboard` in `config.toml` (default true); pass through to `TerminalSession::new`
- [ ] 3.6 Tests: Ctrl+Backspace encodes to distinct bytes from Backspace; Shift+Enter encodes distinct from Enter; Alt+letter encodes via CSI-u

## 4. Frontend: terminal renderer core

- [ ] 4.1 Add `src/components/terminal/wezterm/TerminalRenderer.ts` — class managing canvas instances, one per session, fully outside React (mirrors the existing `terminalService.ts` pattern)
- [ ] 4.2 Implement `mount(sessionId, container)`: creates `<canvas>`, attaches it, subscribes to `terminal:grid-update` for that session, requests initial full grid via `terminal_get_full_grid`
- [ ] 4.3 Implement font atlas: on first render, rasterize ASCII printable + common Latin-1 into an `OffscreenCanvas`, indexed by `(codepoint, fg, bg, attrs)`. Regenerate on font/theme change.
- [ ] 4.4 Implement render loop: apply incoming `GridUpdate`, blit changed cells from atlas to canvas, draw cursor
- [ ] 4.5 Implement resize: ResizeObserver on container → compute new cols/rows from canvas size and font metrics → call `terminal_resize`
- [ ] 4.6 Fallback path for non-atlased codepoints: direct `fillText` (slow path, acceptable for emoji/CJK until post-MVP)

## 5. Frontend: input capture and encoding handoff

- [ ] 5.1 Attach `keydown` handler to the canvas (or a focusable wrapper div) — build `TerminalKeyEvent` from `KeyboardEvent`
- [ ] 5.2 Call `invoke("terminal_input", { sessionId, event })` — no local encoding
- [ ] 5.3 Replicate IME composition pattern: `compositionstart/compositionend` on a hidden textarea overlaying the canvas; composed text goes via `terminal_input` with `text` field set and `code="IME"`
- [ ] 5.4 Ensure global cluihud shortcuts (Ctrl+1/2/3, Ctrl+K, Ctrl+Tab, etc.) still work: keydown handler filters them before sending to backend (same list as today in `terminalService.ts:wireIMEFix`)

## 6. Frontend: selection, copy/paste, scrollback

- [ ] 6.1 Mouse-drag selection: `mousedown` → track start cell; `mousemove` → compute end cell and highlight range; `mouseup` → freeze selection
- [ ] 6.2 Copy: Ctrl+C (when selection active and not passing through to shell via SIGINT concerns) → extract cells from selection range via `terminal_copy_selection` command → `navigator.clipboard.writeText`
- [ ] 6.3 Paste: Ctrl+Shift+V → `navigator.clipboard.readText` → send as bracketed paste via `terminal_paste` command (backend wraps in `\x1b[200~...\x1b[201~`)
- [ ] 6.4 Scrollback: wheel event → track scroll offset in frontend → include offset in subsequent render requests → backend provides scrollback rows via `terminal_get_scrollback(session_id, offset, lines)`
- [ ] 6.5 Scroll-to-bottom on new input or when user types (match xterm.js behavior)

## 7. Frontend: OSC 8 hyperlinks

- [ ] 7.1 Surface `hyperlink` field in the `Cell` type; renderer underlines cells with a hyperlink
- [ ] 7.2 Track hover: mouseover a hyperlink cell → cursor changes to pointer
- [ ] 7.3 Click a hyperlink cell → open in system browser via Tauri `shell.open` (for https/http) or file handler (for file://)

## 8. Migration flag and coexistence

- [ ] 8.1 Add config flag `experimental.wezterm_terminal: bool` (default `false`) in `config.toml`
- [ ] 8.2 `TerminalManager.tsx` reads the flag; mounts `TerminalCanvas` (new) or legacy `terminalService` host based on flag
- [ ] 8.3 During Fase 2-3, backend emits BOTH `pty:output` (legacy) and `terminal:grid-update` (new) so switching is hot-reload safe
- [ ] 8.4 Settings UI: simple toggle in `Settings` panel to flip the flag without editing the TOML

## 9. Testing

- [ ] 9.1 Backend unit tests for VT sequences (per task 1.7)
- [ ] 9.2 Backend unit tests for input encoding (per task 3.6)
- [ ] 9.3 Integration test: spawn PTY with `bash -c 'echo hello; ls; exit'`, capture grid updates, assert final state
- [ ] 9.4 Manual E2E test script: run a Claude CLI session, verify: colored prompts, tool output, plan mode text, Ctrl+Backspace, Shift+Enter, copy/paste, OSC 8 on a path like `file:///tmp/foo.txt`
- [ ] 9.5 Stress test: `yes | head -100000` to verify coalescing holds frame rate
- [ ] 9.6 Regression check on hooks: SessionStart, PreToolUse, PostToolUse fire correctly during a session on the new terminal (should be unaffected but verify)

## 10. Cut-over and cleanup

- [ ] 10.1 Flip default to `experimental.wezterm_terminal = true`
- [ ] 10.2 Soak period: 1 week of daily use, log any regressions
- [ ] 10.3 Remove `@xterm/xterm` and `@xterm/addon-fit` from `package.json`; delete legacy `terminalService.ts` and the dual-emission path in `pty.rs`
- [ ] 10.4 Remove the flag and the coexistence code
- [ ] 10.5 Update `CLAUDE.md` stack table: xterm.js → wezterm-term
