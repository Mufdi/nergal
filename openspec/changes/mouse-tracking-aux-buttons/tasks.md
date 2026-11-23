## 1. Mouse tracking in the grid stream

- [x] 1.1 Test (Red): Rust — a `TerminalSession` fed `ESC[?1000h` reports `mouse_tracking: true` in its snapshot, likewise `ESC[?1002h` and `ESC[?1003h`, and `false` after the matching `l`; a `GridDiffer` that already emitted, fed a snapshot identical except `mouse_tracking` flipped, returns `Some` update with the new value, returns `None` when nothing changed, emits three times for `h → l → h` with no other change, and after `invalidate()` emits the current value. An attached emitter fed `ESC[?1000h` sends JSON containing `"mouseTracking":true` (pattern: `attached_channel_receives_session_updates` in `emitter.rs`). Use `#[path]` test files so they can be hash-locked.
- [x] 1.2 Add `mouse_tracking` to `GridSnapshot` and `GridUpdate`, fill it from `is_mouse_grabbed()` in `snapshot()`, and emit on change in `GridDiffer`.

## 2. Buttons follow mouse tracking

- [x] 2.1 Test (Red): vitest for `reportsMouse(mouseTracking, scrollOffset)`: true only for tracking on and offset 0; false for tracking off, and for tracking on with offset 30. `auxButtonToForward(button, reports)`: reports true → 1 → "middle", 2 → "right", 0 → null; reports false → null for 1 and 2.
- [x] 2.2 Add `mouseTracking` to the frontend `GridUpdate` and terminal entry, and use `reportsMouse(entry.mouseTracking, entry.scrollOffset)` for both `auxButtonToForward` and the `contextmenu` listener.

## 3. Review follow-ups (Revision 1, user: fix every finding)

- [x] 3.1 Test (Red): vitest `reportsMouse(tracking, offset, shift)` (shift true → false even with tracking and offset 0); `auxReduce`: press with nothing held → sends press; second press while held → no send; release of the held button with a pointer cell → sends release at that cell; with a null pointer cell → at the press cell; release of another button → no send; blur → held null, no send; release after blur → no send. Rust: a session with `ESC[?1000h` on the primary screen, scrolled back into history, gets `mouse_button(Right, Press, …)` and writes no bytes to the PTY; the same at the live bottom writes the SGR/X10 report.
- [x] 3.2 Implement Revision 1 (design.md D2-D4, implementation.md "Revision 1 plan"), including the comment and indentation fixes and the `docs/patterns.md` note.

- [x] 3.3 Test (Red): vitest `releasePointerCell(cell, scrollOffset)`: returns the cell at offset 0, null at offset 30, null when the cell is null. Rust: a press dropped while scrolled back, followed by its release (still scrolled back), writes nothing.
- [x] 3.4 Implement Revision 2 (implementation.md "Revision 2 plan").

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- [x] 4.2 `~/.claude/skills/work/scripts/work-gates.sh main mouse-tracking-aux-buttons` passes.
- [ ] 4.3 Manual walk: in a shell, run `printf '\e[?1000h\e[?1006h'; cat -v`; a right click prints an SGR report like `^[[<2;…M` and no context menu opens. Press Ctrl+C, run `printf '\e[?1000l'`, then right-click: no report, and the context menu opens.
- [ ] 4.4 Manual walk: open `less` on a long file (alternate screen, no mouse tracking); a right click opens the context menu. In `htop`, or `vim` with `:set mouse=a`, a right or middle click reaches the app.
- [ ] 4.5 Manual walk: in a Claude Code session (default renderer), a right click pastes the clipboard once on release and no context menu opens; a middle click reaches Claude Code (record what it does) and no context menu opens. Scroll the terminal up into history and right-click: nothing is sent to Claude Code and the context menu opens. Back at the bottom, Shift+right-click opens the context menu; with focus in the terminal, the Menu key opens no menu while Claude Code runs, and Shift+F10 opens it. (Left clicks keep their alternate-screen rule.)
