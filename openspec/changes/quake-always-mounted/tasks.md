## 1. Slide phase state machine

- [x] 1.1 Test (Red): `src/components/quake/quakeSlide.test.ts` covers "Slide phases on open and close", "Session switch snaps without a slide", reopen during exit, initial phase, and the phase-to-class map
- [x] 1.2 Implement `src/components/quake/quakeSlide.ts` (`SlidePhase`, `nextSlidePhase`, `initialSlidePhase`, `slideClass`, `SLIDE_MS`, `isSliding`)

## 2. Always-mounted overlay

- [x] 2.1 Add `terminalService.isActiveShell(sessionId, shellId)`
- [x] 2.2 `globals.css`: pinned layer (`.quake-overlay { will-change: transform }`), `.quake-hidden` (translated + `visibility: hidden`), and the slide-time `transition: none` on `.nergal-panel-focus` for the overlay and its following siblings
- [x] 2.3 `QuakeTerminal.tsx`: render always; phase state driven by `nextSlidePhase`; `onAnimationEnd` + 2× fallback timer; host and ResizeObserver registered once; show effect skips the already-active shell; blur focus inside the overlay and reset a stale `quake` zone when it closes
- [x] 2.4 Remove the obsolete "Known limit" and `rendered` comments; keep WHY-only comments

## 3. Verification

- [x] 3.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit && pnpm test`
- [ ] 3.2 Manual walk (glow on): open and close with `Ctrl+}` several times; the slide runs smooth in both directions
- [ ] 3.3 Manual walk: close over an open spec panel with live refresh; the exit slide plays
- [ ] 3.4 Manual walk: print output in a shell, scroll up, close, reopen; rows and scroll position are the same
- [ ] 3.5 Manual walk: session switch with the quake open in A and closed in B; no exit slide over B, and typing goes to B's agent terminal
- [ ] 3.6 Manual walk: new shell tab, `exit` in a shell, `Ctrl+W` in the quake, resize drag; no regressions
