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

## UI brief

Applied frontend-build Steps 2-3c at small scale (two new states on an existing indicator, no new surface). Product type: desktop dev tool, dense session chrome. Style: reuse the existing status-dot vocabulary (docs/design.md §3.10), no new tokens or animations. `done` = static green-500 ring with transparent center (`ring-1 ring-inset ring-green-500 bg-transparent`), distinct from the filled green `working` dot by shape, not only hue; `error` = static filled `bg-red-500`. Neither animates, so reduced-motion needs no extra handling and the states still differ by color/shape. Anti-slop: no gradients, glows or new palette. UX laws: feedback on state change is immediate (atom update), consistent styling with the other dots, `aria-hidden` kept since the dot is decorative beside the session name. Platform: WebKitGTK webview, `ring-inset` box-shadow is supported. No keyboard or focus surface touched.

## Green — groups 1-5, 2026-11-26

Builders (sonnet): Rust groups 1-4 (`fef6c5d`), frontend group 5 (`497bc29`, plus `docs/design.md` §3.10 for the two new dots). Deviations (Rust): `schedule_ready_drain_for_app(app, sid)` resolves `SharedDb`/`AgentRuntimeState`/`WorktreeGateState`/`AppBridge` so `pty.rs` and `hooks/server.rs` share one call site; `forget_session` (session deletion only, not `SessionEnd`) also drops the `program_status` entry; an identical `Set` from the live PTY refreshes the stored `pty_id` without touching `status` or `set_at`. Frontend: none.

Verification (orchestrator, 2026-11-26):
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: `test result: ok. 1423 passed; 0 failed; 1 ignored` (lib) + 3 small suites ok.
- `rustfmt --edition 2024 --check` on the 18 changed `.rs` files: `FMT_OK`.
- `npx tsc --noEmit`: no errors. `pnpm test`: 60 files, 743 tests passed.
- `work-gates.sh main osc7501-session-status`: 6a PASS, 6b PASS, 6c PASS (gitleaks clean), Red locks PASS (9 files unchanged), 5 WARN (only the tokio `test-util` dev-dependency feature; no new crate).
- Comment audit: added comments are WHY (the rest were moved with `MockDelivery`).

## Review (B5), 2026-11-26

`handoff/REVIEW.md`: adversarial (opus) FAIL, 5 Important + 5 Nit; frontend (sonnet) approve with 2 Important (the red `error` dot is hard to tell from attention and working at 6px under reduced motion / red-green CVD) + 5 Nit, no blockers. All findings accepted; artifacts updated (`bf656d2`): reports count only while the PTY is armed (query answered; disarm on `SessionEnd` non-clear and EOF), the `Stop` drain skips only root `blocked`, settle window from both sources (`idle_since`), in-flight guard before worktree outcomes, unreachable hook trigger removed, atomic `entry` update, `error` as an `X` icon, status bar dot from the shared resolution, doc drift fixed. Task 4.1 reopened until its hook-server decisions have tests (group 7).

## Red — group 7 (review fixes), 2026-11-26

Test-writer (sonnet): 6 new test-only files, locked (15 lines in `.red-tests`, all OK): `agents/program_status_arming_tests.rs`, `hooks/events_review_tests.rs`, `terminal/session_query_seen_tests.rs`, `pty_review_tests.rs`, `mcp/messaging_review_tests.rs`, `src/lib/sessionIndicatorReview.test.ts`. Red: `cargo test --no-run` 29 errors, all missing planned items (`SessionEnd.reason`, `disarms_on_session_end`, `idle_since`, `ready_transition`, `reports_apply`, `stop_drain_allowed`, `delivery::is_in_flight`, `arm/disarm_program_status`, `program_status_armed`, `take_query_seen`); vitest 12 failed (new file, missing exports), 743 passed. Ruling (orchestrator): arming is checked by the reader-thread gate `reports_apply(agent_session, armed)`, not inside `apply_program_status`, so the group 3 locked tests stay valid (implementation.md updated). No React render harness exists (node env, no jsdom): `indicatorIcon`/`indicatorDotClass` are tested as pure helpers.

## Green — group 7 (review fixes), 2026-11-26

Builder (sonnet): every REVIEW.md finding fixed (arming with `take_query_seen` + `program_status_armed` + `SessionEnd.reason`/`disarms_on_session_end`; `stop_drain_allowed`; `idle_since` in the send path; `is_in_flight` before worktree outcomes in the `Stop` path and `ready_drain`; unreachable hook trigger removed; `agent_completed` via `ready_transition`; `apply_program_status` through the `DashMap` entry API; `error` as lucide `X`; status bar dot from `resolveIndicatorState` + `indicatorDotClass` (its idle dot is now `bg-muted-foreground/40`, the indicator's shade); exhaustive `RootState` switch; contiguous atoms; `docs/design.md` §3.10 and `docs/architecture.md` terminal modules). `_removed` kept: no eslint config in the repo.

Ruling (orchestrator): two locked test files from the group-7 test-writer were not rustfmt-clean (`cargo fmt --check` would fail in CI). Formatting only, no semantic change: formatted with `rustfmt --edition 2024` and re-locked (`hooks/events_review_tests.rs`, `mcp/messaging_review_tests.rs`). One comment reworded (`mcp/messaging.rs:49`, a background-agent wait can carry a root status).

Verification (orchestrator):
- `cargo clippy --all-targets -- -D warnings`: clean. `cargo test`: `1440 passed; 0 failed; 1 ignored`. `cargo fmt --check`: no diff.
- `npx tsc --noEmit`: clean. `pnpm test`: 61 files, 755 tests passed.
- `work-gates.sh`: 6a/6b/6c PASS, Red locks PASS (15), 5 WARN (tokio `test-util` dev feature only).
- Comment audit: the added comments are WHY.

## Re-review (round 2) and Red — group 8, 2026-11-26

Re-review (opus, appended to REVIEW.md): FAIL; 14 of 17 first-round findings resolved, 2 partial (status bar `error` still a dot), 1 left to the walk (`done` ring at `xs`); new: 1 Important (disarm on `SessionEnd` also ran for `/resume` and nested `claude -p`, both with Claude Code still alive, freezing the status and letting a wake paste into a running turn) + 4 Nit (non-atomic in-flight check, status bar `error` dot, `assertNever` throws in render, arming not limited to the live PTY) + 2 doc notes. All accepted; artifacts `5c373fb` (disarm only on a top-level `SessionEnd` whose reason is not `clear`/`resume`; reasons verified in the 2.1.332 binary: clear, resume, logout, prompt_input_exit, other).

Ruling: the locked `hooks/events_review_tests.rs` encoded the old disarm rule, so it was wrong against the updated spec; the test-writer rewrote it (11-case table over `disarms_on_session_end(top_level, reason)`) and re-locked. New locked files: `agents/program_status_rereview_tests.rs`, `mcp/outcome_inflight_tests.rs`, `src/lib/sessionIndicatorRereview.test.ts` (18 lines in `.red-tests`, all OK). Red: `cargo test --no-run` 1 error (`disarms_on_session_end` arity); with the old call, all 5 new Rust behavior tests fail for the expected reasons (late query arms A, non-live PTY armed, no in-flight mark during the outcome paste); vitest 5 failed (`assertNever` throws; `statusBarDot` missing), 755 passed.

## Green — group 8 (re-review fixes), 2026-11-26

Builder (sonnet): `disarms_on_session_end(top_level, reason)` and a live-PTY-only disarm in the `SessionEnd` handler (new `AgentRuntimeState::live_pty`); `arm_program_status` only for the live PTY; `drain_worktree_outcomes` holds the in-flight mark from before taking the notes until the paste fails (notes requeued) or its settle callback runs (the separate pre-check in `hooks/server.rs` became redundant and was removed; `mcp/messaging.rs` keeps its check); `statusBarDot` + lucide `X` for `error` in the status bar; compile-time-only exhaustiveness with a hook-mode fallback in `resolveIndicatorState`. Not changed: `notify_outcome`'s immediate wake does not take the in-flight mark (pre-existing path, outside the re-review finding). No third review: the fixes are small and each is covered by a locked test.

Verification (orchestrator): clippy clean; `cargo test` 1445 passed / 0 failed / 1 ignored; `cargo fmt --check` no diff; `tsc` clean; vitest 62 files / 760 tests; work-gates 6a/6b/6c PASS, Red locks PASS (18), 5 WARN (tokio dev feature only); added comments WHY.
