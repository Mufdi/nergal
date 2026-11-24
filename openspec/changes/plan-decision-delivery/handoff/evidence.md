# Evidence: plan-decision-delivery (FRI-604)

## Diagnosis (2026-11-23)

- User walk: Approve with `A` and with the mouse both ended the panel review, but Claude Code kept asking (focus was in the plan panel).
- XD001 transcript `18e18c76`: ExitPlanMode `toolu_01Fo3C5j56H69mMkKQD9EXik` at 20:57:34Z, hook deny "Plan review timed out — please resubmit" at 21:27:35Z (exact 30-minute backstop). A later Revise from the panel (21:31:32Z) did reach the hook.
- Root cause: two `plan:ready` emitters (`server.rs:1128` without `decision_path`, `:1277` with it); `PreToolUse` hook is `async: true` and the server spawns a task per connection, so the preview can land after the review and wipe `decisionPath`; `handleApprove` then goes `idle` silently (`PlanPanel.tsx:115-120`). Secondary: `std::fs::write` to a dead FIFO creates a regular file and reports success.
- Nergal log of that hour not kept (only the current run, no debug level).

## Planning

- iprev (opus): R1 REVISE (2 high, 4 medium, 4 low), R2 REVISE (3 medium, 4 low), R3 APPROVED (6 low, folded into design/implementation/tasks).

## Baseline

- Branch `mufdidev/fri-604-plan-approve-key` from main `1ad4c7a`, whose full verify ran green minutes before: `cargo test` 1240 passed, 0 failed; `pnpm test` 514 passed; clippy, fmt, tsc clean.

## Red (test-writer, sonnet)

- New: `src-tauri/src/plan_fifo_tests.rs` (7 helper cases), `src-tauri/src/agents/claude_code/adapter_plan_fifo_tests.rs` (4 claim cases incl. a 50x two-thread race), `src/stores/planReady.test.ts` (5 `applyPlanReady` + 5 `decisionGuard`). Locked in `handoff/.red-tests`.
- Scaffolding by the test-writer so the Red compiles against the crate: `src-tauri/src/plan_fifo.rs` holds only the `#[path]` test include; `#[cfg(unix)] mod plan_fifo;` in `lib.rs`; `#[path = "adapter_plan_fifo_tests.rs"]` include in `adapter.rs`.
- Inline test edits in `adapter.rs` `mod tests` (not hash-lockable; the Builder must not touch them): `submit_plan_decision_writes_to_registered_fifo` rewritten with a real FIFO and a reader opened first; new `submit_plan_decision_to_regular_file_is_io_error`.
- Red output: `cargo test --lib plan_fifo` fails to compile (E0432 missing `write_plan_decision`, E0599 missing `claim_pending_plan_fifo`); vitest fails on the missing `@/stores/planReady` module, the other 514 tests pass.

## Green (Builder)

- `plan_fifo.rs`: `write_plan_decision` (O_WRONLY|O_NONBLOCK|O_NOFOLLOW|O_CLOEXEC, ENXIO retry to deadline, fstat FIFO check, poll(POLLOUT) write continuing from offset, OwnedFd). `ClaudeCodeAdapter::claim_pending_plan_fifo` via `remove_if`; adapter `submit_plan_decision` writes through the helper in `spawn_blocking` (Unix). Command `submit_plan_decision` is `async`, claims then `spawn_blocking` 5 s (Windows branch unchanged). `PlanReview` handler registers only FIFOs whose parent is `ipc_dir()`, emits `decision_path: ""` plus warn otherwise, warns on unresolved plan.
- Frontend: `src/stores/planReady.ts` (`applyPlanReady`, `decisionGuard`), used in the `plan:ready` listener; `PlanPanel` guards with `e.repeat` (forwarded in the custom event `detail`), in-flight ref, single `failDecision` path (toast + idle), failed Revise keeps annotations.
- Deviations: none. Note: in the listener the document entry is computed from `docs[path] ?? session plan` as previous plan, status from the session result.
- Verify: clippy `--all-targets -D warnings` clean; `cargo test`: 1250 passed, 0 failed (lib); rustfmt --edition 2024 --check on the 4 edited .rs files OK; `npx tsc --noEmit` clean; `pnpm test`: 524 passed.

## Review-time fix (orchestrator)

- Found in the diff: the listener based the document entry on `docs[payload.path]` even when that document was from an earlier cycle, so a preview on another path dropped the live decision path for that document. Test-writer added 4 `documentBase` cases (Red: TypeError, function missing), relocked; `documentBase` added to `src/stores/planReady.ts` and used in `src/stores/hooks.ts`.

## Gates (after Green + fix)

- `pnpm test`: 52 files, 528 passed. `tsc --noEmit`: clean.
- `cargo clippy --all-targets -D warnings`: no issues. `cargo test`: 1252 passed, 0 failed. `cargo fmt --check`: clean.

| Gate | Result | Detail |
|------|--------|--------|
| 5. Deps | SKIP | no manifest or lockfile changed |
| 6a. Placeholders | PASS | none |
| 6b. Bar weakening | PASS | none |
| 6c. Secrets | PASS | gitleaks clean |
| 2. Red locks | PASS | 3 locked file(s) unchanged |

## Revision 1 Red (test-writer)

- Added: 4 Rust cases in `plan_fifo_tests.rs` (fast fail with a 5 s deadline, 3 `fifo_in_ipc_dir`), new `src-tauri/src/platform/plan_fifo_path_tests.rs` (nonce path), vitest `isLiveReview` block and 2 `undelivered` cases. Include line appended to `platform/mod.rs`.
- Red: Rust lib tests fail to compile (E0432 `fifo_in_ipc_dir`); vitest 8 failed in `planReady.test.ts` (missing `isLiveReview`), 530 passed. Relocked (4 files).

## Revision 1 Green (Builder)

Done (3.2, 3.3, 3.4):
- `src-tauri/src/plan_fifo.rs`: ENXIO retry capped at min(deadline, 1 s); `pub fn fifo_in_ipc_dir`.
- `src-tauri/src/hooks/server.rs` PlanReview (Unix): uses `fifo_in_ipc_dir` with `ipc_dir().ok()`; an `ipc_dir()` error logs its own message, "outside the IPC dir" only when the dir resolved.
- `src-tauri/src/platform/mod.rs`: Unix FIFO name `plan-{pid}-{nonce}.fifo` (`Uuid::new_v4().simple()`); Windows and other branch unchanged. Grep for `plan-{pid}`/`plan-` found no other parser (only cli.rs:226 caller).
- Frontend: `undelivered` status, `planDecisionInFlightAtom`, `isLiveReview` (planReady.ts), PlanPanel (canAnnotate via isLiveReview, header label, disabled/aria-busy buttons, atom in-flight guard read via `appStore` at decision time, `failDecision(action, err)`, empty Revise fails visibly, listeners read handlers through a ref, effect deps reduced to `[canAnnotate]`). `PlanAnnotationFooter` prop type widened to `PlanReviewStatus`.
- Other readers checked: `canEnterAnnotationModeAtom`, Workspace.tsx:136/:284, SessionIndicator compare only to `pending_review`, so `undelivered` behaves like `idle`; `stop` event still resets to `idle`.
- docs/hooks.md plan review section updated.

Deviations: the `nergal:revise-plan` listener still skips when there are zero annotations (read from the ref); the empty-feedback `failDecision("Revise")` fires only if serialization is empty despite annotations. The key handler already blocks R with no annotations.

Verify tails:
- clippy --all-targets -D warnings: No issues found
- cargo test: `test result: ok. 1255 passed; 0 failed; 1 ignored` (+ 2 other suites ok)
- rustfmt --edition 2024 --check plan_fifo.rs server.rs platform/mod.rs: clean
- npx tsc --noEmit: clean; pnpm test: 52 files, 538 tests passed

## Gates (after Revision 1, orchestrator run)

- `cargo clippy --all-targets -D warnings`: no issues. `cargo test`: 1257 passed, 0 failed. `cargo fmt --check`: clean. `tsc --noEmit`: clean. `pnpm test`: 52 files, 538 passed.

| Gate | Result | Detail |
|------|--------|--------|
| 5. Deps | SKIP | no manifest or lockfile changed |
| 6a. Placeholders | PASS | none |
| 6b. Bar weakening | PASS | none |
| 6c. Secrets | PASS | gitleaks clean |
| 2. Red locks | PASS | 4 locked file(s) unchanged |

## Revision 2 Red (test-writer)

- Adversarial re-review (in REVIEW.md) found 2 Important (failed Revise hid its highlights and disabled X; `undelivered` spec wording) and 5 nits; the user had ruled to fix every finding.
- Added: vitest `isReviewDocument` (5), `showReviewAnnotations` (5), `decisionGuard` status cases; Rust `review_decision_path` (3). Red: vitest 13 failed in `planReady.test.ts`; Rust E0432 `review_decision_path`. Relocked.

## Revision 2 Green (Builder)

Implemented:
- `src/stores/planReady.ts`: `isReviewDocument`, `showReviewAnnotations`, `isLiveReview` built on `isReviewDocument`, `status` in `decisionGuard` (ignores anything but `pending_review`).
- `src/components/plan/PlanPanel.tsx`: decisions, global comment and annotation-mode toggle (listener plus auto-exit) gated by `isLiveReview` / review-document check; rendered annotations, the X key and `nergal:clear-annotations` use `showReviewAnnotations`. The approve/revise listeners, the clear listener and the keydown handler are separate gates (A/C/R need `canAnnotate`, X needs `showAnnotations`). `guardDecision` reads the current status from `appStore.get(planReviewStatusMapAtom)[sessionId]`.
- `src-tauri/src/plan_fifo.rs`: `review_decision_path`, ENXIO budget 900 ms. `hooks/server.rs` PlanReview applies it (Some registers and emits, None emits empty path), keeping the separate warn for an ipc_dir error vs outside-dir.
- `docs/hooks.md`: FIFO name, `undelivered` causes and lifetime.

Deviations: `AnnotatableMarkdownView` unchanged. Creation already requires `annotationMode` (lines ~254, 309, 340, 386), and `annotationMode` is forced off outside a live review, so `annotationsEnabled` alone only restores/renders highlights. SpecPanel passes both props true, unaffected.

Verify:
- `cargo clippy --all-targets -- -D warnings`: Finished, no warnings.
- `cargo test`: `test result: ok. 1258 passed; 0 failed; 1 ignored` (plus 1 and 1 doc/bin ok).
- `rustfmt --edition 2024 --check plan_fifo.rs hooks/server.rs`: clean.
- `tsc --noEmit`: no errors. `pnpm test`: 52 files, 551 tests passed.

## Gates (after Revision 2, orchestrator run)

- `cargo clippy --all-targets -D warnings`: no issues. `cargo test`: 1260 passed, 0 failed. `cargo fmt --check`: clean. `tsc --noEmit`: clean. `pnpm test`: 551 passed.
- work-gates: deps SKIP, placeholders PASS, bar weakening PASS, gitleaks clean, 4 Red locks unchanged.
- Comment audit: every added comment states a WHY (registry claim, handler ref, ENXIO budget, `original` vs `content`, `documentBase`).
- Walks 4.3-4.7 wait for the checkpoint-1 `.deb` (after FRI-604, 506, 507, 473).

## Revision 3 (walk findings, 2026-11-24)

- Root cause of Approve never reaching Claude Code: the hook printed a bare `allow`; the current hook reference requires `updatedInput` on `allow` for `ExitPlanMode` ("returning allow alone is not sufficient"). Evidence: walk transcript `~/.claude/projects/-home-felipe-Projects-XD001/18e18c76-0e4e-4831-9830-f4e83cbe2040.jsonl` 02:40-03:07 UTC: every Revise has `hook_permission_decision: deny`, no approval has a hook decision; the installed binary equals the checkpoint-1 deb (`307da3b`).
- Locks are `git hash-object` blob ids (what `work-gates.sh` checks), not `sha1sum`. The Revision 3 test-writer wrote `sha1sum` lines; all six lines were recomputed with `git hash-object`; the two older files are unchanged since `d52f64a`.
- Red (test-writer): new `src-tauri/src/hooks/cli_plan_review_output_tests.rs` (6), appended adapter (3), setup (1), vitest `applyReviewClosed` (5) + `answered` (3). Rust Red = E0432/E0599/E0308 on the missing items; vitest 6 failed / 39 passed in `planReady.test.ts`. Relocked.
- Builder: `cli.rs` (`plan_review_output`, `Released`, silent no-session path), `server.rs` (release in `process_event` for top-level `PostToolUse[ExitPlanMode]`, `Stop`, `SessionEnd`; replaced `PlanReview` FIFO; `plan:review-closed`), `adapter.rs` (`register` returns the replaced path, `release_pending_plan_fifo`), `setup.rs` (matcher + `ExitPlanMode`), frontend `answered` state, `docs/hooks.md`. Deviation accepted: release lives in `process_event` because the `Stop` arm only matches `transcript_path: Some`.
- Main-thread doc fix: two inaccurate `docs/hooks.md` sentences (why `updatedInput` is needed; what CC does on release).

| Gate | Result | Detail |
|------|--------|--------|
| 5. Deps | SKIP | no manifest or lockfile changed |
| 6a. Placeholders | PASS | none |
| 6b. Bar weakening | PASS | none |
| 6c. Secrets | PASS | gitleaks clean |
| 2. Red locks | PASS | 6 locked file(s) unchanged |

- clippy --all-targets -D warnings: clean. cargo test: `test result: ok. 1268 passed; 0 failed; 1 ignored` (+ other binaries ok). rustfmt --edition 2024 --check (4 files): OK. tsc: clean. vitest: 559 passed (52 files).

## Revision 4 (Revision 3 re-review, 2026-11-24)

- Spike (isolated `claude --model haiku`, `--settings` with a test `PermissionRequest[Write]` hook, no `NERGAL_*` env): the hook stayed alive 6 s after the user answered Claude Code's own dialog, with no signal; the tool ran. A fresh session had no transcript file on disk at hook start nor after Esc. User decision: accept the terminal deny/Esc gap (`interview_answers.terminal_deny_or_esc_gap`).
- Red (test-writer): `server_review_close_tests.rs` (9 `closes_review` + unix `release_plan_review` FIFO), `cli_plan_review_rev4_tests.rs` (12), `adapter_plan_owner_tests.rs` (4), `src/stores/planReview.test.ts`. Rust Red = E0432/E0599 on the missing items.
- Red-conflict ruling: the locked Revision 3 vitest asserts `applyReviewClosed("submitted")` toEqual `{status, toast}` exactly, while the new file expected a `restore` key. Ruling: `applyReviewClosed` returns no `restore`; the restore decision is a separate pure helper `shouldRestoreAnnotations(prev, next)`. Only the new file was changed and relocked.
- Builder (Revision 4): `cli.rs`, `server.rs`, `events.rs`, `adapter.rs`, `commands/plans.rs`, `planReady.ts`, `hooks.ts`, `annotations.ts`, `PlanPanel.tsx`, `docs/hooks.md`. Accepted deviations: the stash lives in `annotations.ts` (import cycle); a restored stash is in memory only (the Revise already cleared the DB rows); the adapter-trait `submit_plan_decision` keeps a bare approve (no production caller).
- Found in the Builder diff: `plan:review-closed` was emitted only when a registration existed, so a delivered Revise followed by a terminal approval never reached the panel (spec scenario "A Revise that lost the race"); and a nested `PlanReview` still opened a live review. Test-writer added `server_review_emit_tests.rs` (10, Red E0432); main thread added `emits_review_closed` + `review_decision_for` and wired them. Removed one WHAT doc comment (`approved_plan_override`).

| Gate | Result | Detail |
|------|--------|--------|
| 5. Deps | SKIP | no manifest or lockfile changed |
| 6a. Placeholders | PASS | none |
| 6b. Bar weakening | PASS | none |
| 6c. Secrets | PASS | gitleaks clean |
| 2. Red locks | PASS | 11 locked file(s) unchanged |

- clippy clean; cargo test `ok. 1304 passed; 0 failed; 1 ignored`; tsc clean; vitest 588 passed (53 files).

## Revision 4 reverted (2026-11-24, user decision)

- Verification of `c247d3a` (REVIEW.md "Verification Revision 4"): 9 resolved, 4 partial, 1 not resolved, 8 new; root limit: `top_level` is only known on `SessionStart`/`Stop`. Third round of new cases in the release mechanism; user chose to revert to Revision 3 and document the rest as known situations (design.md "Revision 4 (reverted)", `docs/hooks.md`). `git revert --no-commit c247d3a`, keeping REVIEW.md and evidence.md history.
- After the revert: clippy clean; cargo test `ok. 1268 passed; 0 failed; 1 ignored`; rustfmt OK; gates all PASS (6 locks); tsc clean; vitest 559 passed.
