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

- [x] 3.1 `TerminalKeyEvent` serde type (shipped in Phase 2 alongside GridUpdate)
- [x] 3.2 `map_event` in `terminal/input.rs` maps code-first with text/key fallback. Covers Enter, Tab, Backspace, Escape, Delete, Insert, Home/End, PageUp/Down, arrows, F1–F24, Numpad Enter, NumLock/CapsLock/ScrollLock, ContextMenu, PrintScreen. Printable chars go through Char('c'); control chars are rejected.
- [x] 3.3 `terminal_input(session_id, event)` Tauri command in `pty.rs`: locks the session, calls `TerminalSession::key_down` (which delegates to wezterm's encoder with the shared PTY writer as sink), wakes the emitter
- [x] 3.4 Kitty keyboard protocol enabled via `CluihudTerminalConfig::enable_kitty_keyboard`. **Also**: `enable_csi_u_key_encoding` is unconditionally true so Ctrl+Backspace / Shift+Enter / Alt+letter are distinctive even when the shell never opts into Kitty.
- [x] 3.5 `Config::terminal_kitty_keyboard` (JSON config, default true via `#[serde(default)]` for backwards compat). `PtyManager::new(kitty_keyboard)` takes the flag; each new session's `CluihudTerminalConfig` is built from it.
- [x] 3.6 Tests (10 total across `input` and `session` modules): Ctrl+Backspace ≠ Backspace, Shift+Enter ≠ Enter, Alt+letter ≠ bare letter, plain 'a' → `b"a"`, CSI-u fallback still works when Kitty is off, modifier bitflag composition, F-key range parsing, physical-key precedence over text fallback, control-char rejection.

## 4. Frontend: terminal renderer core

- [x] 4.1 `src/components/terminal/wezterm/wezTerminalService.ts` — module-scoped service (not a class; mirrors the existing `terminalService.ts` pattern). Owns the per-session `Entry` map, host div, and activeId.
- [x] 4.2 `show(sessionId, cwd, mode)` creates the canvas + container, invokes `start_claude_session`, subscribes to `terminal:grid-update`, and seeds the shadow grid with `terminal_get_full_grid` if the backend already has state (resume case).
- [x] 4.3 `FontAtlas` class backs `fontAtlas.ts` — lazy rasterization into a 128×128 cell glyph cache keyed by `(char, fg, bold, italic)`. Simpler than pre-rasterization; covers any codepoint on first use and evicts by round-robin row when full. `measureFont` computes DPR-aware cell metrics.
- [x] 4.4 `applyUpdate` patches the shadow grid with only the changed rows + cursor + title, then `paintRow` redraws just those rows via the atlas. Cursor paints last so it overlays correctly.
- [x] 4.5 Resize via `computeCols` (reads container getBoundingClientRect + font metrics) → new backend command `resize_session_terminal(session_id, cols, rows)` so the frontend doesn't need to know pty_ids.
- [x] 4.6 No separate fallback path — the atlas's lazy rasterization path doubles as the fallback. Any codepoint the font can render goes through the same `drawGlyph`.

## 5. Frontend: input capture and encoding handoff

- [x] 5.1 `keydown` handler attached to the focusable canvas in `wireInput`; builds `TerminalKeyEvent` from `KeyboardEvent`
- [x] 5.2 `invoke("terminal_input", { sessionId, event })` — no local encoding
- [ ] 5.3 IME composition — deferred. Phase 4 canvas lacks the hidden textarea overlay; CJK/dead-key composition needs Phase 5 follow-up
- [x] 5.4 `shouldPassThrough` mirrors the global-shortcut filter from `terminalService.ts:wireIMEFix` so Ctrl+1/2/3, Ctrl+K, Ctrl+Tab, Ctrl+Shift+letter, Alt+arrows skip the terminal

## 6. Frontend: selection, copy/paste, scrollback

- [x] 6.1 `mousedown/move/up` in `wireInput` — anchor-and-head selection with cell-accurate hit testing via `mouseToCell`. `paintSelection` overlays `WEZ_THEME.selectionBackground` on the selected rectangle; re-applied after per-row repaints so live updates don't wipe the tint.
- [x] 6.2 Ctrl+Shift+C (only when a selection is active; otherwise falls through to cluihud global). `serializeSelection` walks rows, trimming trailing blanks per row, joined with `\n`, then `navigator.clipboard.writeText`.
- [x] 6.3 Ctrl+Shift+V → `navigator.clipboard.readText` → new backend `terminal_paste` command wraps text in `\x1b[200~...\x1b[201~` and writes to the PTY.
- [ ] 6.4 Scrollback — deferred. Needs a `terminal_get_scrollback` backend command and a scroll-offset render mode in the renderer. Not required for MVP; the backend already keeps 10k rows of scrollback, so the state exists, just not exposed to the frontend yet.
- [x] 6.5 Typing clears any active selection before sending the key — matches every mainstream terminal's "type to dismiss highlight" behavior. Auto-scroll-to-bottom is implicit: the renderer is always pinned to the live viewport until 6.4 lands.

## 7. Frontend: OSC 8 hyperlinks

- [x] 7.1 `CellSnapshot.hyperlink: string | null` already surfaces via `terminal:grid-update`. Phase 4's `paintRow` underlines cells (since `cell.underline` follows OSC 8 once wezterm sets it) — independently, the click target is computed from `hyperlink` so the visual and functional cues match.
- [x] 7.2 `mousemove` (non-drag) inspects `entry.grid[row][col].hyperlink`; flips `canvas.style.cursor` to `pointer` on entry and back to `default` on exit.
- [x] 7.3 `mouseup` distinguishes click vs drag: a mousedown+mouseup on the same cell with a hyperlink calls `open(hyperlink)` from `@tauri-apps/plugin-shell`. Drags resolve to selections instead.

## 8. Migration flag and coexistence

- [x] 8.1 `Config::experimental_wezterm_terminal: bool` in the JSON config (default `false`, `#[serde(default)]` for existing configs)
- [x] 8.2 `TerminalManager.tsx` reads `configAtom` and branches: flag-on mounts `WezTerminalManager`, flag-off keeps the legacy `LegacyTerminalManager` with `terminalService.ts`
- [x] 8.3 Dual-emission implemented in Phase 2: backend always emits both `pty:output` and `terminal:grid-update`, so toggling the flag at runtime Just Works
- [x] 8.4 SettingsPanel: refactored to split string-valued fields from boolean toggles (no union-typed inputs); "Wezterm terminal (experimental)" and "Kitty keyboard protocol" exposed as checkboxes

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
