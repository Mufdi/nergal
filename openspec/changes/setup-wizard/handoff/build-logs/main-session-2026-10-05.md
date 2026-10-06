# Build log — setup-wizard — 2026-10-05 (main session, branch backlog-2026-10)

## B0 re-validation
- Anchors drifted (commands split into modules, AskUserModal gone, Settings CATEGORIES); scope cut per user to a launch-time hooks dialog; re-triaged L -> M.
- iprev: round 1 REVISE (10 findings accepted), round 2 APPROVED (5 folded in).

## Group 1+3 Red (test-writer, sonnet)
- Tests: src-tauri/src/setup_status_tests.rs (12), src/lib/hooksSetup.test.ts (5).
- Red evidence: `cargo test setup` -> 15 errors, all E0425 on the missing target symbols (apply_setup x11, hooks_status_at x2, run_at x2). vitest -> import failure on the missing `@/lib/hooksSetup` module.
- TDD note: for new Rust/TS APIs, Red is a compile/import failure on exactly the target symbols. Most apply_setup tests are characterization tests for the refactor; the new-behavior red proof is below.
- Locked in handoff/.red-tests.

## Group 1+2 Green (backend builder, sonnet)
- setup.rs: apply_setup + SetupChanges, run_at, D7 non-array error, parse_settings shared, hooks_status(_at) side-effect free, HooksStatus with binary_on_path (APPIMAGE guard + which).
- Commands: setup_hooks_status added; both commands map errors with format!("{e:#}"); registered in lib.rs.
- Red proof D3: with hooks_status_at calling load_settings -> `panicked at src/setup_status_tests.rs:135:5: assertion failed: !dir.path().join("missing").exists()`; reverted -> ok.
- Locked test file was not rustfmt-clean (2 rewrap hunks). Main session ran rustfmt on it, proved the diff whitespace-only (`diff` of the whitespace-stripped files: identical), re-locked: 0a85f908763230edf5d4401e753c96a88b360466.

## Group 3+4 Green (frontend builder, sonnet)
- hooksSetup.ts, HooksStatus type, HooksSetupDialog.tsx (ProjectBootstrapPrompt layout + ConfirmHost keyboard, own-ref focus, Enter always consumed), mounted after WorktreeGate in App.tsx. ui-brief.md written.

## Verify (main session)
- Lock check: both paths in .red-tests match `git hash-object`.
- `cargo clippy --all-targets -- -D warnings` -> "No issues found"
- `cargo test` -> `test result: ok. 829 passed; 0 failed; 1 ignored`
- `cargo fmt --check` -> clean
- `npx tsc --noEmit` -> no errors
- `pnpm test` -> `Test Files 15 passed (15)`, `Tests 111 passed (111)`
- Gate 7 placeholders -> 0 hits. Comment audit -> 1 added comment (WHY, D7), 1 moved pre-existing doc comment.

## Reviews
- Frontend reviewer + adversarial reviewer → see handoff/REVIEW.md. 4 Important fixed via Revision 1 (design.md).

## Revision 1 (post-review)
- Red (test-writer): 6 tests appended. Evidence: `error[E0425]: cannot find function binary_usable`; with those removed, `apply_setup_leaves_null_event_untouched_when_no_nergal_hook_targets_it ... FAILED` (`hooks.MyEvent is not an array`). run_at malformed test = characterization (passes).
- Lock note: the previous lock used `git hash-object` without `-w`, so the append-only claim was verified by test names (12 originals present, 18 total) instead of bytes. Blobs are now stored (`-w`).
- Green (builder): D7 scoped, `binary_usable` + cfg(windows) `binary_on_path`, dialog keyboard/copy/a11y fixes. Main session gated `binary_usable` with `cfg(any(not(windows), test))` to avoid a Windows dead_code warning.
- Docs: docs/hooks.md + README.md now point to `nergal hook setup` and the launch dialog (main session).
- Verify: locks ok; clippy "No issues found"; `cargo test` → `ok. 835 passed; 0 failed; 1 ignored`; fmt clean; tsc clean; vitest `111 passed`; placeholder scan 0 hits.
