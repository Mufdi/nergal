## 1. Slide phase state machine

- [x] 1.1 Test (Red): `src/components/quake/quakeSlide.test.ts` covers "Slide phases on open and close", reopen during exit, initial phase, and the phase-to-class map
- [x] 1.2 Implement `src/components/quake/quakeSlide.ts` (`SlidePhase`, `nextSlidePhase`, `initialSlidePhase`, `slideClass`, `SLIDE_MS`, `isSliding`)

## 2. Always-mounted overlay

- [x] 2.1 Add `terminalService.isActiveShell(sessionId, shellId)`
- [x] 2.2 `globals.css`: pinned layer (`.quake-overlay { will-change: transform }`), `.quake-hidden` (translated + `visibility: hidden`), and the slide-time `transition: none` on `.nergal-panel-focus` for the overlay and its following siblings
- [x] 2.3 `QuakeTerminal.tsx`: render always; phase state driven by `nextSlidePhase`; `onAnimationEnd` + 2× fallback timer; host and ResizeObserver registered once; show effect skips the already-active shell; blur focus inside the overlay and reset a stale `quake` zone when it closes
- [x] 2.4 Remove the obsolete "Known limit" and `rendered` comments; keep WHY-only comments

## 3. Review revision

- [x] 3.1 Test (Red): `quakeSlide.test.ts` covers animation-name matching on `animationEnd` (mismatch keeps the phase, match completes, fallback completes)
- [x] 3.2 `quakeSlide.ts`: `animationEnd` carries `animationName`; `QuakeTerminal.tsx` passes `e.animationName`
- [x] 3.3 `globals.css`: `.quake-exit, .quake-hidden { pointer-events: none }`; overlay `onMouseDown` returns when `open` is false
- [x] 3.4 `terminalService.reportRegionUnfocused` called from the close effect in `QuakeTerminal.tsx`

## 4. Walk fix: session switch slides out

- [x] 4.1 Test (Red): `quakeSlide.test.ts` covers "Session switch closes with the exit slide" (every close from `entering`/`shown` goes to `exiting`; the close event carries no session-switch flag)
- [x] 4.2 `quakeSlide.ts`: drop `sessionSwitch` from the close event and its snap branch; `QuakeTerminal.tsx` tracks only the previous `open`

## 5. Verification

- [x] 5.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit && pnpm test`
- [x] 5.2 Manual walk (glow on): open and close with `Ctrl+}` several times; the slide runs smooth in both directions
- [x] 5.3 Manual walk: close over an open spec panel with live refresh; the exit slide plays
- [x] 5.4 Manual walk: print output in a shell, scroll up, close, reopen; rows and scroll position are the same
- [x] 5.5 Manual walk: session switch with the quake open in A and closed in B; the exit slide plays and typing goes to B's agent terminal
- [x] 5.6 Manual walk: new shell tab, `exit` in a shell, `Ctrl+W` in the quake, resize drag; no regressions
