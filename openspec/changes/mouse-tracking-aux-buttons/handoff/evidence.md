# Evidence: mouse-tracking-aux-buttons (FRI-506)

## Planning

- iprev (opus): R1 REVISE (1 high: a scrolled-back primary screen would report viewport rows as live rows; 1 medium; 6 low), R2 APPROVED (4 low, folded).

## Baseline

- Branch `mufdidev/fri-506-mouse-tracking-clicks`, stacked on `mufdidev/fri-604-plan-approve-key` (`d52f64a`), whose gates ran green: `cargo test` 1260 passed, `pnpm test` 551 passed, clippy/fmt/tsc clean.

## Red (test-writer)

- New: `terminal/mouse_tracking_tests.rs` (session, DECSET 1000/1002/1003 on/off), `terminal/differ_mouse_tracking_tests.rs` (flip, unchanged, h-l-h, invalidate), `terminal/emitter_mouse_tracking_tests.rs` (channel JSON has `"mouseTracking":true`); `mouseButtons.test.ts` gains `reportsMouse` cases. Includes appended to session.rs, differ.rs, emitter.rs. Locked.
- Red: Rust 11 compile errors (E0560/E0609 `mouse_tracking`); vitest 4 failed (`reportsMouse` missing).
- Note: the snapshot method is `grid_snapshot()`; the inline differ test literal in `differ.rs` `mod tests` needs `mouse_tracking: false` (Builder edit, mechanical).

## Green (Builder)

- Rust: `mouse_tracking` on `GridSnapshot`/`GridUpdate` (types.rs); `grid_snapshot()` fills it from `is_mouse_grabbed()`; `GridDiffer` keeps `last_mouse_tracking: Option<bool>` (None after `invalidate()`/new), emits on change, updates it in the same branch as cursor/title. Inline differ test helper got `mouse_tracking: false`.
- Frontend: `mouseTracking` on `GridUpdate` and the terminal entry (set next to `isAltScreen`); `reportsMouse` in mouseButtons.ts; `auxButtonToForward(button, reports)`; mousedown and `contextmenu` use `reportsMouse(entry.mouseTracking, entry.scrollOffset)`; contextmenu comment reworded (no longer says TUI/alt screen).
- Deviations: none. Locked tests untouched.
- Verify: clippy --all-targets -D warnings clean; `cargo test` 1267 passed, 0 failed; `rustfmt --edition 2024 --check` on differ/session/types OK; `tsc --noEmit` no errors; `pnpm test` 52 files, 555 passed.

## Revision 1 Red (test-writer)

- `mouseButtons.test.ts` rewritten (3-arg `reportsMouse`, 7 `auxReduce` cases); new `terminal/mouse_scroll_gate_tests.rs` (scrolled-back right press writes nothing; live bottom writes a report). Red: vitest 8 failed; Rust 1 failed (scrolled-back case), 1 passed (live bottom, already true). Relocked (5 files).

## Revision 1 Green (Builder)

- cargo clippy --all-targets -- -D warnings: clean.
- cargo test: 1269 passed, 0 failed (locked mouse tracking/scroll-gate tests included).
- rustfmt --edition 2024 --check session.rs types.rs: ok.
- npx tsc --noEmit: clean. pnpm test: 52 files, 558 tests passed.
- Implemented: reportsMouse(shift), auxReduce (auxPress/auxRelease removed), terminalService routing (mousedown, window mouseup/blur, sendFocus(false), contextmenu with Shift), session.rs scrolled-back gate + doc fix, types.rs doc fix, docs/patterns.md section 15.

## Review-time fix (orchestrator)

- The Builder's backend gate dropped releases too, so a press at the live bottom released after a wheel scroll stayed held in the app. Test-writer added `release_of_a_button_pressed_at_the_bottom_is_reported_after_scrolling_into_history` (Red: fails on the empty capture), relocked; the gate in `session.rs` `mouse_button` now applies to presses only.

## Gates (after Revision 1 + press-only gate, orchestrator run)

- `cargo clippy --all-targets -D warnings`: no issues. `cargo test`: 1272 passed, 0 failed. `cargo fmt --check`: clean. `tsc --noEmit`: clean. `pnpm test`: 558 passed.
- work-gates (base `mufdidev/fri-604-plan-approve-key`): deps SKIP, placeholders PASS, bar weakening PASS, gitleaks clean, 5 Red locks unchanged.

## Revision 2 Red (test-writer)

- Re-review (REVIEW.md) found 1 Important (keyboard contextmenu never reaches the canvas listener) and 5 nits; all folded into Revision 2.
- vitest `releasePointerCell` (3, Red: missing export); Rust `release_with_no_held_button_is_not_reported_while_scrolled_into_history` (passes already: regression guard). Relocked.

## Revision 2 Green (Builder)

- `mouseButtons.ts`: added `releasePointerCell(cell, scrollOffset)`.
- `terminalService.ts`: window mouseup passes `releasePointerCell(mouseToCell(entry, e), entry.scrollOffset)`; `contextmenu` listener moved from `entry.canvas` to `entry.container` (same predicate), WHY comment extended.
- `docs/patterns.md` section 15: scrolled-back view sends no press; release of a live-bottom press still goes out.
- `session.rs`: wheel doc block moved onto `mouse_wheel`.
- `cargo clippy --all-targets -- -D warnings`: no issues.
- `cargo test`: `test result: ok. 1271 passed; 0 failed; 1 ignored`.
- `rustfmt --edition 2024 --check src/terminal/session.rs`: clean.
- `npx tsc --noEmit`: no errors. `pnpm test`: 52 files, 561 tests passed.

## Final gates (orchestrator)

- clippy clean; `cargo test` 1273 passed, 0 failed; fmt clean; tsc clean; `pnpm test` 561 passed; work-gates: 5 locks unchanged, gitleaks clean. Comment audit: WHY-only (doc blocks moved, not new).
- Walks 4.3-4.5 wait for the checkpoint-1 `.deb`.
