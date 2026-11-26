# Evidence — osc7501-session-status (FRI-623)

## Plan (2026-11-26)

- Research: Claude Code 2.1.332 binary read for OSC 7501 detection and format (`~/.claude/projects/-home-felipe-Projects-cluihud/work/osc7501/research.md`); Nergal hook points mapped by an Explore agent (`nergal-side.md` in the same folder).
- User decisions: `done` and `error` get their own visuals; child entries out of scope; delivery invariant (after iprev r2, convergence rule): a wake note never lands inside a dialog or a running turn, and a queued message never stays queued while its target accepts a prompt.
- iprev (opus, `claude -p`): r1 REVISE (11 findings: messaging ignored the status, EOF not guarded by pty, synchronous reply under the session lock, set_at semantics, unscoped atom, SessionEnd divergence, scanner abort, test helpers, msg sanitizing, tint, walk gaps) → r2 REVISE (7: stale hook `running` stranded the trust-prompt drain, first-owner-wins hid new PTY reports, write queue carries keyboard input, Stop drain unguarded, vtparse claim, untestable drain, set_at on identical report) → r3 REVISE at the medium-risk cap (6: worktree outcomes in the ready drain and `notify_outcome`, untestable paused-time drain, racy key/reply order test, hook-side transitions, two residual races). All high/medium findings applied after each round; the two low r3 items are accepted risks in design D9. Reviews kept in `handoff/iprev/`.
- Commits: `7cd4747` plan, `7d7ea39` r1 fixes, `7b0c39a` r2 fixes, `c23f62c` r3 fixes.

## Baseline (2026-11-26)

- CI on main `ab472c2`: `Rust lint + test` FAILED on `log_owner_tests::children_do_not_inherit_the_lock` (`src-tauri/src/log_owner_tests.rs:147`), 1353 passed / 1 failed; macOS, Windows and frontend jobs green. Locally the test passes 3 of 3 runs (`cargo test --lib log_owner_tests`: 8 passed). Pre-existing and intermittent, not caused by this change: tracked as FRI-628.
- Local baseline on the branch (before Red): `cargo clippy --all-targets -- -D warnings` clean; `npx tsc --noEmit` clean; `pnpm test` 58 files / 711 tests passed. The `cargo test` summary was lost to the output filter, and the test-writer started editing right after; the Rust test baseline is the CI run on `ab472c2` above (1353 passed, 1 intermittent failure, FRI-628).
