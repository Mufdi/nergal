Linear: FRI-494

## Why

On a large screen, markdown lines in the right panel and terminal lines get too long to read. Line length for prose reads best at 45–75 characters, and WCAG 1.4.8 caps it at 80. The user is moving to a bigger screen, so the problem grows now.

## What Changes

- New reading mode with two independent flags: one for the main terminal, one for right-panel markdown.
- Terminal flag on: the main terminal uses at most 100 columns, centered in its pane. The PTY receives those columns, so the agent wraps at that width and nothing is cut.
- Markdown flag on: every markdown view of the right panel (Plan, Spec, Obsidian note, Linear and ClickUp descriptions and comments) renders in a centered column of at most `100ch`, the terminal's column count. Tables span the full view width in both states.
- New shortcut `leader f` toggles the flag of the surface that holds focus: the terminal zone toggles the terminal flag, the panel zone toggles the markdown flag. The sidebar and quake zones change nothing.
- Both flags persist in the config file and survive a restart.
- The quake terminal never uses reading mode.

## Capabilities

### New Capabilities

- `reading-mode`: the terminal column cap, the markdown width cap, the focus-routed toggle and the persistence of both flags.

### Modified Capabilities

None. `keyboard-shortcuts` gains one registry entry under its existing rules, and `terminal-wezterm` pixel geometry keeps holding because the canvas size still derives from the column count.

## Impact

- Frontend: `src/components/terminal/terminalService.ts` (column count and canvas placement for the `center` region), `src/components/plan/MarkdownView.tsx` and `src/components/plan/AnnotatableMarkdownView.tsx` (width), `src/stores/shortcuts.ts` (new `leader f` entry), a new reading-mode store, `src/lib/types.ts` and `src/stores/config.ts` (two config keys).
- Backend: `src-tauri/src/config.rs` gains two frontend-owned `bool` fields with `serde(default)`. No migration, no new command.
- No new dependency.
