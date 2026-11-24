Linear: FRI-507

## Why

`terminal_paste` always wraps the clipboard text in `ESC[200~ … ESC[201~` (`src-tauri/src/pty.rs:1987`, `write_bracketed` at `:1995`). A program that did not enable DECSET 2004 (`cat`, `read`, an old REPL in a quake shell) then shows the markers as literal text (`^[[200~`). xterm and wezterm's `send_paste` (`term/src/terminalstate/mod.rs:820-843`) wrap only when the app enabled 2004, and otherwise write the text with newlines turned into `\r` on Unix (`term/src/config.rs:117-125`).

## What Changes

- `terminal_paste` (the user's paste) wraps only when the session's terminal reports `bracketed_paste_enabled()`.
- Without 2004 the text goes out plain, with embedded paste markers still removed and every newline (`\r\n`, `\n`, `\r`) written as `\r`, as wezterm does on Unix (newlines first, then markers).
- `paste_to_session` (MCP, Linear and ClickUp deliveries to an agent session) keeps wrapping always, and the PR viewer's "Apply with Claude" moves from `terminal_paste` to a new `terminal_deliver` command backed by it (`src/components/git/PrViewer.tsx:501`): a delivery can arrive before Claude Code enables 2004, and without markers a multi-line body would split into several turns.

## Capabilities

### New Capabilities

### Modified Capabilities
- `terminal-wezterm`: the "Bracketed paste" scenario of "Selection and clipboard" depends on DECSET 2004; a new scenario covers the plain paste.

## Impact

- `src-tauri/src/pty.rs` (`terminal_paste`, the paste encoder), `src-tauri/src/terminal/session.rs` (an accessor for the mode).
- `src-tauri/src/lib.rs` (register `terminal_deliver`), `src/components/git/PrViewer.tsx` (uses it). No config or schema change.
