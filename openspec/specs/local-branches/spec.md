# local-branches Specification

## Purpose
TBD - created by archiving change local-branches-chip. Update Purpose after archive.

## Requirements

### Requirement: Branches chip lists local nergal branches with pending work

The Branches chip SHALL list the local branches under `refs/heads/nergal/` of the active session's workspace repository that have pending work. A branch has pending work when `git rev-list --count <branch-sha> --not --remotes <target-ref> --` is greater than 0. The target ref is `refs/heads/<merge_target>` of the session that uses the branch when it has one, and the repository root's `HEAD` otherwise. Symbolic refs SHALL NOT be listed. A branch whose check fails SHALL be listed with an unknown count. A workspace that is not a git repository SHALL get an empty list.

#### Scenario: Unpushed branch is listed

- **GIVEN** a workspace repository with `main` checked out at the root and a local branch `nergal/a-1` with 2 commits that are not on any remote and not in `main`
- **WHEN** the Branches chip loads
- **THEN** the list SHALL contain `nergal/a-1` with a pending count of 2

#### Scenario: Branch already on a remote is not listed

- **GIVEN** a local branch `nergal/b-2` whose commit is reachable from `refs/remotes/origin/nergal/b-2`
- **WHEN** the Branches chip loads
- **THEN** the list SHALL NOT contain `nergal/b-2`

#### Scenario: Branch merged into its session's merge target is not listed

- **GIVEN** a session that uses `nergal/c-3` with merge target `develop`, and every commit of `nergal/c-3` is reachable from `refs/heads/develop` but not from the root `HEAD`
- **WHEN** the Branches chip loads
- **THEN** the list SHALL NOT contain `nergal/c-3`

#### Scenario: Branches outside the nergal namespace are ignored

- **GIVEN** a local branch `feature/x` with 3 unpushed commits
- **WHEN** the Branches chip loads
- **THEN** the list SHALL NOT contain `feature/x`

#### Scenario: Symbolic ref is not listed

- **GIVEN** `refs/heads/nergal/x` is a symbolic ref to `refs/heads/main`
- **WHEN** the Branches chip loads
- **THEN** the list SHALL NOT contain `nergal/x`

#### Scenario: Tag with the same name does not change the branch name

- **GIVEN** a local branch `nergal/t-7` with 1 unpushed commit and a tag `nergal/t-7`
- **WHEN** the Branches chip loads
- **THEN** the list SHALL contain a row named exactly `nergal/t-7`

#### Scenario: Failed check keeps the branch visible

- **GIVEN** a session that uses `nergal/d-4` with merge target `gone`, and no ref `refs/heads/gone` exists
- **WHEN** the Branches chip loads
- **THEN** the list SHALL contain `nergal/d-4` with an unknown pending count

#### Scenario: Not a git repository

- **GIVEN** a workspace whose directory is not a git repository
- **WHEN** the Branches chip loads
- **THEN** the list SHALL be empty and no error SHALL be shown

### Requirement: Branch rows show their details

Each row SHALL show the branch name, the pending commit count (or "?" when unknown), the relative date and subject of the branch's last commit, and the name of the session that uses the branch. The list SHALL be ordered by last commit date, newest first. It SHALL be loaded when the chip opens, after each action of the chip, and when the user presses the refresh button. It SHALL NOT poll.

#### Scenario: Newest branch first

- **GIVEN** listed branches `nergal/old-1` with a last commit on 2026-11-01 and `nergal/new-2` with a last commit on 2026-11-10
- **WHEN** the Branches chip loads
- **THEN** `nergal/new-2` SHALL appear above `nergal/old-1`

### Requirement: Branches without a session are marked

A listed branch SHALL be marked "no session" when no session of the workspace has `worktree_branch` equal to the branch name. Otherwise the row SHALL show that session's name. A branch checked out, rebased or bisected in a worktree whose directory exists, or in a locked worktree, SHALL be marked "in use". A branch held only by registered worktrees whose directory is missing and that are not locked SHALL NOT be marked "in use"; its row SHALL show a "worktree missing" marker. The backend SHALL compute these fields from the database and git.

#### Scenario: Branch of a deleted session

- **GIVEN** the session that used `nergal/a-1` was deleted and its branch was kept because it had unpushed commits
- **WHEN** the Branches chip loads
- **THEN** the row for `nergal/a-1` SHALL show the "no session" marker

#### Scenario: Branch of a live worktree session

- **GIVEN** a session named `Refactor` with `worktree_branch` `nergal/refactor-5` and 1 unpushed commit
- **WHEN** the Branches chip loads
- **THEN** the row for `nergal/refactor-5` SHALL show `Refactor` and SHALL NOT show the "no session" marker

#### Scenario: Branch checked out outside Nergal

- **GIVEN** no session uses `nergal/e-6` and the user checked it out in a worktree created with `git worktree add`
- **WHEN** the Branches chip loads
- **THEN** the row SHALL show the "no session" and "in use" markers

#### Scenario: Worktree deleted by hand

- **GIVEN** `nergal/f-8` was checked out in `<repo>/.worktrees/nergal/f-8`, the user removed that directory with `rm -rf`, and git still lists it as prunable
- **WHEN** the Branches chip loads
- **THEN** the row SHALL show the "no session" and "worktree missing" markers and SHALL NOT show "in use"

### Requirement: Commit drill-down shows each commit's diff

Expanding a listed branch SHALL show its pending commits, newest first, at most 100, each with its short hash and subject. A pending commit is a commit returned by `git rev-list <branch-sha> --not --remotes <target-ref> --`. Expanding a commit SHALL list the files it changes. Opening a file SHALL open Zen mode on the diff of that file in that commit, with the commit's files as the Zen file list.

#### Scenario: Open a commit's file

- **GIVEN** a listed branch whose pending commit `abc1234` changes `src/a.ts` and `src/b.ts`
- **WHEN** the user expands the branch, expands `abc1234` and opens `src/a.ts`
- **THEN** Zen mode SHALL show the diff of `src/a.ts` in `abc1234`
- **AND** the Zen file list SHALL be `src/a.ts` and `src/b.ts`

#### Scenario: Pending commits only

- **GIVEN** a listed branch with 5 commits, of which the oldest 3 are in `main` (the target)
- **WHEN** the user expands the branch
- **THEN** exactly the newest 2 commits SHALL be shown

### Requirement: Branches chip keyboard navigation

The Branches chip SHALL keep one cursor over every visible row: branch rows, the commit rows of the expanded branch, and the file rows of the expanded commit, in display order. `ArrowDown`/`j` and `ArrowUp`/`k` SHALL move the cursor one row, wrapping at the ends. `Space` and `Enter` on a branch row SHALL expand or collapse it. `Space` and `Enter` on a commit row SHALL expand or collapse it. `Enter` on a file row SHALL open that file as "Open a commit's file" describes. `O` on a branch row SHALL start "Open session" and `D` on a branch row SHALL start "Delete", only when the row offers that action. No other key SHALL open a session or delete a branch. Collapsing the branch or commit that holds the cursor SHALL move the cursor to the collapsed row.

#### Scenario: Reach and open a file with the keyboard

- **GIVEN** the chip lists `nergal/a-1` then `nergal/b-2`, and `nergal/a-1` has the pending commit `abc1234` that changes `src/a.ts`
- **WHEN** the cursor is on `nergal/a-1` and the user presses `Space`, `j`, `Enter`, `j`, `Enter`
- **THEN** Zen mode SHALL show the diff of `src/a.ts` in `abc1234`

#### Scenario: Enter on a branch row expands it

- **GIVEN** the cursor is on `nergal/a-1`, marked "no session" and collapsed
- **WHEN** the user presses `Enter`
- **THEN** `nergal/a-1` SHALL be expanded and no session SHALL be opened

#### Scenario: Action keys only act on branch rows

- **GIVEN** the cursor is on a commit row of `nergal/a-1`, marked "no session"
- **WHEN** the user presses `O` or `D`
- **THEN** no confirmation SHALL open

#### Scenario: Collapse moves the cursor up

- **GIVEN** `nergal/a-1` is expanded and the cursor is on its commit `abc1234`
- **WHEN** the user collapses `nergal/a-1` by clicking its row
- **THEN** the cursor SHALL be on `nergal/a-1`

### Requirement: Branches chip stays current

While the Branches chip is shown, its list SHALL reload every 5 seconds and when the set of sessions in the workspace changes, without the user switching chips. The chip badge SHALL reload when the set of sessions in the workspace changes. A reload SHALL keep the cursor on the same row when that row still exists, and keep the expanded branch and commit when they still exist. A branch whose commit SHA changed SHALL load its commits again when expanded.

#### Scenario: Deleting a session shows its kept branch

- **GIVEN** the Branches chip is shown and a session uses `nergal/a-1`, which has 1 unpushed commit
- **WHEN** the user deletes that session and Nergal keeps the branch
- **THEN** the chip SHALL list `nergal/a-1` as "no session" and the badge SHALL count it, without a chip switch

#### Scenario: Branch restored from a terminal appears

- **GIVEN** the Branches chip is shown and the branch `nergal/a-1` does not exist
- **WHEN** the user runs `git branch nergal/a-1 <sha>` for an unpushed commit in a terminal
- **THEN** within 5 seconds the chip SHALL list `nergal/a-1`

#### Scenario: Reload keeps the cursor and the expansion

- **GIVEN** `nergal/a-1` is expanded and the cursor is on its commit `abc1234`
- **WHEN** the list reloads and `nergal/a-1` still points to the same SHA
- **THEN** `nergal/a-1` SHALL stay expanded and the cursor SHALL stay on `abc1234`

### Requirement: Open a session on a branch without a session

A listed branch marked "no session" and not marked "in use" SHALL offer an "Open session" action. The frontend SHALL first show a confirmation that names the branch, and call the backend only on confirmation. The backend SHALL refuse it unless the name is a valid Nergal branch name, the ref exists and is not symbolic, no session of the workspace uses it, it is not in use, and `<repo>/.worktrees/nergal/<slug>` does not exist. A valid name is `nergal/<slug>` where `<slug>` is non-empty, has no `/`, does not start with `.`, and has only ASCII letters, digits, `.`, `_` and `-`. The backend SHALL first remove the registrations of missing, unlocked worktrees that hold the branch. It SHALL then create the worktree with the branch attached (its `HEAD` is the symbolic ref `refs/heads/nergal/<slug>`), without creating or moving any branch, and create a Claude Code session named `<slug>` on it. The frontend SHALL add the session to the sidebar, mark it fresh, activate it and focus its terminal.

#### Scenario: Open session recreates the worktree on the branch

- **GIVEN** a branch `nergal/a-1` marked "no session" at commit `S` and no directory `<repo>/.worktrees/nergal/a-1`
- **WHEN** the user runs "Open session" on it
- **THEN** `git -C <repo>/.worktrees/nergal/a-1 symbolic-ref HEAD` SHALL print `refs/heads/nergal/a-1` and `HEAD` SHALL be `S`
- **AND** a session named `a-1` with `worktree_branch` `nergal/a-1` SHALL exist and be active
- **AND** `refs/heads/nergal/a-1` SHALL still point to `S`

#### Scenario: Worktree deleted by hand is reopened

- **GIVEN** `nergal/f-8` is held only by a missing, unlocked worktree registration
- **WHEN** the user runs "Open session" on it
- **THEN** that registration SHALL be gone, other missing registrations SHALL remain, and the new session SHALL be on `nergal/f-8`

#### Scenario: Cancel opens nothing

- **GIVEN** a branch `nergal/a-1` marked "no session"
- **WHEN** the user presses `O` on its row and cancels the confirmation
- **THEN** no backend command SHALL run and no session SHALL be created

#### Scenario: Branch name is HTML-escaped in the open confirmation

- **GIVEN** a branch `nergal/<b>x`
- **WHEN** the open confirmation opens
- **THEN** the dialog body SHALL contain the escaped text `nergal/&lt;b&gt;x` and no `<b>` element

#### Scenario: Leftover directory refuses

- **GIVEN** a branch `nergal/a-1` marked "no session" and an existing directory `<repo>/.worktrees/nergal/a-1`
- **WHEN** the user runs "Open session" on it
- **THEN** the command SHALL return an error that names the directory, and no session or worktree SHALL be created

#### Scenario: Session appeared since the list loaded

- **GIVEN** the chip listed `nergal/a-1` as "no session", and a session that uses `nergal/a-1` was created afterwards
- **WHEN** the user runs "Open session" on it
- **THEN** the command SHALL return an error that names that session, and no worktree SHALL be created

#### Scenario: Invalid branch name refuses

- **WHEN** the command receives the branch `nergal/../x`, `nergal/a/b`, `nergal/.x` or `feature/x`
- **THEN** it SHALL return an error and SHALL NOT run any git command that changes the repository

#### Scenario: The session row cannot be written

- **GIVEN** the worktree was created and the session row cannot be inserted
- **WHEN** "Open session" finishes
- **THEN** the new worktree SHALL be removed and the command SHALL return the error

### Requirement: Delete a branch without a session

A listed branch marked "no session" and not marked "in use" SHALL offer a "Delete" action. The frontend SHALL first show a destructive confirmation. It SHALL name the branch, state its pending commit count, and state that those commits exist only on this machine. On confirmation, it SHALL call the backend with the branch name and the commit SHA the row was loaded with. On success, the toast SHALL show the restore command `git -C <repo> branch <branch> <sha>`, with the repository path quoted for the shell. A kept branch SHALL show its reason. The list SHALL reload in every case.

#### Scenario: Cancel keeps the branch

- **WHEN** the user cancels the confirmation
- **THEN** no backend command SHALL run and the branch SHALL remain

#### Scenario: Branch name is HTML-escaped in the confirmation

- **GIVEN** a branch `nergal/<b>x`
- **WHEN** the confirmation opens
- **THEN** the dialog body SHALL contain the escaped text `nergal/&lt;b&gt;x` and no `<b>` element

#### Scenario: Restore command names the repository

- **GIVEN** the branch `nergal/a-1` of the repository `/home/u/my repo` was deleted at commit `S`
- **WHEN** the toast is shown
- **THEN** it SHALL contain `git -C '/home/u/my repo' branch nergal/a-1 S`

### Requirement: Branch delete guards

The delete command SHALL keep the branch and return a reason when the name is not a valid Nergal branch name, or the ref is symbolic or unreadable. It SHALL also keep it when the ref no longer points to the confirmed SHA, a session of the workspace uses it, it is in use, or the restore record cannot be written. It SHALL first remove the registrations of missing, unlocked worktrees that hold the branch. Then it SHALL append the branch and SHA to the restore record (`deleted-branches.log` under the user data directory). It SHALL check again that the branch is not in use, and delete the ref only if it still points to the confirmed SHA, without following a symbolic ref. It SHALL NOT require the branch to be safe to drop. A missing ref SHALL be reported as already deleted.

#### Scenario: Delete a branch with unpushed work

- **GIVEN** a branch `nergal/a-1` marked "no session" at commit `S` with 2 unpushed commits
- **WHEN** the user confirms its deletion
- **THEN** `refs/heads/nergal/a-1` SHALL not exist
- **AND** the restore record SHALL end with a line containing the repository path, `nergal/a-1` and `S`
- **AND** the result SHALL be deleted with SHA `S` and the repository path

#### Scenario: Worktree deleted by hand does not block the delete

- **GIVEN** `nergal/f-8` is held only by a missing, unlocked worktree registration
- **WHEN** the user confirms its deletion
- **THEN** the branch SHALL be deleted and that registration SHALL be gone

#### Scenario: Locked missing worktree keeps the branch

- **GIVEN** `nergal/g-9` is held by a missing worktree that is locked (`git worktree lock`)
- **WHEN** the delete command runs for it
- **THEN** the branch SHALL remain and the result SHALL be kept with the in-use reason

#### Scenario: Already deleted

- **GIVEN** `refs/heads/nergal/a-1` no longer exists
- **WHEN** the delete command runs for it
- **THEN** the result SHALL be "already deleted" and the restore record SHALL be unchanged

#### Scenario: Branch moved after the list loaded

- **GIVEN** the row for `nergal/a-1` was loaded at commit `S`, and the branch now points to `T`
- **WHEN** the user confirms its deletion
- **THEN** the branch SHALL remain at `T` and the result SHALL be kept with the reason that the branch changed

#### Scenario: Branch checked out since the list loaded

- **GIVEN** `nergal/a-1` became checked out in a worktree after the list loaded
- **WHEN** the user confirms its deletion
- **THEN** the branch SHALL remain and the result SHALL be kept with the in-use reason

#### Scenario: Session appeared since the list loaded

- **GIVEN** a session that uses `nergal/a-1` was created after the list loaded
- **WHEN** the user confirms its deletion
- **THEN** the branch SHALL remain and the result SHALL name that session

#### Scenario: Restore record cannot be written

- **GIVEN** the restore record path cannot be written
- **WHEN** the user confirms the deletion of `nergal/a-1`
- **THEN** the branch SHALL remain and the result SHALL name the record path

### Requirement: Branch changes on one repository run one at a time

"Open session", "Delete" and the branch drop of `delete_session` SHALL hold a per-repository lock for their whole check-and-change sequence. The lock SHALL be released on every return path. A command SHALL never wait for the per-repository lock while it holds the database lock.

#### Scenario: Delete waits for a running open

- **GIVEN** an "Open session" on `nergal/a-1` holds the lock of repository `R`
- **WHEN** a delete of `nergal/a-1` in `R` starts
- **THEN** the delete SHALL start its checks only after the open releases the lock
- **AND** the delete SHALL keep the branch and name the session the open created

#### Scenario: The database stays available while a command waits

- **GIVEN** the lock of repository `R` is held
- **WHEN** a delete for a branch of `R` is waiting for it
- **THEN** another caller SHALL be able to take the database lock
