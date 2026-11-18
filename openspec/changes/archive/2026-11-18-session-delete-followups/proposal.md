Linear: none (no room for new issues; recorded as a comment on FRI-490)

## Why

The manual walk of `session-resume-by-id` and `worktree-root-owned-cleanup` (2026-11-17) found three gaps. `delete_session` never deletes the session's local branch `nergal/<slug>`, so every deleted worktree session leaves a branch behind. A session kept after a failed reclaim has a half-deleted worktree (git already removed files and the `.git` file), and opening it launches Claude Code in that broken directory, which asks for directory trust. A session created during the current run starts a fresh conversation when its terminal is reopened, because its "fresh" flag is never cleared.

## What Changes

- Deleting a worktree session also deletes its local branch when the branch is safe to drop: every commit of the branch is reachable from a remote-tracking ref or from the session's merge target (or the repo root's current `HEAD`). The delete is an atomic compare-and-delete on the checked commit; the old commit goes to a durable append-only record and to an informational toast with the exact restore command. Otherwise the branch is kept and an info toast names it.
- A worktree session whose worktree has no `.git` entry (half deleted or gone) is "pending delete". The sidebar marks it, the backend refuses to launch an agent in it from any path, and the only action is to retry the delete.
- The fresh-session flag is consumed by the first agent launch. Any later launch of that session resumes its own conversation.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `session-continuity` (introduced by `session-resume-by-id`, not yet archived): adds the branch rule, the pending-delete state and the one-shot fresh flag as ADDED requirements. Archive order: `session-resume-by-id` and `worktree-root-owned-cleanup` first, then this change.

## Impact

- Backend: `src-tauri/src/worktree.rs` (branch safety check + compare-and-delete), `src-tauri/src/commands/workspaces_sessions.rs` (`delete_session` deletes the branch and returns typed notices), `get_workspaces` (pending-delete flag), `src-tauri/src/pty.rs` (`start_claude_session` refuses a pending session), `src-tauri/src/models.rs` (`Session` gains a computed field, not persisted).
- Frontend: `src/components/terminal/TerminalManager.tsx` (fresh flag, pending panel, no env shells for pending), `src/components/session/SessionRow.tsx` (pending marker), `src/stores/pendingDeletes.ts` (typed notices).
- No schema migration.
