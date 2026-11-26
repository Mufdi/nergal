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
