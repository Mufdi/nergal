## 1. Characterize wezterm-term answers (FRI-502 matrix evidence)

- [x] 1.1 Test: kitty query `CSI ? u` is never answered (`src-tauri/src/terminal/session.rs`)
- [x] 1.2 Test: DA1, XTVERSION, OSC 11 and DECRQM 2026 answers (`src-tauri/src/terminal/session.rs`)
- [x] 1.3 Test: current key encoding truth for the MODIFIED "Input encoding via wezterm" requirement (`kitty_flags_push_does_not_change_ctrl_enter`, `modify_other_keys_distinguishes_ctrl_enter`)
- [x] 1.4 Test (Red): a session built through `build_terminal_session` does not answer `CSI ? u` and a config JSON with `terminal_kitty_keyboard: true` still deserializes (`app_session_ignores_legacy_kitty_keyboard_config_key`, `legacy_terminal_kitty_keyboard_key_is_ignored`)
- [x] 1.5 Remove `terminal_kitty_keyboard` from `Config`, `PtyManager`, `NergalTerminalConfig::with_kitty_keyboard`, the frontend config type/defaults and the Settings Terminal section (`src-tauri/src/config.rs`, `src-tauri/src/pty.rs`, `src-tauri/src/terminal/config.rs`, `src/components/settings/SettingsPanel.tsx`)

## 2. Focus reporting

- [x] 2.1 Test (Red): focus reports only after DECSET 1004, no repeats (`focus_change_is_reported_only_after_decset_1004`, `repeated_focus_state_is_not_reported_twice`)
- [x] 2.2 Add `TerminalSession::focus_changed` (`src-tauri/src/terminal/session.rs`)
- [x] 2.3 Add `terminal_focus` command and register it (`src-tauri/src/pty.rs`, `src-tauri/src/lib.rs`)
- [x] 2.4 Send focus/blur of the hidden textarea to `terminal_focus` (`src/components/terminal/terminalService.ts`)
- [x] 2.5 Re-check blur after 100 ms (`hasInputFocus`) and send the real focus state once after attach (review finding)
- [x] 2.6 Skip the settle-timeout report when the entry was destroyed (`entries.get(key) === entry`)
- [x] 2.7 Report focus loss explicitly when a visible terminal is hidden: tab switch (`showTerminal`), `hideAll`, `setHost(null)` (no reliance on WebKitGTK blur for display:none)

## 3. Middle and right buttons over the alt screen

- [x] 3.1 Test (Red): wezterm-term reports middle/right press and release in SGR (`right_and_middle_buttons_report_press_and_release`)
- [x] 3.2 Test (Red): `auxButtonToForward` maps buttons 1/2 only over the alt screen (`src/components/terminal/mouseButtons.test.ts`)
- [x] 3.3 Add `src/components/terminal/mouseButtons.ts` and forward press/release from the canvas
- [x] 3.4 Suppress the webview context menu over the alt screen
- [x] 3.5 Deliver the aux release through a window-level `mouseup` listener (review finding)
- [x] 3.6 Test (Red): held-slot logic — press→release, press→blur→release sends nothing, second press ignored (`mouseButtons.test.ts` "aux held slot")
- [x] 3.7 Replace per-press closures with one `entry.auxHeld` slot (`auxPress` / `auxRelease`), one module-level window `mouseup`, slot cleared on window `blur` and on `sendFocus(false)`

## 4. Paste marker stripping

- [x] 4.1 Test (Red): embedded, rebuilt and C1 markers are stripped; color sequences survive (`src-tauri/src/pty.rs`)
- [x] 4.2 Add `strip_paste_markers` and call it from `write_bracketed`

## 5. Whole-message PTY writes

- [x] 5.1 Test (Red): `SharedWriterAdapter` delivers a 9-byte report through a 3-byte writer (`shared_writer_adapter_never_splits_a_report_on_short_writes`)
- [x] 5.2 Switch `SharedWriterAdapter::write` to `write_all`
- [x] 5.3 Test: write errors propagate (`shared_writer_adapter_propagates_write_errors`); log `tracing::warn!` before returning them

## 6. Verification

- [x] 6.1 (re-run after review fixes) `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit && pnpm test`
- [x] 6.2 Manual (live CC ≥ v2.1.289 in the installed app): focus reports — steps in `handoff/terminal-matrix.md` § Manual checks
- [x] 6.3 Manual: right/middle click paste over CC fullscreen — steps in `handoff/terminal-matrix.md`
- [ ] 6.4 Manual: Ctrl+V paste of a body containing `ESC[201~` in an aux shell — steps in `handoff/terminal-matrix.md`
