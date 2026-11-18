## 1. Branch cleanup on session delete

- [x] 1.1 Test (Red): with a real temp repo and a bare "remote" — `drop_session_branch`: pushed branch → `Deleted { sha }` and the ref is gone; branch merged into `merge_target` → `Deleted`; `merge_target = None` with commits in the repo root's `HEAD` → `Deleted`; detached repo-root `HEAD` containing the commits → `Deleted`; one local-only commit → `Kept` and the branch still exists; unknown `merge_target` → `Kept`; branch checked out in the repo root → `Kept` and the repo root's `HEAD` untouched; branch checked out in a linked worktree → `Kept`; `branch_checked_out` for `nergal/x` is false when only `nergal/x-2` is checked out; a branch being rebased in a worktree (write `rebase-merge/head-name`) → `Kept`; a branch being bisected (write `BISECT_START`) → `Kept`; a record path whose parent does not exist yet (`<tempdir>/missing/sub/deleted-branches.log`) → `Deleted` and the file created; a record path under a regular file (`<tempdir>/file/deleted-branches.log`) → `Kept { RecordFailed }` and the branch intact; a branch that is `Kept` for any other reason writes no record line; a CAS refused by a `reference-transaction` hook that exits 1 in the `prepared` state → `Kept { Moved }` and the branch intact; missing branch → `Missing`; `branch_state` on a non-repo path → `Unknown`; `Deleted` appends one line with repo, branch and SHA to the record file (tempdir) and never truncates earlier lines; `delete_branch_if_at` with a SHA the branch moved past → `false` and the branch intact; `notices_after_removal`: `Ok` + safe branch → one `branch_deleted` notice containing `git branch <name> <sha>` and branch gone, `Ok` + local-only branch → one `branch_kept`, reclaim failure → `Err` and branch intact, other failure → one `worktree_left` mentioning the kept branch and branch intact
- [x] 1.2 Add `branch_state`, `branch_is_safe_to_drop`, `branch_checked_out`, `delete_branch_if_at`, `record_deleted_branch`, `BranchOutcome`, `drop_session_branch` in `src-tauri/src/worktree.rs`
- [x] 1.3 Add `CleanupNotice`, `notices_after_removal`; `delete_session` uses them and returns typed notices

## 2. Pending delete

- [x] 2.1 Test (Red): `worktree_pending_delete` (dir without `.git` → true; missing dir → true; with a `.git` file → false); `refuse_launch` (pending worktree path → `Err`; healthy worktree → `Ok`; `None` worktree (repo-root session) → `Ok`; existing worktree with a `cwd` that does not exist → `Err`; `cwd = None` → `Ok`)
- [x] 2.2 Add `worktree_pending_delete`, `Session.worktree_pending_delete`, compute it in `get_workspaces` after releasing the guard
- [x] 2.3 Add `refuse_launch`; call it in `start_claude_session` before `reserve_session_pty` (missing row refused) and in `spawn_aux_shell` (gains `State<SharedDb>`)
- [x] 2.4 Frontend: pending panel in `TerminalManager` (no terminal, no env shells, manual remedy with path), marker in `SessionRow.tsx`, `Session` type field, `terminalService` catch removes the container and toasts once per session key
- [x] 2.5 Test (Red): `worktree_pending_delete` is true for a `.git` file whose `gitdir:` target is missing (walk C3) and false for a `.git` file whose target exists with `HEAD` and for a `.git` directory; the three locked "healthy" fixtures that used `gitdir: /x` move to a real target
- [x] 2.6 `worktree_pending_delete` checks the `gitdir:` target

- [x] 2.7 Test (Red): `pick_resume_id` follows `continued-in` (single hop, chain, missing continuation, cycle, invalid id)
- [x] 2.8 `pick_resume_id` follows `continued-in` from the stored id

## 3. One-shot fresh flag and notices

- [x] 3.1 Test (Red): `src/lib/launchMode.test.ts` — `launchAndConsume`: explicit mode wins and keeps the set; fresh → `new` and removes only that id; not fresh → `continue`; input never mutated; `noticeToast` maps the three kinds with their `type` (`error`, `info`, `info`); `src/stores/pendingDeletesNotices.test.ts` — a successful `delete_session` resolving `[{ kind: "branch_deleted", message: "… git branch nergal/x abc1234" }]` produces one info toast titled "Branch deleted"
- [x] 3.2 Add `src/lib/launchMode.ts`; `TerminalManager` writes `nextFresh` back after a launch
- [x] 3.3 Session `finalize` in `src/stores/pendingDeletes.ts` shows notices through `noticeToast`

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`
- [x] 4.2 `npx tsc --noEmit && pnpm test`
- [x] 4.3 Manual walk (starting without `~/.local/share/nergal`): a pushed branch is deleted with its session and `deleted-branches.log` gains its line; a local-only branch is kept and a "Branch kept" toast names it
- [x] 4.4 Manual walk: repeat B2; the kept session shows the pending marker and panel, starts no terminal, and the retry deletes it
- [x] 4.5 Manual walk: create a session, chat, close its tab, reopen it → it resumes its conversation
