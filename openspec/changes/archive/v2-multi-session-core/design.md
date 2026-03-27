## Context

cluihud v1 is a single-session Tauri v2 + React 19 desktop app wrapping Claude Code CLI on Linux. Backend: Rust (~2,763 lines) with singleton PlanManager, TaskStore, PtyManager. Frontend: React (~2,804 lines) with Jotai atoms, xterm.js terminal. Hook server on Unix socket receives Claude CLI events tagged by `session_id`, but all state converges to a single session.

Competitive analysis of cmux (Swift/macOS), Superset (Electron/macOS), and Conductor (Electron/macOS) revealed git worktrees as the standard isolation primitive. All three are macOS-only, making cluihud the only Linux option.

**Constraints**: Tauri v2 (not Electron), Linux only, Claude Code only, no sudo, incremental implementation.

## Goals / Non-Goals

**Goals:**
- Run N concurrent Claude Code sessions with filesystem isolation via git worktrees
- Each session has its own terminal, plan state, tasks, activity, cost, modified files
- Session switching is instant (sub-50ms) with no terminal re-creation
- Workspaces and sessions persist across app restarts
- Sidebar shows workspaces → sessions tree with status indicators
- Session CRUD: create (with worktree), switch, resume, delete (cleanup worktree)

**Non-Goals:**
- Multi-agent support (Codex, Gemini) — Claude Code only
- Cloud sync or team collaboration
- Embedded browser pane
- Port allocation per workspace
- Resource monitoring (CPU/memory)
- Git operations beyond worktree management (commit/push/PR in Phase 2)

## Decisions

### D1: Git worktrees for session isolation
**Choice**: `git worktree add .worktrees/cluihud/<slug> -b cluihud/<slug>`
**Alternative**: Separate clones or directory copies
**Rationale**: Worktrees share `.git` storage (efficient), allow per-session branches, enable `git diff` for change review (Phase 2). Superset and Conductor both use this pattern. Location `.worktrees/cluihud/` keeps them visible but gitignore-able.

### D2: JSON file persistence (not SQLite)
**Choice**: `~/.config/cluihud/state.json` with Workspace/Session structs
**Alternative**: SQLite via rusqlite (Superset uses better-sqlite3)
**Rationale**: Expected volume is <100 sessions across <20 workspaces. No complex queries needed. JSON is human-readable for debugging. Can migrate to SQLite later if needed.

### D3: Hidden div terminal multiplexing
**Choice**: Render all active terminals as `<div style="display:none">`, show only the active one
**Alternative**: Destroy/recreate terminals on switch
**Rationale**: Preserves scroll history, WebGL context, and xterm.js state. Instant switch (~0ms vs ~200ms for re-init). Trade-off: ~5-10MB per terminal instance. Acceptable for <20 sessions.

### D4: `SessionStateManager` replacing singletons
**Choice**: `HashMap<SessionId, SessionRuntime>` where each runtime has PlanManager + TaskStore + CostSummary
**Alternative**: Keep global singletons, filter by session_id in frontend
**Rationale**: Backend-level isolation prevents cross-session state corruption. Hook events routed by session_id to correct runtime. Frontend atoms become simple projections.

### D5: Jotai Record<sessionId, T> pattern
**Choice**: `planStateMapAtom: Record<string, PlanState>` with derived `activePlanAtom`
**Alternative**: Zustand slices, or atom family per session
**Rationale**: Consistent with existing Jotai codebase. Record pattern is explicit about what's stored. Derived atoms are reactive — changing `activeSessionIdAtom` recomputes all active-* atoms automatically.

### D6: First session uses main checkout (no worktree)
**Choice**: The first/default session in a workspace uses the repo's main checkout directly. Additional sessions get worktrees.
**Alternative**: Always create worktree, even for first session
**Rationale**: Avoids unnecessary worktree for single-session users. Main checkout is where the user already has their terminal. Matches user mental model. Worktrees are opt-in for parallelism.

## Risks / Trade-offs

- **[Terminal memory]** Each xterm.js instance uses ~5-10MB → Limit to 20 concurrent sessions, warn above 10. Mitigation: lazy PTY creation (only when session is activated).
- **[Git worktree conflicts]** If user manually works in the main checkout while a worktree session is active, merge conflicts on worktree removal. Mitigation: prompt before delete, show diff.
- **[Hook socket sharing]** All Claude CLI sessions share the same Unix socket. Events must be correctly routed by session_id. Mitigation: session_id is already present in all hook events.
- **[Persistence corruption]** Crash during state.json write could corrupt workspace data. Mitigation: atomic write (write temp file, rename).
- **[Worktree cleanup on crash]** If app crashes, orphan worktrees remain. Mitigation: on startup, reconcile state.json with `git worktree list`.
