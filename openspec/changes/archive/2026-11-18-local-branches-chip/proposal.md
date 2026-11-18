Linear: none (no room for new issues; recorded as a comment on FRI-490)

## Why

Since `session-delete-followups`, deleting a worktree session keeps its local branch when the branch has commits that exist only on this machine. A worktree removed outside Nergal also leaves its branch behind, and the session loses the link to it. No part of the app shows these branches, so their work is easy to forget and hard to recover or clean up.

The History chip also has a bug: opening a file of a commit shows the working-tree diff of that file, not the diff of the commit.

## What Changes

- The git panel gains a sixth chip, `Branches`. It lists the workspace's local `nergal/*` branches with pending work: commits that are on no remote-tracking ref and not in the branch's merge target. This is the negation of the rule that `delete_session` uses to decide that a branch is safe to drop.
- Each listed branch shows its name, its number of pending commits, the date and subject of its last commit, and the session that uses it. A branch that no session uses shows a "no session" marker.
- Expanding a branch lists its pending commits. Expanding a commit lists its files. Opening a file shows the diff of that commit for that file in Zen mode.
- A branch with no session offers two actions:
  - **Open session**: recreates the worktree `<repo>/.worktrees/nergal/<slug>` on the existing branch, creates a Claude Code session on it, and activates the session.
  - **Delete**: after a confirmation that names the branch and its pending commit count, deletes the branch. The delete writes the branch and its commit to the existing restore record first, and the toast shows the exact restore command.
- A branch that a session uses, or that is checked out, rebased or bisected in any worktree, offers neither action.
- A branch whose worktree directory was removed by hand shows a "worktree missing" marker. Both actions first clear that stale git registration.
- A new backend command returns the diff of one file in one commit. The History chip uses it, so opening a commit's file shows that commit's diff.

## Capabilities

### New Capabilities

- `local-branches`: the Branches chip. It covers the pending-work rule, the session marker, the commit and diff drill-down, and the "Open session" and "Delete" actions with their guards.

### Modified Capabilities

- `git-panel-v2`: the five-chip "Chip-strip navigation in the GitPanel" requirement is replaced by a six-chip one. The new requirement keeps the active chip per session, as the code already does. Another new requirement states that opening a commit's file shows that commit's diff.

## Impact

- Backend: `src-tauri/src/worktree.rs` (branch listing with pending-work count, worktree for an existing branch, per-commit file diff, forced branch delete with the existing guards), `src-tauri/src/commands/git.rs` (new workspace-scoped commands and the per-commit diff command), `src-tauri/src/commands/workspaces_sessions.rs` (open a session on an existing branch), `src-tauri/src/lib.rs` (command registration).
- Frontend: `src/stores/git.ts` and `src/components/git/chips/ChipStrip.tsx` (sixth chip), a new `src/components/git/chips/BranchesChip.tsx`, `src/components/git/GitPanel.tsx` (chip body), `src/stores/zenMode.ts`, `src/components/zen/ZenMode.tsx` and `src/components/plan/DiffView.tsx` (commit diffs), `src/components/git/chips/HistoryChip.tsx` (bug fix), and the session activation flow shared with `src/components/layout/Sidebar.tsx`.
- No schema migration. The restore record `~/.local/share/nergal/deleted-branches.log` keeps its format.
