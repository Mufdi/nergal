# Evidence: local-branches-chip

## Baseline

Skipped: the base commit `0079b75` (main) is the merge of PR #3, whose CI ran 6/6 green. Work branch: `feat/local-branches-chip`.

## Planning

- iprev R1 (opus): REVISE, 11 findings. R2 (sonnet; the opus weekly limit was hit and codex auth had expired): APPROVED. Full log: `handoff/iprev-review.md`.
- `openspec validate local-branches-chip`: valid.

## Red (all groups)

- Frontend (groups 4–5): `src/stores/zenMode.test.ts` (3), `src/lib/localBranches.test.ts` (9), `src/stores/sessionActivation.test.ts` (4). The run `rtk proxy pnpm vitest run <3 files>` fails for the expected reasons:
  - zenMode: `expected undefined to be null` and `expected undefined to deeply equal {hash…}`, because the `commit` field is missing;
  - the other two: `Cannot find module`.
- Backend (groups 1–3): `worktree_local_branches_tests.rs` (43), `db/branch_owners_tests.rs` (2), `commands/branches_tests.rs` (31). Red is compile-level: the APIs and module attachments do not exist yet. All three pass `rustfmt --check`.
- Ruling (orchestrator, before locking): the test-writer had asserted that `open_branch_session_core` does not insert the session row, and the two `finish_branch_session` tests relied on it. That contradicts implementation.md, where the core runs D4 steps 1–7, including the insert. Changes:
  - the test became `opening_a_session_inserts_its_row`;
  - the finish tests now build the worktree with `add_worktree_for_branch` and an unsaved `Session`.
- All six files are locked in `handoff/.red-tests`.

## UI brief

- Product type: dense developer-tool list inside an existing GitPanel chip strip; style follows `docs/design.md` (dark, 10-11px type, mono for refs, orange cursor bar), no new palette or fonts. Baseline kept low-variance and low-motion: no animation beyond the refresh spinner.
- Architecture: the chip is a pure consumer of `localBranchesCacheAtom` (plain atom keyed by workspace id, filled by GitPanel on mount and by the chip's own reload). Post-action reload runs in `finally`, so success and failure both resync. Pure logic (labels, confirm body, toast shape) lives in `lib/localBranches.ts` so it is unit-tested without a DOM.
- Mechanics copied from `HistoryChip` (cursor state, window keydown gated by `listenerActive`, `event.code`, inputs and modified keys ignored) and the loading/empty patterns (`ProgressBar`, never raw spinners for regions).
- UX laws applied: targets are real `<button>`s (reachable by Tab, revealed on cursor row, hover and focus-within); one destructive action goes through `confirm` with the loss stated and the restore command given afterwards; feedback via toasts; tolerant of missing data ("?" pending, "no session" marker); max two row actions.
- Anti-patterns avoided: no emojis, no new global shortcut, no hardcoded colors outside the existing orange/red/yellow status palette.

## Green: groups A, B and C (2026-11-18)

Builders: A backend (sonnet), B commit diffs (sonnet), C Branches chip (sonnet). B and C ran after A's command shapes were locked; C after B.

Rulings during Green:
- **Locked Rust fixtures used an invalid git date** (`"500 +0000"`; git 2.34.1 needs `@500 +0000`). Every fixture-based test panicked at setup, so these tests had never run (Red was compile-level). The orchestrator fixed the stamp in `worktree_local_branches_tests.rs` and `commands/branches_tests.rs` and re-locked both. In the same pass it removed the unused fixture helper `add_worktree_existing`, so the Builder's `#[allow(dead_code)]` on the module attachment is gone.
- **Real defect found by the locked merge test** (`commit_file_diff_of_a_merge_is_the_first_parent_diff`): on git 2.34.1, `diff-tree -m --first-parent <merge>` still emits the diff against the second parent. Fixed in `worktree/local_branches.rs`: `commit_trees` passes `<id>^1 <id>` explicitly, and `--root <id>` for a root commit. iprev R2 had flagged this risk.
- Builder A deviations accepted:
  - `symlink_metadata` errors other than `NotFound` count as "in use" (stricter, fail-closed);
  - the session id prefix is built from `chars().take(6)`;
  - `build_new_session` is now `pub(crate)`;
  - refusal wording is the Builder's own, and the locked tests pin only the key substrings.
- Builder B deviations accepted: `fetchDiff` deps include `commit?.hash`/`commit?.scope` (the scope object comes from the Zen atom, so its reference is stable); `files` falls back to `[filePath]`.
- Builder C deviations accepted: Space on a focused button inside the chip does not toggle the row; `addAndActivateSessionAtom` writes the expanded set only when the workspace is missing from it.

### Gates

| Gate | Result | Detail |
|------|--------|--------|
| 1. Compile/Type + build | PASS | `cargo clippy --all-targets -- -D warnings`: `Finished`. `npx tsc --noEmit`: no errors. `pnpm build`: `✓ built in 7.89s` |
| 2. Tests | PASS | `cargo test`: `1131 passed; 0 failed; 1 ignored`. `pnpm vitest run`: `Test Files 32 passed (32)`, `Tests 297 passed (297)` |
| 2. Red locks | PASS | 6 locked files unchanged (`work-gates.sh`) |
| 3. Lint/format | PASS | `rustfmt --edition 2024 --check` on every changed or new `.rs`: OK |
| 4. Security (insecure-defaults) | PASS | No env, config or secrets in the diff. The error paths fail closed: no data dir → record fails → branch kept; git failure → "in use"; invalid name → refused before git |
| 5. Deps | SKIP | no manifest changed |
| 6a. Placeholders | PASS | none |
| 6b. Bar weakening | WARN, justified | 3 `eslint-disable-next-line react-hooks/exhaustive-deps` in `BranchesChip.tsx`, the same keydown-listener pattern as `HistoryChip.tsx:98`. The listener deliberately re-binds only on cursor or list changes |
| 6c. Secrets | PASS | gitleaks clean |

Comment audit: every added comment states a WHY (lock order, refname tag ambiguity, the hash-like branch name, first-parent on git 2.34, the short-name attach, the plain-atom keying).

## Review fixes (B5, 2026-11-18)

Reviewers: adversarial, security and frontend, all `sonnet`, because the opus weekly limit was reached. Consolidated in `handoff/REVIEW.md`. The verdicts:
- Security: PASS.
- Frontend: FAIL, on one 🔴 (a stuck loader).
- Adversarial: 4 Important bugs, plus design and doc items.

Regression tests (test-writer, then locked):
- `worktree_local_branches_review_tests.rs`: commit files with non-ASCII and spaced names; the non-ASCII diff.
- `commands/branches_review_tests.rs`: a stale SHA leaves the missing-worktree registration intact; a locked missing worktree keeps the branch; an unwritable record names its path.
- Red confirmed on two of them: the quoted `café.txt`, and the registration cleared on a stale SHA. The other three were regression guards and already passed.
- Ruling: the stale-SHA test first used a "wrong" SHA equal to the branch's own SHA, since `nergal/y` was created from `main` with no commit. The orchestrator added one commit to the branch plus an `assert_ne!`, then re-locked. The orchestrator also removed an unused `use super::*` that would have failed clippy.

Fixes:
- Backend (orchestrator, small):
  - `commit_files` uses `-z` and splits on NUL (A2/S3);
  - "missing worktree" also needs git's `prunable` flag (A3);
  - `delete_local_branch_core` checks the branch state and SHA before it clears stale registrations, so a refused delete leaves the repo unchanged (A4).
- Frontend and docs (Builder, sonnet):
  - error state with Retry and a toast (F1);
  - reload on every mount, plus a shared in-flight loader `src/stores/localBranchesLoader.ts` with a `fresh` follow-up (A1/F8);
  - Enter opens a session and `D` deletes, as in `StashesChip` (F2);
  - focus rings (F3/F4); row-specific `aria-label`s (F5); header split count (F6); the "unpushed" wording;
  - all actions disabled while one runs; a focusable "worktree missing" badge; an empty state;
  - the stale comment in `git.ts` (A8); Branches added to `docs/architecture.md` and `docs/patterns.md` (A7).

### Gates after fixes

| Gate | Result | Detail |
|------|--------|--------|
| 1. Compile/Type + build | PASS | clippy `--all-targets -D warnings`: `Finished`. `tsc --noEmit`: no errors. `pnpm build`: `✓ built in 7.25s` |
| 2. Tests | PASS | `cargo test`: `1136 passed; 0 failed; 1 ignored`. vitest: `32 files, 297 tests passed` |
| 2. Red locks | PASS | 8 locked files unchanged |
| 3. Format | PASS | `rustfmt --edition 2024 --check` on changed and new `.rs` |
| 6a/6c | PASS | no placeholders; gitleaks clean |
| 6b | WARN, justified | 2 `eslint-disable-next-line react-hooks/exhaustive-deps`, the same keydown-listener pattern as `HistoryChip` |

## Revision 1: walk feedback (2026-11-18)

- User walk on the first `.deb` reported: no keyboard path to commit files; sidebar arrows also moved the Branches cursor; Enter opened a session at once; the chip did not refresh live; the git panel opened slowly.
- Root causes:
  - Chip listeners on `window` never checked `focusZoneAtom` (all git chips and the chip strip).
  - Nine git panel read commands were sync, so Tauri 2 ran them on the main thread, where the Linux webview draws. `gh pr list` measured 0.83 s and 0.89 s in XD001.
- Red: `src/lib/branchTree.test.ts` (41) and `src/lib/gitChipKeys.test.ts` (9), written by a separate test-writer, locked in `.red-tests`. They failed on missing modules only.
- Green gates:
  - `cargo clippy --all-targets -- -D warnings`: no issues.
  - `cargo test`: 1136 passed, 1 ignored.
  - `rustfmt --edition 2024 --check` on the 5 changed command files: OK.
  - `npx tsc --noEmit`: clean. `pnpm test`: 347 passed.
  - `work-gates.sh main local-branches-chip`: 10 locks unchanged, gitleaks clean. 6b WARN: one `eslint-disable react-hooks/exhaustive-deps` in `BranchesChip` (the key listener re-binds on `items`, `cursor`, `listenerActive`, `busyBranch`; same pattern as `HistoryChip`).
- Data repair, Nergal closed: the user deleted the session "test" by mistake. Restored row `e30eb7-1794952210` from `nergal.db.bak-2026-11-18-test-relink`, linked to conversation `18e18c76` (last entry 21:56Z). Backup before the write: `nergal.db.bak-2026-11-18-test-restore`.
- Walks passed, as the user reported ("todo perfecto"): tasks 6.4–6.11 on the first `.deb`, and R1–R6 (tasks 7.8–7.12 plus the "test" restore) on the 19:26 `.deb`.

## Revision 2: History keyboard and push freeze (2026-11-18)

- Red: `src/lib/historyTree.test.ts` (15), written by a separate test-writer, locked. It failed on the missing module only.
- `git_commit`, `git_push`, `git_ship`, `get_pr_preview_data` now `async` with `blocking()`.
- Gates: clippy `--all-targets -D warnings` clean; `cargo test` 1136 passed, 1 ignored; `rustfmt --check` OK; `tsc` clean; `pnpm test` 361 passed; `work-gates.sh`: 11 locks unchanged, gitleaks clean. 6b WARN: the `HistoryChip` key-listener `eslint-disable react-hooks/exhaustive-deps` line, the same justification as `BranchesChip`.
- Walks 8.6 and 8.7 (H1, H2, P1) passed on the 20:10 `.deb`, as the user reported ("todas las pruebas done").
