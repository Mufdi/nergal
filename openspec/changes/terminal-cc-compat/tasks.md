## 1. Characterize wezterm-term answers (FRI-502 matrix evidence)

- [x] 1.1 Test: kitty query `CSI ? u` answered with `CSI ? 0 u`, silent when kitty is off (`src-tauri/src/terminal/session.rs`)
- [x] 1.2 Test: DA1, XTVERSION, OSC 11 and DECRQM 2026 answers (`src-tauri/src/terminal/session.rs`)

## 2. Focus reporting

- [x] 2.1 Test (Red): focus reports only after DECSET 1004, no repeats (`focus_change_is_reported_only_after_decset_1004`, `repeated_focus_state_is_not_reported_twice`)
- [x] 2.2 Add `TerminalSession::focus_changed` (`src-tauri/src/terminal/session.rs`)
- [x] 2.3 Add `terminal_focus` command and register it (`src-tauri/src/pty.rs`, `src-tauri/src/lib.rs`)
- [x] 2.4 Send focus/blur of the hidden textarea to `terminal_focus` (`src/components/terminal/terminalService.ts`)
- [x] 2.5 Re-check blur after 100 ms (`hasInputFocus`) and send the real focus state once after attach (review finding)

## 3. Middle and right buttons over the alt screen

- [x] 3.1 Test (Red): wezterm-term reports middle/right press and release in SGR (`right_and_middle_buttons_report_press_and_release`)
- [x] 3.2 Test (Red): `auxButtonToForward` maps buttons 1/2 only over the alt screen (`src/components/terminal/mouseButtons.test.ts`)
- [x] 3.3 Add `src/components/terminal/mouseButtons.ts` and forward press/release from the canvas
- [x] 3.4 Suppress the webview context menu over the alt screen
- [x] 3.5 Deliver the aux release through a window-level `mouseup` listener (review finding)

## 4. Paste marker stripping

- [x] 4.1 Test (Red): embedded, rebuilt and C1 markers are stripped; color sequences survive (`src-tauri/src/pty.rs`)
- [x] 4.2 Add `strip_paste_markers` and call it from `write_bracketed`

## 5. Whole-message PTY writes

- [x] 5.1 Test (Red): `SharedWriterAdapter` delivers a 9-byte report through a 3-byte writer (`shared_writer_adapter_never_splits_a_report_on_short_writes`)
- [x] 5.2 Switch `SharedWriterAdapter::write` to `write_all`

## 6. Verification

- [x] 6.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit && pnpm test`
- [ ] 6.2 Manual (live CC ≥ v2.1.289 in the installed app): focus reports — steps in `handoff/terminal-matrix.md` § Manual checks
- [ ] 6.3 Manual: right/middle click paste over CC fullscreen — steps in `handoff/terminal-matrix.md`
- [ ] 6.4 Manual: Ctrl+V paste of a body containing `ESC[201~` in an aux shell — steps in `handoff/terminal-matrix.md`
