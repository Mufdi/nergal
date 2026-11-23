## Context

The canvas forwarded middle and right buttons only on the alternate screen. This change gates them on the app's mouse-tracking mode, the rule wezterm-term uses to emit a report (`term/src/terminalstate/mouse.rs:144`, `:195`).

## Goals / Non-Goals

**Goals:** middle/right buttons and the context menu follow mouse tracking; reports never name a history row; the user keeps a way to the context menu.

**Non-Goals:** left button, wheel and selection keep their alternate-screen rules.

## Decisions

### D1. Gate on `is_mouse_grabbed()`, live bottom only
Forward when tracking is on and the view is at the live bottom. Alternative: wezterm's GUI scrolls to the bottom and reports (`wezterm-gui/src/termwindow/mouseevent.rs:931-938`). Rejected: a click while reading history would yank the view and act on content the user does not see.

### D2. Shift bypasses reporting
Like wezterm's default `bypass_mouse_reporting_modifiers = SHIFT`, xterm and kitty. Without it a Claude Code pane never shows the context menu. The Menu key has no modifier, so it stays suppressed under tracking; Shift+F10 works.

### D3. The backend enforces the scroll rule too
The frontend `scrollOffset` lags the backend by one update, so a right click right after a wheel scroll could carry a viewport row. `TerminalSession::mouse_button` drops the event on the primary screen while `scroll_offset > 0`. Alternative: frontend only. Rejected: the race is real and the check is one line where the state lives.

### D4. A pure reducer for the held button
`auxReduce(held, event)` handles press, release (pointer cell, or the press cell when the pointer is off the canvas) and focus loss, so the scenarios "Release outside the canvas" and "Focus loss while a button is held" get unit tests instead of only walks. The press and context-menu decisions become pure functions of (button, shift, tracking, offset).

## Risks / Trade-offs

- Synchronized output can hold a mode change up to `SYNC_OUTPUT_CAP`; a right click in that window shows the context menu once. Accepted.
- A crashed app can leave tracking on; a right click then types a report at the shell. xterm and wezterm behave the same.
- Left clicks on the primary screen with tracking on still do not reach the app (out of scope; walk notes say so).
