Linear: FRI-475

## Why

The quake slide looks stuck and jerky on open and close (user walk, 2026-10-05). The overlay mounts on every open and unmounts on every close. Each open builds a new compositing layer, re-parents the shell canvases, and repaints the grid twice in the first frames of a 200 ms slide. With the panel glow on, the focus change also animates a large blurred `box-shadow` on the sliding element and on the agent pane under it. That repaints every frame of the slide. The exit timer also unmounts on a fixed 200 ms, so a late-starting exit is cut short.

## What Changes

- The quake overlay stays mounted for the app lifetime. A closed overlay is translated above the content area and hidden with `visibility: hidden`.
- The overlay keeps one pinned compositing layer (`will-change: transform`). The slide animates only `transform`.
- The slide is a four-phase state machine (hidden, entering, shown, exiting). `animationend` ends a slide, with a timer fallback. A session switch that closes the overlay plays the same exit slide.
- During a slide, `.nergal-panel-focus` border and glow transitions are off on the overlay and on the panels below it. The glow change happens in one frame, not over 200 ms.
- Host registration and the ResizeObserver run once per mount, not once per open. Reopening the already-active shell does not re-fit or repaint it.
- Focus inside a hidden overlay is released, and a stale `quake` focus zone resets to `terminal`. Keystrokes never reach a hidden shell.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `quake-terminal`: the "Quake terminal overlay" requirement gains scenarios for the slide on open and close, and for scrollback kept across close and reopen.

## Impact

- `src/components/quake/QuakeTerminal.tsx`: always-mounted render, phase state, single host/observer registration.
- `src/components/quake/quakeSlide.ts` (new): pure phase transitions, unit-tested.
- `src/styles/globals.css`: `quake-hidden` class, pinned layer, slide-time transition freeze.
- `src/components/terminal/terminalService.ts`: one read-only helper to ask whether a shell is the quake's active terminal.
- No backend, schema, or shortcut change. `Ctrl+}` semantics stay the same.
