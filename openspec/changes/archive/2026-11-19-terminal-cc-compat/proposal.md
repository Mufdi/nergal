Linear: FRI-504, FRI-505 (both found in FRI-502)

## Why

Two terminal answers to Claude Code do not match what the canvas shows.

- **FRI-504.** `OSC 10 ; ?` and `OSC 11 ; ?` report wezterm's default palette: black background, grey text (`src-tauri/src/terminal/config.rs:34`). The canvas paints the theme's `--terminal-surface` and `--terminal-foreground` (`src/components/terminal/theme.ts:20-31`). With the light theme, Claude Code is told the background is black and can pick dark-theme colors on white.
- **FRI-505.** Claude Code asks `CSI ? 2026 $ p` and wezterm answers "recognized" (`2`), so Claude Code wraps each frame in `CSI ? 2026 h` … `CSI ? 2026 l`. wezterm-term ignores those modes; its GUI mux handles them. The emitter (`src-tauri/src/terminal/emitter.rs:105-145`) snapshots 8 ms after each chunk, and the reader reads 8 KB at a time (`src-tauri/src/pty.rs:489`). A large redraw can arrive in several chunks and paint half a frame.

## What Changes

- The terminal's default foreground and background follow the active theme. The frontend sends the colors it paints at startup and on every theme change. Live sessions answer the new colors at once, because wezterm reads the config palette on each query.
- `TerminalSession` tracks synchronized-output mode from the bytes it parses, across chunk boundaries. While the mode is on, the emitter holds the snapshot until the mode ends, at most 150 ms after it started.

## Capabilities

### New Capabilities

### Modified Capabilities
- `terminal-wezterm`: adds "Color queries report the theme colors" and "Synchronized output holds the frame".

## Impact

- Backend: `src-tauri/src/terminal/config.rs`, `session.rs`, `emitter.rs`, the `set_terminal_default_colors` command next to `terminal_attach` in `src-tauri/src/pty.rs`, registration in `lib.rs`.
- Frontend: `src/components/terminal/terminalService.ts` (send the colors after `refreshTermTheme`), a pure color normalizer.
- `DECRQM` for mode 2026 keeps answering `2`. Claude Code only checks that the mode is known, and wezterm owns that reply.
