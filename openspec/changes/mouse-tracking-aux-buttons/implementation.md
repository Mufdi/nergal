# Implementation Plan: mouse-tracking-aux-buttons

> Grounded in current codebase, symbols verified 2026-11-23.

## Verified codebase facts (do not re-assume)

- wezterm-term (git tag `20240203-110809-5046fc22`, `src-tauri/Cargo.toml:83`) exposes `Terminal::is_mouse_grabbed()` = `mouse_tracking || button_event_mouse || any_event_mouse` (`term/src/terminalstate/mod.rs:748-750` in the cargo git checkout), set by DECSET 1000 / 1002 / 1003 (`mod.rs:1754-1793`) and cleared on reset (`performer.rs:681-686`).
- `GridSnapshot` (`src-tauri/src/terminal/types.rs:45-53`) and `GridUpdate` (`types.rs:64-75`, `serde(rename_all = "camelCase")`) both carry `is_alt_screen`. `TerminalSession::snapshot` fills it at `src-tauri/src/terminal/session.rs:547-556`.
- `GridDiffer` (`src-tauri/src/terminal/differ.rs`) emits only when the first emission is pending, rows changed, the cursor changed or the title changed (`differ.rs:61-80`); it keeps `last_cursor` and `last_title`. Its tests build `GridSnapshot` literals (`differ.rs:~112-125`); other literals live in `types.rs` users and `session.rs`.
- Frontend: `GridUpdate` in `src/lib/types.ts:143-152`; the terminal entry keeps `isAltScreen` (`src/components/terminal/terminalService.ts:87`, initial `:441`, update `:1318`). `auxButtonToForward(button, isAltScreen)` (`src/components/terminal/mouseButtons.ts:10-15`) is called at `terminalService.ts:742`; the `contextmenu` listener suppresses on `entry.isAltScreen` (`terminalService.ts:864-866`). Other `isAltScreen` uses (left drag `:786`, `:814`, `:852`, selection `:962`, wheel in `src-tauri/src/pty.rs:2213`) are out of scope.
- Existing vitest for the helper: `src/components/terminal/mouseButtons.test.ts`.
- wezterm-term's `mouse_button_press` / `mouse_button_release` emit only when `is_mouse_grabbed()` (`term/src/terminalstate/mouse.rs:144`, `:195`), independent of the screen. RIS clears the modes (`performer.rs:681-687`); DECSTR does not.
- `mouseToCell` (`src/components/terminal/terminalService.ts:1020-1028`) returns the viewport row; `TerminalSession::mouse_button` (`src-tauri/src/terminal/session.rs:438-446`) passes it as the live-screen `y`. On the primary screen with `scrollOffset > 0` (wheel → `scroll_by`, `src-tauri/src/pty.rs:2213-2226`) those rows differ, hence the `scrollOffset === 0` condition. `scrollOffset` already arrives in every update (`terminalService.ts:1314`).
- Emitter test pattern: `attached_channel_receives_session_updates` in `src-tauri/src/terminal/emitter.rs` (or its `#[path]` test files); `terminal_attach` calls `invalidate()` on the differ (`src-tauri/src/pty.rs:2392-2395`).
- Synchronized output (DECSET 2026) holds emission up to `SYNC_OUTPUT_CAP` (`src-tauri/src/terminal/emitter.rs:131-142`).

## Execution order

1. Rust: `mouse_tracking: bool` on `GridSnapshot` and `GridUpdate`; `snapshot()` sets it from `is_mouse_grabbed()`; `GridDiffer` stores `last_mouse_tracking`, emits when it changed, and updates it in the same emitting branch as `last_cursor` / `last_title` (`differ.rs:64-66`); `invalidate()` forces the next emission, which carries the current value. Fix every `GridSnapshot` literal.
2. Frontend: `mouseTracking: boolean` on `GridUpdate`; entry field `mouseTracking` (initial `false`, set from each update in `applyUpdate`); a pure `reportsMouse(mouseTracking, scrollOffset)` (true only with tracking and offset 0) in `mouseButtons.ts`; `auxButtonToForward(button, reports)` and the `contextmenu` listener both use it.

## Per-phase risk

- An update that arrives before the first snapshot reaches the entry leaves `mouseTracking` false, so the context menu shows: the safe default.
- Synchronized output can hold a mode change up to `SYNC_OUTPUT_CAP`; a right click in that window shows the context menu once. Accepted.
- A held aux button while the view scrolls into history: the release still goes out (keyed off `entry.auxHeld` through `auxReduce`, `terminalService.ts:150`), at the pointer's cell, or at the press cell when the pointer is off the canvas (`terminalService.ts:153`), so wezterm-term gets a balanced press/release.
- Releasing a held aux button after tracking turned off: `auxRelease` keys off `entry.auxHeld`, not the mode, so the release still goes out and wezterm-term drops it when no mode is on.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, `npx tsc --noEmit`, `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh main mouse-tracking-aux-buttons`.
- Manual walk on the checkpoint-1 `.deb` (tasks group 3).

## Revision 1 plan (review follow-ups)

- `src/components/terminal/mouseButtons.ts`: `reportsMouse(mouseTracking, scrollOffset, shiftKey)`; `auxReduce(held, event)` with events `press` (AuxHeld), `release` (button, pointer cell or null), `blur`, returning `{ held, send }` where `send` is `{ kind: "press" | "release", aux, col, row }` or null; it replaces `auxPress` / `auxRelease`. `auxButtonToForward(button, reports)` keeps its name; rename the second parameter to `reportsMouseEvents`.
- `src/components/terminal/terminalService.ts`: mousedown (`:742-752`), window mouseup (`:148-156`), window blur (`:157-159`) and `sendFocus(false)` (`:626`) go through `auxReduce`; `contextmenu` uses `reportsMouse(..., e.shiftKey)` with a WHY comment for the Shift exception; fix the indentation at `:1321`.
- `src-tauri/src/terminal/session.rs` `mouse_button` (`:408-445`): return `Ok(())` without forwarding when the alternate screen is not active and `scroll_offset > 0`; fix its doc comment (`?1006h` is an encoding; list 1000/1002/1003). `src-tauri/src/terminal/types.rs:63-64`: drop the stale "Phase 2 emits 0" sentence.
- `docs/patterns.md`: a short terminal-mouse note (mouse-tracking apps get middle/right; Shift bypasses; scrolled-back views never report).

## Revision 2 plan (re-review follow-ups)

- `contextmenu` listener moves from `entry.canvas` to `entry.container` (`terminalService.ts:865`): the keyboard-opened menu fires on the focused textarea, a sibling of the canvas (`:387`), and only the container sees both.
- `releasePointerCell(cell, scrollOffset)` in `mouseButtons.ts`: the pointer cell at the live bottom, `null` when scrolled back, so `auxReduce` falls back to the press cell instead of a history row; used by the window mouseup (`terminalService.ts:150`).
- `docs/patterns.md` terminal-mouse note: a scrolled-back view sends no press; a release of a button pressed at the bottom still goes out.
- `src-tauri/src/terminal/session.rs:392-402`: move the wheel doc block onto `mouse_wheel` (`:459`).
