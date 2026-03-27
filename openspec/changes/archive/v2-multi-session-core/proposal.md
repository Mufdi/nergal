## Why

cluihud operates with a single-session model: one PTY, one global plan, one task store. Users cannot run parallel Claude Code sessions without conflicts. Competitive analysis of cmux, Superset, and Conductor revealed that git worktree isolation is the industry standard for multi-agent session management. cluihud needs this to be viable as a power-user cockpit for Claude Code on Linux.

## What Changes

- **Workspace management**: Git repos become first-class workspaces with CRUD operations, folder picker for adding new workspaces, persistent state in `~/.config/cluihud/state.json`
- **Session isolation via git worktrees**: Each session creates a git worktree with its own branch (`cluihud/<slug>`), giving agents isolated filesystems within the same repo
- **Multi-PTY terminal**: Each session gets its own xterm.js terminal instance. All terminals render as hidden divs; only the active session's terminal is visible. Instant switching without destroying scroll history or WebGL context
- **Session-scoped state**: Plan content, tasks, activity log, modified files, cost tracking — all scoped per `session_id` using `Record<sessionId, T>` Jotai atoms with derived atoms for the active session
- **Session lifecycle**: Create (worktree + PTY + `claude`), switch (swap active atoms), resume (`claude --resume <id>`), delete (kill PTY + remove worktree)
- **Backend state manager**: Replace singleton `SharedPlanManager` + `SharedTaskStore` with `SessionStateManager` that routes hook events to per-session runtime by `session_id`
- **BREAKING**: `planContentAtom`, `costSummaryAtom`, `sessionModeAtom`, `terminalIdAtom` become session-scoped maps. All components consuming these atoms must use derived active-session versions

## Capabilities

### New Capabilities
- `workspace-management`: CRUD for workspaces (git repos), persistent state, folder picker integration via tauri-plugin-dialog
- `session-lifecycle`: Create/switch/resume/delete sessions with git worktree isolation, per-session PTY management
- `multi-terminal`: Render multiple xterm.js terminals simultaneously, visibility toggling for instant session switching
- `session-scoped-state`: All plan/task/activity/cost/file state keyed by session_id with reactive derived atoms

### Modified Capabilities

## Impact

- **Backend**: New modules `workspace.rs`, `worktree.rs`, `session_state.rs`. Major refactor of `commands.rs`, `hooks/server.rs`, `lib.rs`
- **Frontend**: New `stores/workspace.ts`, `TerminalManager.tsx`. Major refactor of `stores/hooks.ts`, `stores/plan.ts`, `stores/session.ts`, `Sidebar.tsx`, `Workspace.tsx`, `useTerminal.ts`
- **Dependencies**: `tauri-plugin-dialog` (already added), `git` CLI (runtime dependency for worktree operations)
- **Persistence**: New file `~/.config/cluihud/state.json` for workspace/session state
- **Git**: Creates `.worktrees/cluihud/` directory in user repos (should be gitignored)
