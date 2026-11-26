# Evidence — kitty-graphics-canvas (FRI-613)

## Plan (2026-11-26)

- Analysis: `~/.claude/projects/-home-felipe-Projects-cluihud/work/kitty-graphics/analysis.md` (wezterm kitty security holes: `t=f`/`t=t`/`t=s` read/delete files, uncapped compression and chunks, `w=0`+`c` division by zero with release `panic = "abort"`).
- User decisions: kitty before FRI-624; protection in Nergal, not a fork; `a=q` unanswered until FRI-624; memory invariant (convergence after iprev r1+r2): PTY output can never grow Nergal memory without bound through kitty → per-terminal lifetime caps; caps (after r4): 256 MiB, 131,072 cells, 1024 placements, 4096 transmissions, 4096 deletes, with a visible notice, deletes pass after the other caps.
- iprev (opus, critical, until APPROVED): r1 REVISE (16, 5 critical: byte filter cannot match vtparse) → typed-action gate with Nergal owning the termwiz parser and `Terminal::perform_actions`; r2 REVISE (gate cannot model wezterm memory) → lifetime caps; r3 REVISE (wezterm image cache returns other dimensions → div by zero) → origin-only placements, count caps, 4 KiB minimum charge; r4 REVISE (placements × cells CPU, real capacity) → user picked low caps + notice, snapshot run index, kill switch; r5 APPROVED (6 medium/low applied). Reviews in `handoff/iprev/`.
- Side issue: FRI-629 (vtparse APC buffer unbounded, pre-existing).

## Baseline (2026-11-26)

Main `23470f5` = FRI-623 merge; its local gates passed at `8c61d7a` (cargo test 1445, vitest 760, clippy/fmt/tsc clean). CI on main is red only for the intermittent FRI-628 test.

## Red (2026-11-26)

- Test-writers (sonnet, separate from the Builders): Rust `kitty_gate_tests.rs` (39 tests) + `session_kitty_tests.rs` (17, one `#[ignore]` timed); frontend `imageLayers.test.ts` (5) + `kittyCapped.test.ts` (5, pure helper `kittyCappedNotice`, `KITTY_CAPPED_MESSAGE`).
- Red proof: `cargo test --no-run` fails only on unresolved `KittyGate`/`GateLimits` (E0432, plus 4 E0277 that follow from it) and `TerminalSession::set_kitty_limits` (E0599); against a pass-through stub gate 37/39 gate tests fail (the 2 that pass guard against over-blocking). `pnpm test` fails only on the two missing modules; 760 other tests green.
- Locks: `handoff/.red-tests`, 4 files, hashes re-verified by the orchestrator with `git hash-object`.
- Open in 2.1: the 2-second worst-stacking bound is measured after Green (no implementation existed to measure); if it misses, report the time, never loosen it. An in-flight transfer dropped at the instant a cap trips is not reachable through the public API (any action that trips a cap also interrupts the transfer); covered by the chunked-over-bytes-cap test.

## UI brief

- Scope: no new visual surface. `paintRow` now paints row clear, under-background images, cell backgrounds, under-text images, glyphs/underlines, over-text images (kitty's three z layers); sixel/iTerm2 runs (z = 0) still draw over text.
- Layering lives in the pure `imageLayers` helper (`src/components/terminal/imageLayers.ts`), one stable ascending-z sort per layer; `drawImageRuns` takes the ordered list.
- Cap notice: `terminal:kitty-capped` is listened in `src/stores/hooks.ts`; `kittyCappedNotice` dedupes per terminal and fires one existing `info` Sileo toast with neutral Spanish copy. No shortcuts, tokens or focus zones touched.

## Revision 1 Red (2026-11-26)

- Rust Builder found wezterm placement-id doubling (`vecstorage.rs:19-30` + `image.rs:190-216`; 18 stacked placements 10 s, >5 GB) and a wrong locked assertion (`CursorPosition.seqno` in the parity test). User ruled: strip placement ids in the gate. Artifacts: design Revision 1 (`2ede33d`).
- Test-writer revised the two Rust locked files: placements, `a=T` and `d=i`/`d=I` deletes forwarded with `placement_id: None`; 5 string-terminator residue tests; fuzz predicate rejects any placement id; parity compares cursor x/y/shape/visibility; worst stacking uses 1024 distinct images; new `placements_with_placement_ids_stay_linear`, `deleting_with_a_placement_id_removes_the_image`, and the seeded property `held_image_cells_never_exceed_the_forwarded_placed_cells_under_random_traffic` (user invariant made executable). Clippy needless-borrow fixed in the test, so no allow is needed.
- Red against the uncommitted Builder code: `cargo test --lib terminal::` 207 passed, 9 failed (all on placement ids or held cells past forwarded), 1 ignored, about 18 s, no hang.
- Locks re-verified: `kitty_gate_tests.rs` 87a21af2, `session_kitty_tests.rs` bffa677c.

## Revision 2 (2026-11-26)

- Green after Revision 1: clippy clean, `cargo test` 1512 passed / 2 ignored, rustfmt check OK, locks OK. Timed worst stacking in release: `took 9.226013234s` (bound 2 s) — wezterm `assign_image_to_cells` clones each covered cell's image list, O(N²): N=128 376 ms, 256 1.39 s, 512 5.59 s; Nergal snapshot linear (82/136/282 ms).
- User: stack depth cap of 8 (recommended), no more hunting of extreme edge cases. Artifacts `59b207a`.
- Red 2.3: `a_ninth_stacked_image_pauses_kitty_images`, `eight_images_stacked_on_each_of_two_cells_are_all_drawn_without_a_notice`, worst stacking expects 8 runs per row. Red = `E0432 unresolved import KITTY_MAX_STACK_DEPTH` only. Lock `session_kitty_tests.rs` 5b7ea528.

## Green (2026-11-26)

- Builders (sonnet): Rust `kitty_gate.rs`, `session.rs` (parser ownership, multimap run index, stack depth check `perform_capping_stacks`, reads lines one by one because wezterm `Screen::with_phys_lines` panics on a wrapped ring buffer), `pty.rs` (`terminal:kitty-capped` emit), `config.rs` (`enable_kitty_graphics` true), `clippy.toml`; frontend `imageLayers.ts`, `kittyCapped.ts`, `paintRow` three layers, `hooks.ts` toast listener.
- Orchestrator gates: `cargo clippy --all-targets -- -D warnings` → No issues found; `cargo test` → 1514 passed, 2 ignored; rustfmt check on changed files → OK; `npx tsc --noEmit` → no errors; `pnpm test` → 770 passed.
- Timed worst stacking (release, `--ignored`): passed (suite 0.01 s; was 9.226 s before Revision 2).
- Clippy guard: a temporary `t.advance_bytes(b"x")` in non-test code failed clippy with `use of a disallowed method 'wezterm_term::Terminal::advance_bytes'`; removed (Builder run).
- work-gates: deps SKIP, placeholders PASS, secrets PASS, Red locks PASS (4). 6b WARN justified: `#[allow(clippy::disallowed_methods)]` is the planned parity test that must call the emulator's own parser (design D6); `.skip(8)` is an iterator, not a skipped test.

## Review-fix Red (2026-11-26)

- Reviews: adversarial FAIL (2 Important: chafa GIF stacking → accepted risk by user, FRI-612 comment; main spec contradiction → MODIFIED requirement; 2 Nit: docs drift fixed, chafa fixture test), frontend PASS (4 Nit), security FAIL (1 Important: stack check clipped to visible width, bypass with `ESC[1;999H`).
- Red: `a_cursor_past_the_last_column_still_caps_the_stack_at_eight_images`, `chafa_kitty_output_with_empty_header_and_closing_chunks_draws_and_writes_nothing` (real chafa 1.8 bytes; exposed that the gate rejected non-zero trailing base64 bits that wezterm and chafa accept → artifacts and the base64 gate test aligned to the emulator rules), English toast text (app UI copy is English). Red output: 2 session tests fail, 1 gate test fails, 1 vitest fails.
- Frontend Nit 3 (purge `kittyNotified`) not fixed: one short key per capped terminal for the app run, the backend emits once; purging needs a terminal-close hook for no measurable gain.
- Locks: kitty_gate_tests 27c4d7dc, session_kitty_tests 09eb765a, kittyCapped.test b9219cf1, imageLayers.test 8fcdb6af.

## Review-fix Green (2026-11-26)

- Fixes: stack check no longer clips columns to the visible width (security #1); gate decodes base64 with the emulator rules, trailing bits allowed (chafa fixture); toast text in English; `kittyCappedNotice` returns `prev` when already notified; `imageLayers` returns a shared frozen empty result for rows without images.
- Gates: clippy No issues found; cargo test 1516 passed, 2 ignored; rustfmt check OK; tsc no errors; vitest 770 passed; work-gates deps SKIP, placeholders PASS, secrets PASS, Red locks PASS (4), 6b WARN same two justified lines as in Green.

## Walk 1 (2026-11-26)

- K8 PASS (`kittyGraphics=no`). K9 cap behaves as designed (logo drawn at 48x24 = 1152 cells, so about 113 images fit; the first loop of 80 fits, the second trips the toast); walk defect: the whole sentence in the sileo title rendered title-cased and badly on first paint → regression test (kittyCapped.test.ts b13abf48) + split into title "Kitty images paused" and description "Reopen this terminal to show them again." (app toast convention). tsc clean, vitest 770 passed.
- K11 (chafa GIF): grey block with a "1", cursor blinks until Ctrl+C; user: no patch here, recorded on FRI-612.
