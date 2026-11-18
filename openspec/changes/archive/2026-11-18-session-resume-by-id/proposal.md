Linear: none (no room for new issues; recorded as a comment on FRI-490)

## Why

A Claude Code session reopens with `claude --continue`, which picks the newest conversation in the cwd. Two sessions that share a cwd therefore share one conversation. This happens with several repo-root sessions, and with a worktree session whose directory was deleted outside Nergal: startup reconciliation turns it into a repo-root session, and it then reopens the main session's conversation. On 2026-11-02 and 2026-11-12 four XD001 sessions lost their own conversation this way. The degraded sessions also cannot be deleted, because the sidebar only offers delete when `worktree_path` is set.

## What Changes

- Nergal records each Claude Code session's own conversation id. It reads the id from the `SessionStart` hook and stores it in the existing `agent_internal_session_id` column. It updates the id when the conversation changes (`/clear`, `/resume`).
- Reopening a Claude Code session runs `claude --resume <id>` with its own id. **BREAKING**: `--continue` is no longer used for Claude Code sessions.
- A session without a recorded id falls back to the conversation id in its last `Stop` transcript path. When no id resolves to a transcript on disk, the session starts a fresh conversation. It never borrows another session's conversation.
- When a worktree directory no longer exists at startup, the session becomes a repo-root session and keeps its own conversation. Nergal shows one notice that lists the affected sessions.
- Any session can be deleted, except the last repo-root session of a workspace. This guard lives in the backend and in the sidebar. New sessions keep the current rule: the first session of a workspace is a repo-root session, and later ones get a worktree.

## Capabilities

### New Capabilities

- `session-continuity`: each session keeps its own agent conversation across reopen, restart and worktree loss; the repo-root delete guard.

### Modified Capabilities

- `cc-adapter`: the "CC adapter spawns claude binary with current behavior" requirement changes from `--continue` to `--resume <conversation id>`.

## Impact

- Backend: `src-tauri/src/pty.rs` (resume resolution), `src-tauri/src/hooks/server.rs` (id capture on `SessionStart`), `src-tauri/src/lib.rs` (`reconcile_worktrees` + startup notice), `src-tauri/src/commands/workspaces_sessions.rs` (`delete_session` guard), `src-tauri/src/db/sessions.rs` (queries).
- Frontend: `src/components/session/SessionRow.tsx`, `src/components/layout/Sidebar.tsx` (delete affordance), one startup toast listener.
- No schema migration. The change reuses `sessions.agent_internal_session_id` and `session_transcripts.transcript_path`.
- Other agents (Pi, OpenCode, Codex) keep their current resume path. They are frozen (focus freeze 2026-10-04).
