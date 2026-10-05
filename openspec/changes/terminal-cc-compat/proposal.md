Linear: FRI-502

## Why

Claude Code v2.1.247–2.1.287 changed how it uses the terminal. The FRI-502 audit (matrix in `handoff/terminal-matrix.md`) found three gaps that the canvas terminal can close without touching key encoding:

- The app never sends focus reports, so DECSET 1004 is inert.
- Right and middle clicks never reach the TUI, so CC's paste-on-release cannot fire.
- A pasted body that carries `ESC[201~` ends the bracketed paste early. The rest of the body then runs as typed input.

A fourth gap is a latent truncation: wezterm's writer thread ignores short writes, so a mouse report or answerback can reach the PTY cut in half.

## What Changes

- The frontend reports textarea focus and blur to the backend. The backend forwards them to wezterm-term, which emits `CSI I` / `CSI O` only when the app enabled DECSET 1004.
- Over the alt screen, the canvas forwards middle and right press and release to wezterm-term. A right click over the alt screen no longer opens the webview context menu.
- The bracketed-paste encoder strips embedded paste markers (7-bit and C1 forms) from the body before wrapping it.
- The PTY writer adapter given to wezterm-term writes each message whole.

Out of scope, tracked apart:

- Kitty key encoding (Ctrl+Enter send-now, Ctrl+Shift): FRI-503.
- OSC 10/11 colors that ignore the active theme: FRI-504.
- Synchronized output (DEC 2026) in the emitter: FRI-505.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-wezterm`: adds focus reporting, auxiliary mouse button forwarding, paste marker stripping, and whole-message PTY writes.

## Impact

- `src-tauri/src/terminal/session.rs`: new `focus_changed`.
- `src-tauri/src/pty.rs`: new `terminal_focus` command, `strip_paste_markers`, `SharedWriterAdapter::write` uses `write_all`.
- `src-tauri/src/lib.rs`: registers `terminal_focus`.
- `src/components/terminal/terminalService.ts` + new `mouseButtons.ts`: focus/blur listeners, middle/right forwarding, context-menu suppression over the alt screen.
- No new dependencies. No config or schema change.
