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
