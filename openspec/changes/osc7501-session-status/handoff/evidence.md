# Evidence — osc7501-session-status (FRI-623)

## Plan (2026-11-26)

- Research: Claude Code 2.1.332 binary read for OSC 7501 detection and format (`~/.claude/projects/-home-felipe-Projects-cluihud/work/osc7501/research.md`); Nergal hook points mapped by an Explore agent (`nergal-side.md` in the same folder).
- User decisions: `done` and `error` get their own visuals; child entries out of scope; delivery invariant (after iprev r2, convergence rule): a wake note never lands inside a dialog or a running turn, and a queued message never stays queued while its target accepts a prompt.
- iprev (opus, `claude -p`): r1 REVISE (11 findings: messaging ignored the status, EOF not guarded by pty, synchronous reply under the session lock, set_at semantics, unscoped atom, SessionEnd divergence, scanner abort, test helpers, msg sanitizing, tint, walk gaps) → r2 REVISE (7: stale hook `running` stranded the trust-prompt drain, first-owner-wins hid new PTY reports, write queue carries keyboard input, Stop drain unguarded, vtparse claim, untestable drain, set_at on identical report) → r3 REVISE at the medium-risk cap (6: worktree outcomes in the ready drain and `notify_outcome`, untestable paused-time drain, racy key/reply order test, hook-side transitions, two residual races). All high/medium findings applied after each round; the two low r3 items are accepted risks in design D9. Reviews kept in `handoff/iprev/`.
- Commits: `7cd4747` plan, `7d7ea39` r1 fixes, `7b0c39a` r2 fixes, `c23f62c` r3 fixes.

## Baseline (2026-11-26)

- CI on main `ab472c2`: `Rust lint + test` FAILED on `log_owner_tests::children_do_not_inherit_the_lock` (`src-tauri/src/log_owner_tests.rs:147`), 1353 passed / 1 failed; macOS, Windows and frontend jobs green. Locally the test passes 3 of 3 runs (`cargo test --lib log_owner_tests`: 8 passed). Pre-existing and intermittent, not caused by this change: tracked as FRI-628.
- Local baseline on the branch (before Red): `cargo clippy --all-targets -- -D warnings` clean; `npx tsc --noEmit` clean; `pnpm test` 58 files / 711 tests passed. The `cargo test` summary was lost to the output filter, and the test-writer started editing right after; the Rust test baseline is the CI run on `ab472c2` above (1353 passed, 1 intermittent failure, FRI-628).

## Red — groups 1-4 (Rust), 2026-11-26

Test-writer (sonnet): 69 tests in 7 test-only files, locked in `handoff/.red-tests` (all 7 hashes OK). 1.1 scanner/parser 19; 2.1 write queue 2 + session 8 + pty 1; 3.1 state 9 + `combined_mode` 12 + `status_payload` 3; 4.1 exhaustive 24-row `busy_rule` table + invariant over all 24 combinations, `delivery_busy` 2, send path 3, `ready_drain` 4 (`start_paused`), `notify_outcome` 4. Scaffolding: tokio `test-util` dev-dependency; `MockDelivery` moved to `mcp::delivery::test_support` (`pub(crate)`, cfg(test)); `#[path]` includes in the production files.

Red proof: `cargo test --no-run` fails only on the planned interface (88 errors: missing `ProgramStatusScanner`, `Osc7501`, `MAX_BODY`, `parse_report`, `ReportAction`, `ProgramStatus`, `RootState`, `PtyWriteQueue`, `OwnedStatus`, `StatusChange`, `apply_program_status`, `clear_program_status_if`, `program_status`, `combined_mode`, `set_status_reply`, `status_payload`, `busy_rule`, `delivery_busy`, `ready_drain`; `advance_bytes` must return `Vec<Vec<u8>>`; `build_terminal_session` must take the `SharedWriter`). The test-writer checked satisfiability against a throwaway reference implementation outside the repo (69 new + 1354 existing passed).

Gaps (no harness): the `hooks/server.rs` Stop-drain gate and the hook-side busy-to-not-busy drain need a real `AppHandle`; covered by `busy_rule`/`delivery_busy`/`ready_drain` plus walks 6.5/6.6. Quake-shell gate lives in the reader thread: walk 6.8. SessionEnd keeps the status: frontend test in 5.1.

Interface decisions: `ProgramStatusScanner::default()`; `build_terminal_session` takes the `SharedWriter` by value; `ready_drain` gate is `Option<Arc<WorktreeGateState>>`; test-only `AgentRuntimeState::set_activity_for_test` lives in the locked state test file (Builder must not add an inherent method with that name). Commit `4fe3f8e`.

## Red — group 5 (frontend), 2026-11-26

Test-writer (sonnet): `src/lib/sessionIndicatorState.test.ts` (every MODIFIED indicator scenario, hook attention over done/completed, `submitted` plan review raises no attention, `isAwaitingSession`) and `src/stores/programStatus.test.ts` (`applyProgramStatus` set/replace/msg-only change/same reference/removal/other sessions; `session:program-status` listener; `session_end` keeps the entry, via `setupHookListeners` with `@/lib/tauri` mocked as in `shortcutSignals.test.ts`). Locked (2 lines appended to `.red-tests`). Red: both fail with `Cannot find package '@/lib/sessionIndicatorState'` / `'@/stores/programStatus'`; the other 58 files (711 tests) pass. Interface: `applyProgramStatus(prevMap, payload)` returns `prevMap` for an equal entry; `resolveIndicatorState` args `mode: string`, `planReview?: string`, `pendingAsk: boolean`, `pendingAttention: boolean`, `root?: ProgramStatusEntry`.
