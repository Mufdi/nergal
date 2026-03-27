## 1. Backend: Workspace & Session Data Model

- [x] 1.1 Create `src-tauri/src/workspace.rs` — Workspace, Session, SessionStatus structs with serde. WorkspaceManager with load/save to `~/.config/cluihud/state.json`, atomic writes (temp file + rename)
- [x] 1.2 Create `src-tauri/src/worktree.rs` — Functions: `create_worktree(repo_path, slug)`, `remove_worktree(path)`, `list_worktrees(repo_path)`, `is_git_repo(path)`. All execute `git` CLI commands
- [x] 1.3 Register WorkspaceManager as Tauri managed state in `src-tauri/src/lib.rs`

## 2. Backend: Session State Manager

- [x] 2.1 Create `src-tauri/src/session_state.rs` — SessionRuntime struct (PlanManager + TaskStore + CostSummary). SessionStateManager as `HashMap<SessionId, SessionRuntime>` with get_or_create, remove
- [x] 2.2 Replace SharedPlanManager + SharedTaskStore singletons with SessionStateManager in `src-tauri/src/lib.rs`
- [x] 2.3 Refactor `src-tauri/src/hooks/server.rs` — route events by session_id to correct SessionRuntime. Update plan:ready, tasks:update, cost:update, files:modified handlers
- [x] 2.4 Refactor plan commands (get_plan, save_plan, reject_plan, approve_plan, diff_plan) to accept session_id parameter and operate on per-session PlanManager

## 3. Backend: Workspace/Session Commands

- [x] 3.1 Add Tauri commands to `src-tauri/src/commands.rs`: `create_workspace(repo_path)`, `get_workspaces()`, `delete_workspace(workspace_id)`
- [x] 3.2 Add Tauri commands: `create_session(workspace_id, name)`, `delete_session(session_id)`, `rename_session(session_id, name)`, `get_session(session_id)`
- [x] 3.3 Register all new commands in `src-tauri/src/lib.rs` invoke_handler
- [x] 3.4 On startup: reconcile state.json with git worktree list (cleanup orphans)

## 4. Frontend: Session-Scoped State

- [x] 4.1 Create `src/stores/workspace.ts` — workspacesAtom, activeSessionIdAtom, activeSessionAtom (derived), activeWorkspaceAtom (derived)
- [x] 4.2 Refactor `src/stores/plan.ts` — Replace flat atoms with `planStateMapAtom: Record<sessionId, PlanState>`. Add `activePlanAtom` derived from activeSessionIdAtom
- [x] 4.3 Refactor `src/stores/session.ts` — costMapAtom, modeMapAtom, terminalMapAtom as Record<sessionId, T>. Add activeCostAtom, activeModeAtom derived atoms
- [x] 4.4 Create `src/stores/activity.ts` refactor — activityMapAtom: Record<sessionId, ActivityEntry[]>. Add activeActivityAtom derived
- [x] 4.5 Refactor `src/stores/hooks.ts` — All set() calls now index by session_id into the map atoms. plan:ready, tasks:update, cost:update, files:modified, hook:event all use payload.session_id as key

## 5. Frontend: Terminal Manager

- [x] 5.1 Create `src/components/terminal/TerminalManager.tsx` — Renders one div per session in terminalMapAtom. Only active session's div has display:block. Others display:none
- [x] 5.2 Refactor `src/components/terminal/useTerminal.ts` — Accept sessionId prop. Register PTY in terminalMapAtom[sessionId]. Create PTY with cwd from session's worktree_path or workspace repo_path
- [x] 5.3 Update `src/components/layout/Workspace.tsx` — Replace `<TerminalPanel cwd={...}>` with `<TerminalManager />`. All panels read from active-session derived atoms

## 6. Frontend: Sidebar Workspace CRUD

- [x] 6.1 Rewrite `src/components/layout/Sidebar.tsx` WorkspacesView — Load from get_workspaces() command (cluihud's own state, not transcript scanning). Expandable workspace tree with sessions
- [x] 6.2 Add workspace button ("+") — Opens native folder picker via @tauri-apps/plugin-dialog, calls create_workspace command
- [x] 6.3 Add session button ("+") per workspace — Dialog for session name, calls create_session, auto-switches to new session
- [x] 6.4 Session click handler — Calls switch logic: set activeSessionIdAtom, create PTY if needed (lazy), write `claude --resume <id>` if resuming
- [x] 6.5 Session delete via context menu — Confirmation dialog, calls delete_session command
- [x] 6.6 Session status indicators — Colored dots (gray=idle, green=running, orange=needs_attention, blue=completed)

## 7. Frontend: UI Updates

- [x] 7.1 Update TopBar — Show active session name instead of workspace name
- [x] 7.2 Update StatusBar — Show active session's cost from activeCostAtom
- [x] 7.3 Update PlanPanel — Read from activePlanAtom instead of global planContentAtom
- [x] 7.4 Update TaskPanel — Already uses activeSessionTasksAtom (verify it works with new store)
- [x] 7.5 Update ActivityLog — Read from activeActivityAtom instead of global activityAtom
- [x] 7.6 Update RightPanel PlanFileSidebar — Show plans for active session only

## 8. Integration & Verification

- [x] 8.1 `cargo check` passes with no errors
- [x] 8.2 `npx tsc --noEmit` passes with no errors
- [x] 8.3 `pnpm vite build` succeeds
- [x] 8.4 Manual test: create workspace, create session, verify worktree exists
- [x] 8.5 Manual test: create second session, verify independent terminal
- [x] 8.6 Manual test: switch sessions, verify all panels swap state
- [x] 8.7 Manual test: delete session, verify worktree removed
- [x] 8.8 Manual test: restart app, verify persistence
- [x] 8.9 Manual test: run claude in session, verify hooks route correctly ⚠️ Activities y Tasks panels vacíos — hooks no rutean actividades/tasks correctamente
- [x] 8.10 Cleanup: removed `list_sessions` cmd + `SessionSummary` struct from commands.rs, unregistered from lib.rs, deleted `NavSidebar.tsx`, deleted 11 legacy .rs files from src/ (hooks/, claude/, tasks/)
