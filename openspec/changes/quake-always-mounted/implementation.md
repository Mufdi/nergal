## Verified codebase facts (do not re-assume)

- `src/components/quake/QuakeTerminal.tsx:55-71` — `rendered` trails `open` through a `setTimeout(SLIDE_MS)`; a session switch sets it false at once.
- `src/components/quake/QuakeTerminal.tsx:84-88` — `setHost(hostRef.current, "quake")` runs on every `rendered` flip and on `activeSessionId` change; cleanup sets the host to null.
- `src/components/quake/QuakeTerminal.tsx:90-116` — the show effect calls `terminalService.showShell` on every open, even for the shell already active.
- `src/components/quake/QuakeTerminal.tsx:118-127` — the ResizeObserver is created on every open; each callback runs `fitActive("quake")` in a rAF.
- `src/components/quake/QuakeTerminal.tsx:129` — the component returns `null` when closed or without an active session.
- `src/components/quake/QuakeTerminal.tsx:160` — the overlay carries `nergal-panel-focus`, `shadow-xl`, `border-2 ${borderClass}`, and `quake-enter`/`quake-exit`.
- `src/styles/globals.css:707-710` — `.nergal-panel-focus` transitions `border-color` and `box-shadow` over 200 ms.
- `src/styles/globals.css:785-791` — with `data-panel-glow="on"`, `.nergal-panel-focus.border-primary` gets a four-layer `box-shadow` (blur up to 28 px).
- `src/styles/globals.css:1050-1067` — `quake-slide-down` / `quake-slide-up` keyframes, 200 ms; exit uses `forwards`.
- `src-tauri/src/config.rs:412` — `panel_glow` defaults to false; the user's `~/.config/nergal/config.json` has `"panel_glow": true`.
- `src/stores/shortcuts.ts:241-266` — `toggleQuake`: open sets the atom and calls `focusZone("quake")` one rAF later; close sets the atom and calls `focusZone("terminal")` at once.
- `src/stores/shortcuts.ts:206-216` — `focusZone("quake")` sets `focusZoneAtom` and calls `terminalService.focusActive("quake")`.
- `src/components/terminal/terminalService.ts:176-188` — `setHost` re-parents every region container that is not under the new host.
- `src/components/terminal/terminalService.ts:264-277` — `showTerminal` toggles container `display` and, for an existing entry, runs `fit` + `paintAll` in the next rAF.
- `src/components/terminal/terminalService.ts:1088-1102` — `fit` returns early when cols and rows did not change.
- `src/components/terminal/terminalService.ts:1184-1190` — `applyUpdate` paints touched rows on every update, mounted or not.
- `src/components/layout/Workspace.tsx:357-359` — `<QuakeTerminal />` is the first child of the content area; `ResizablePanelGroup` is its next sibling.
- `vitest.config.ts` — node environment, `src/**/*.test.ts`; no DOM. Phase logic must be a pure module to be unit-tested.

## Execution order

1. `src/components/quake/quakeSlide.ts` + `quakeSlide.test.ts` (Red first).
2. `src/components/terminal/terminalService.ts`: add `isActiveShell(sessionId, shellId)`.
3. `src/styles/globals.css`: `.quake-overlay`, `.quake-hidden`, slide-time transition freeze. `quake-shown` is a phase marker with no CSS rule.
4. `src/components/quake/QuakeTerminal.tsx`: always-mounted render, phase state, single registration, focus release.

## Per-section plan

- **quakeSlide.ts**: `type SlidePhase = "hidden" | "entering" | "shown" | "exiting"`; `nextSlidePhase(phase, event)` with events `{ type: "open" }`, `{ type: "close", sessionSwitch: boolean }`, `{ type: "animationEnd" }`; `initialSlidePhase(open)`; `slideClass(phase)` maps to `quake-hidden` / `quake-enter` / `quake-shown` / `quake-exit`. `SLIDE_MS` lives here.
- **terminalService.ts**: `isActiveShell` returns true when `activeIds.get("quake")` is the shell key and the entry exists.
- **QuakeTerminal.tsx**: phase via `useState(initialSlidePhase(open))`; a render-phase update (React's "adjust state on prop change" pattern, previous `{ open, sessionId }` kept in state) dispatches open/close, with `sessionSwitch` when the session id changed; `onAnimationEnd` (filtered to the overlay itself) dispatches `animationEnd`; a fallback timer at `2 * SLIDE_MS` while `entering`/`exiting`. Host + ResizeObserver effects with `[]` deps. Show effect gated on `open` and skipped when `isActiveShell`. Blur `document.activeElement` when `open` turns false and the overlay contains it. Render always; the new-shell and close-shell handlers no-op without an active session. When `open` turns false and the zone is still `quake`, reset `focusZoneAtom` to `terminal`. The `isActiveShell` skip still refocuses the quake when the zone is `quake`.

## Per-phase risk

- Phase 1: low (pure code, tests).
- Phase 3: the sibling selector must match the real DOM order → verified at `Workspace.tsx:357-359`.
- Phase 4: the host must exist before any `showShell` → host effect is declared before the show effect, both run after the first commit; `showTerminal` returns early on a null host (`terminalService.ts:280-281`), so order matters. `animationend` bubbles from children (`transition-colors` does not emit it, but the shell-strip buttons have none) → filter on `e.target === e.currentTarget`.
