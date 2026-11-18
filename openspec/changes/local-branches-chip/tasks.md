## 1. Git layer for local branches

- [x] 1.1 Test (Red): in `src-tauri/src/worktree_local_branches_tests.rs`, using a real temp repo with a bare "remote":
  - `nergal_branch_candidates`:
    - returns `nergal/*` branches newest first;
    - skips `feature/x` and a symref `nergal/s → main`;
    - names a branch `nergal/t` exactly when a tag `nergal/t` exists.
  - `pending_commit_count` gives:
    - 2 for a branch with 2 local-only commits;
    - 0 for a pushed branch;
    - 0 for a branch merged into `Some("develop")` but not into `HEAD`;
    - `None` for target `Some("gone")`.
  - `pending_commits` returns only the commits not in the target, newest first, capped by `limit`.
  - `is_valid_commit_id` accepts `abc1` and a 40-char SHA, and rejects `abc`, a 65-char string, `--output=/tmp/x` and `abcg`.
  - `resolve_commit` returns the full id for a short prefix, and `Err` for `beef` when only a branch `beef` exists.
  - `nergal_slug` accepts `nergal/a-1` and `nergal/a.b_c`, and rejects `nergal/../x`, `nergal/a/b`, `nergal/.x`, `nergal/` and `feature/x`.
  - `commit_files` lists a commit's files joined to the base.
  - `commit_file_diff`:
    - shows the commit's added line and not a later uncommitted change;
    - gives full content for the root commit and the first-parent diff for a merge;
    - returns an empty diff for the path `:(glob)*.ts`;
    - returns `Err` for the id `--output=/tmp/x` or the path `../outside.txt`, and `/tmp/x` is not created.
  - `worktree_entries` parses branch, `locked` and `prunable`.
  - `branch_usage` gives:
    - `InUse` for a branch checked out in an existing worktree;
    - `InUse` for a locked missing worktree;
    - `InUse` for a branch in `rebase-merge/head-name`;
    - `MissingWorktree` for a branch whose worktree dir was `rm -rf`'d;
    - `Free` otherwise.
  - `clear_missing_worktrees` removes only the missing, unlocked entry that holds the branch, and keeps a missing entry of another branch and a locked one.
  - `add_worktree_for_branch`:
    - leaves `symbolic-ref HEAD` equal to `refs/heads/nergal/<slug>` at the branch's SHA, without moving the ref, also when a tag with the same name exists;
    - refuses an existing directory and a dangling symlink at the path;
    - fails when the branch is checked out elsewhere.
  - `drop_branch` with `Unconditional` and the right `expected_sha` deletes a local-only branch and writes one record line.
  - `drop_branch` with a stale `expected_sha` returns `Kept { Moved }` and leaves the branch and the record unchanged.
- [x] 1.2 Add `src-tauri/src/worktree/local_branches.rs` with the locked functions and attach its test file in `worktree.rs`.
- [x] 1.3 Extract `branch_mid_operation`, add `DropPolicy` and `drop_branch` in `worktree.rs`, and make `drop_session_branch` delegate to `drop_branch`. `worktree_branch_tests.rs` and `worktree_review_tests.rs` pass unchanged.

## 2. Branch ownership in the database

- [x] 2.1 Test (Red): `branch_owners("w")` returns the sessions of `w` with a branch, in creation order, with name and merge target. It excludes repo-root sessions and sessions of other workspaces.
- [x] 2.2 Add `BranchOwner` in `models.rs` and `Database::branch_owners` in `db/sessions.rs`.

## 3. Branch commands

- [x] 3.1 Test (Red): in `src-tauri/src/commands/branches_tests.rs`, with an in-memory DB and a temp repo:
  - `local_branch_rows`:
    - a local-only branch has `pending: Some(n)` and `session: None`;
    - an owned branch carries its session `{ id, name }`;
    - a branch checked out in a linked worktree has `in_use: true`;
    - a branch whose worktree dir was removed has `in_use: false` and `missing_worktree: Some(path)`;
    - a pushed branch is absent;
    - an owner with merge target `gone` gives `pending: None`;
    - a non-git directory gives an empty list.
  - `delete_local_branch_core`:
    - an invalid name gives `Kept` and runs no git;
    - an owned branch gives `Kept` naming the session, and the branch stays;
    - an unknown workspace gives `Err`;
    - a local-only branch gives `Deleted { sha, repo }`, the ref is gone and the record ends with the branch line;
    - a branch held by a removed worktree is deleted and its registration is gone;
    - a missing branch gives `Missing`;
    - a stale SHA gives `Kept` and the branch stays.
  - `open_branch_session_core`:
    - creates the worktree attached to the existing branch at its SHA, and a session named `<slug>` with that path and branch;
    - reopens a branch whose worktree dir was removed;
    - an owned branch, an existing directory, and a branch checked out elsewhere each give `Err` and create nothing.
  - `finish_branch_session` with an owner seeded before the call gives `Err` naming the owner and removes the worktree.
  - Locking:
    - `branch_repo_lock` returns the same mutex for a repo path and its symlinked alias;
    - with the repo lock held by the test, a delete on another thread does not finish;
    - the test thread can take the DB guard meanwhile;
    - after the release, the delete finishes.
  - `CommitScope` deserializes `{"kind":"session","sessionId":"s"}` and `{"kind":"workspace","workspaceId":"w"}`.
- [x] 3.2 Add `src-tauri/src/commands/branches.rs` with the cores, the lock and the six `async` commands.
  - Make `capabilities_for_agent_id`, `deleted_branches_record` and `kept_message` `pub(crate)`.
  - Take the repo lock in `notices_after_removal`.
  - Remove `get_commit_files`.
  - Register the new commands in `lib.rs`.

## 4. Commit diffs in Zen and the History fix

- [x] 4.1 Test (Red): `src/stores/zenMode.test.ts` (jotai `createStore`):
  - `openZenModeAtom` with `commit` stores the hash and the scope;
  - without `commit` it stores `null`;
  - `closeZenModeAtom` resets `commit` to `null`.
- [x] 4.2 Add `CommitScope` and `ZenModeState.commit` in `src/stores/zenMode.ts`.
- [x] 4.3 `ZenMode.tsx` passes `commit` to `DiffView`, keys it by commit and path, and shows the short hash in the header. `DiffView` calls `get_commit_file_diff` when `commit` is set and skips the `fileTimestamp` refetch.
- [x] 4.4 `HistoryChip` lists a commit's files with `get_scoped_commit_files` (session scope). It opens Zen with that commit's files only and `commit: { hash, scope: { kind: "session", sessionId } }`.

## 5. Branches chip

- [x] 5.1 Test (Red):
  - `src/lib/localBranches.test.ts`:
    - `branchActionsAllowed` is true only for `session === null && !in_use`;
    - `orphanBranchCount` counts the rows with no session;
    - `deleteBranchConfirm`:
      - escapes `nergal/<b>x` to `nergal/&lt;b&gt;x`;
      - contains the count (`?` when `pending` is null) and "exist only on this machine";
      - is `destructive`.
    - `branchDeleteToast`:
      - maps `deleted` with repo `/home/u/my repo` to "Branch deleted" with `git -C '/home/u/my repo' branch <b> <sha>` (info);
      - maps `missing` to "Branch already deleted" (info);
      - maps `kept` to "Branch kept" with the message (error).
  - `src/stores/sessionActivation.test.ts`: `addAndActivateSessionAtom`:
    - appends the session once;
    - expands its workspace, also from a `null` set;
    - marks it fresh and activates it.
- [x] 5.2 Add `src/lib/localBranches.ts` and `src/stores/sessionActivation.ts`. `Sidebar.spawnSession` uses the atom.
- [x] 5.3 In `src/stores/git.ts` and `ChipStrip`:
  - add `branches` to `ChipMode` and `CHIP_ORDER` (last);
  - add the `Branches` entry in `CHIP_META` (icon `GitBranch`);
  - add a `branchCount` prop that shows `orphanBranchCount`;
  - add a plain `localBranchesCacheAtom` keyed by workspace id.
- [x] 5.4 Add `src/components/git/chips/BranchesChip.tsx`:
  - rows show the name, the pending count, the relative date and subject, the session name or the "no session" marker, and the "in use" and "worktree missing" markers;
  - a refresh button;
  - ↑/↓ and j/k move the cursor, and Space expands a branch, then a commit;
  - commit files open Zen with workspace scope;
  - "Open session" and "Delete" appear only when `branchActionsAllowed`.
    - "Open session" calls `open_branch_session`, then `addAndActivateSessionAtom`, sets the focus zone to `terminal` and calls `terminalService.focusActive()`.
    - "Delete" calls `confirm(deleteBranchConfirm(row))`, then `delete_local_branch` with `row.sha`, then shows a toast from `branchDeleteToast`.
  - the list reloads after every action.
- [x] 5.5 `GitPanel` renders `BranchesChip` for `chipMode === "branches"` and passes the orphan count to `ChipStrip`. When it mounts and `localBranchesCacheAtom` has no entry for the workspace, it runs one `list_local_branches` to fill it.

## 6. Verification

- [x] 6.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && rustfmt --edition 2024 --check <changed .rs files>`
- [x] 6.2 `npx tsc --noEmit && pnpm test`
- [x] 6.3 `~/.claude/skills/work/scripts/work-gates.sh main local-branches-chip`
- [x] 6.4 Manual walk: in a worktree session, commit once without pushing, then delete the session; the branch is kept. In another session's git panel, the Branches chip lists the branch with 1 pending commit and "no session", and the chip count is 1.
- [x] 6.5 Manual walk: expand that branch and its commit, then open a file. Zen shows that commit's diff, and the header shows its short hash.
- [x] 6.6 Manual walk: open the History chip of a session with an uncommitted change to a file that an earlier commit touched. Opening the file from that commit shows the commit's diff, not the uncommitted change.
- [x] 6.7 Manual walk: run "Open session" on the branch.
  - A new session named after the slug opens on that branch, with the commit present.
  - A new commit in that session advances the branch: `git log nergal/<slug>` shows it.
  - The chip now shows the session name and no actions.
- [x] 6.8 Manual walk: delete that session again, then "Delete" the branch from the chip.
  - The confirmation names the branch and says its commits exist only on this machine.
  - After you confirm, the toast shows `git -C '<repo>' branch <name> <sha>` and the row disappears.
  - `~/.local/share/nergal/deleted-branches.log` has the line, and the restore command brings the branch back.
- [x] 6.9 Manual walk: remove a worktree session's directory with `rm -rf` while Nergal is closed, then reopen Nergal. The chip shows the branch with "no session" and "worktree missing", and "Open session" works.
- [x] 6.10 Manual walk: a branch owned by a live session shows its name and offers neither "Open session" nor "Delete".
- [x] 6.11 Manual walk: keyboard only in the Branches chip. j/k move the cursor, Space expands, Enter on a "no session" row opens a session (superseded by Revision 1: Enter expands, `O` opens; walked in 7.8–7.9), and D on a "no session" row opens the delete confirmation. The refresh button, commit rows and file rows show a focus ring when tabbed to.

## 7. Revision 1: walk feedback

- [x] 7.1 Test (Red): `src/lib/branchTree.test.ts` and `src/lib/gitChipKeys.test.ts` against the locked interfaces in `implementation.md` § Revision 1:
  - `branchTreeItems` lists branch, commit and file rows in display order with the locked keys, and shows no commits for a collapsed branch or a branch whose commits are not loaded.
  - `cursorIndexFor` finds a key, clamps the fallback, and returns 0 for an empty list.
  - `branchTreeKeyAction` covers Space/Enter on each row kind, `O`/`D` on an allowed branch, and `O`/`D` on a branch with a session, an in-use branch, a commit row and a file row (all `null`).
  - `changedBranches` returns removed and moved branches only.
  - `openSessionConfirm` escapes `nergal/<b>x`, is not destructive, and names a new Claude Code session.
  - `chipKeysActive` is true only for the panel zone outside Zen, and for the Zen sidebar zone inside Zen.
- [x] 7.2 Make the nine git panel read commands `async` with `spawn_blocking`, bodies unchanged.
- [x] 7.3 Add `src/lib/branchTree.ts` and `src/lib/gitChipKeys.ts`.
- [x] 7.4 `BranchesChip`: one keyed cursor over `branchTreeItems`, actions from `branchTreeKeyAction`, `O` behind `openSessionConfirm`, a 5 s poll and a reload on session-set change that keep the cursor and expansion, and footer hints for the new keys.
- [x] 7.5 Gate the chip strip and the Files, History, Stashes, PRs and Branches chip listeners with `chipKeysActive`.
- [x] 7.6 `GitPanel` reloads the branch badge when the workspace's session ids change.
- [x] 7.7 Gates: `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test`, `rustfmt --edition 2024 --check <changed .rs files>`, `npx tsc --noEmit && pnpm test`, `~/.claude/skills/work/scripts/work-gates.sh main local-branches-chip`.
- [x] 7.8 Manual walk: keyboard only, reach a file under a branch's commit and open it with Enter. Enter on a branch row only expands it.
- [x] 7.9 Manual walk: `O` on a "no session" row shows a confirmation; Cancel creates nothing; confirm opens the session.
- [x] 7.10 Manual walk: with the focus in the sidebar, ↑/↓ move only the sidebar selection; Shift+→ does not change the chip.
- [x] 7.11 Manual walk: with the Branches chip shown, delete a session that has an unpushed commit, then restore a deleted branch from a terminal. Both rows appear without a chip switch, within 5 s.
- [x] 7.12 Manual walk: the git panel opens without the window freezing, and the PR badge fills in afterwards.

## 8. Revision 2: History keyboard and push freeze

- [x] 8.1 Test (Red): `src/lib/historyTree.test.ts` against the locked interfaces in `implementation.md` § Revision 2: item order and keys, no files for a collapsed commit or unloaded files, Space/Enter on a commit, Enter on a file, Space on a file and other codes (`null`), and `cursorIndexFor` on history items.
- [x] 8.2 Make `git_commit`, `git_push`, `git_ship` and `get_pr_preview_data` `async` with `blocking()`, bodies unchanged.
- [x] 8.3 Add `src/lib/historyTree.ts` and widen `cursorIndexFor` to keyed items.
- [x] 8.4 `HistoryChip`: one keyed cursor over `historyTreeItems`, actions from `historyTreeKeyAction`, cursor highlight and focus ring on file rows, footer hint for Enter.
- [x] 8.5 Gates as in 7.7.
- [ ] 8.6 Manual walk: keyboard only in the History chip, reach a commit's file and open it with Enter, in the panel and in the Zen sidebar.
- [ ] 8.7 Manual walk: push from the git panel; the window keeps responding during the push.
