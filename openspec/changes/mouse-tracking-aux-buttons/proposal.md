Linear: FRI-506

## Why

The canvas forwards the middle and right buttons to wezterm-term only on the alternate screen, and it suppresses the webview context menu on the same condition (`src/components/terminal/mouseButtons.ts:10`, `src/components/terminal/terminalService.ts:864-866`). The right condition is whether the running app enabled mouse tracking (DECSET 1000, 1002 or 1003), the rule wezterm-term itself uses to emit a report. Today an alternate-screen app without mouse tracking loses the context menu and gains nothing, and an app that tracks the mouse on the primary screen never gets the buttons.

## What Changes

- Grid snapshots and grid updates carry `mouse_tracking` (`mouseTracking` in the frontend), from wezterm-term's `is_mouse_grabbed()`.
- The differ emits an update when `mouse_tracking` changes, even when no row, cursor or title changed, so a mode switch without a redraw still reaches the canvas.
- Middle and right button forwarding and the context-menu suppression follow `mouseTracking` instead of `isAltScreen`, and only while the view sits at the live bottom (`scrollOffset === 0`): a scrolled-back view shows history, so a report would name the wrong row. wezterm's GUI instead scrolls to the bottom and reports (`wezterm-gui/src/termwindow/mouseevent.rs:931-938`); Nergal keeps the history view and sends nothing, and the backend enforces the same rule so a click right after a wheel scroll cannot slip through.
- Shift bypasses mouse reporting, like wezterm's default `bypass_mouse_reporting_modifiers` (`mouseevent.rs:924-929`), xterm and kitty: Shift+right-click opens the context menu even in a mouse-tracking app.
- Claude Code enables mouse tracking, so in its pane a right or middle click reaches Claude Code (paste on release) instead of opening the context menu, on either screen. The issue asks for wezterm's rule, which does the same.
- Wheel, left-button and selection behaviour keep their alternate-screen rules (out of scope).

## Capabilities

### New Capabilities

### Modified Capabilities
- `terminal-wezterm`: the "Middle and right buttons reach the alt-screen app" requirement becomes "Middle and right buttons reach a mouse-tracking app".

## Impact

- `src-tauri/src/terminal/types.rs`, `src-tauri/src/terminal/session.rs`, `src-tauri/src/terminal/differ.rs`.
- `src/lib/types.ts`, `src/components/terminal/mouseButtons.ts`, `src/components/terminal/terminalService.ts`.
- No config, IPC command or schema change; `GridUpdate` gains one field.
