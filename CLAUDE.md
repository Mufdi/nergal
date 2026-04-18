# cluihud

Desktop app wrapper para Claude Code CLI (Plan Pro) en Linux.
Tauri v2 + React 19 hybrid architecture.

## What cluihud is NOT

Prevent misframing in analysis, recommendations, and tool-driven suggestions:

- **NOT a standalone terminal** — cluihud always runs `claude` underneath. It does not replace bash/zsh, tmux, or any general-purpose shell.
- **NOT a Claude Code reimplementation** — it does not reimplement slash commands, skills, agents, hooks, or any Claude-native functionality. It **observes** the session via the hook pipeline and **augments** the UX around it.
- **NOT an agent framework** — no training loops, no skill emergence, no multi-model orchestration. Those belong to Claude Code itself or to personal skills.

**Useful recommendations** are those that improve the **experience of using Claude Code**: plan editing UX, task visibility, session navigation, hook-driven panels, keyboard shortcuts, workspace/worktree management. Recommendations that would replace Claude-native functionality or add terminal-level features should be redirected to the `personal` lane (personal CC workflow) or rejected.

## Project Documentation

- Design doc completo: `/home/user/Documents/Obsidian/Projects/cluihud/cluihud.md`

## Stack

### Backend (Rust via Tauri)
- **Runtime**: Tauri 2.10 (desktop runtime + IPC bridge)
- **Async**: tokio (multi-threaded runtime)
- **Terminal**: portable-pty (PTY session management)
- **Database**: rusqlite (SQLite, bundled)
- **File watching**: notify + notify-debouncer-full (inotify backend)
- **Markdown**: pulldown-cmark
- **Diffing**: similar
- **CLI**: clap (hook subcommands)
- **Logging**: tracing + tracing-subscriber
- **Serialization**: serde + serde_json

### Frontend (React in Webview)
- **UI**: React 19 + TypeScript 5.9
- **Build**: Vite 7.3
- **Package manager**: pnpm 10.28
- **State**: Jotai (atomic state management)
- **Styling**: TailwindCSS 4.2 + shadcn/ui + class-variance-authority
- **Terminal**: canvas renderer + wezterm-term VT emulator (in Rust backend)
- **Editor**: CodeMirror 6 (syntax highlighting) + MDXEditor (plan editing)
- **Markdown**: react-markdown + remark-gfm
- **Layout**: react-resizable-panels
- **Flow diagrams**: @xyflow/react
- **Icons**: lucide-react
- **Components**: @base-ui/react (headless)

## Architecture

Tauri IPC bridge: Rust backend ↔ React frontend via `invoke()` / `listen()`

```
src/                              # React frontend (TypeScript)
├── main.tsx                      # Entry point (StrictMode + Jotai Provider)
├── App.tsx                       # Root (ErrorBoundary + Workspace)
├── components/
│   ├── layout/                   # Workspace, TopBar, Sidebar, RightPanel, StatusBar
│   ├── terminal/                 # TerminalManager + terminalService (canvas + wezterm-term)
│   ├── editor/                   # CodeEditor (CodeMirror 6)
│   ├── plan/                     # PlanPanel, PlanEditor, AnnotatableMarkdownView
│   ├── spec/                     # SpecViewer (OpenSpec artifacts)
│   ├── tasks/                    # TaskPanel
│   ├── activity/                 # ActivityDrawer
│   ├── session/                  # AskUserModal, SessionRow
│   ├── git/                      # GitPanel
│   ├── command/                  # CommandPalette (Cmd+K)
│   ├── ui/                       # shadcn components (button, dialog, tabs, etc.)
│   └── settings/
├── stores/                       # Jotai atoms
│   ├── workspace.ts              # Workspaces, sessions, costs
│   ├── rightPanel.ts             # Tab state, active tab
│   ├── hooks.ts                  # Hook event listeners
│   ├── shortcuts.ts              # Keyboard shortcuts
│   ├── plan.ts, tasks.ts, git.ts # Domain state
│   └── ...
├── hooks/                        # React hooks (useKeyboardShortcuts)
└── lib/                          # tauri.ts, types.ts, utils.ts

src-tauri/src/                    # Rust backend
├── main.rs                       # CLI entry (hook subcommands)
├── lib.rs                        # Tauri app init, plugins, commands
├── commands.rs                   # 60+ Tauri invoke handlers
├── pty.rs                        # PTY session management
├── db.rs                         # SQLite wrapper
├── hooks/                        # Unix socket server + events
├── claude/                       # Transcript watcher, plan manager, OpenSpec
├── tasks/                        # Task parsing from transcripts
├── worktree.rs                   # Git worktree management
└── config.rs                     # Config, paths, defaults
```

## Coding Standards

### Rust Style (enforced by skills)
- `for` loops > iterator chains
- `let...else` for early returns
- Shadow variables, don't rename (`raw_`, `parsed_`)
- Newtypes over bare strings/bools
- Match all enum variants explicitly (no wildcards)
- No `unwrap()` outside tests — use `anyhow` with `?`
- Doc comments (`///`) on all public items (RFC 1574 style)
- No inline comments explaining WHAT — only WHY

### React/TypeScript Patterns
- Jotai atoms for all state (primitive, composable)
- `useAtomValue()` / `useSetAtom()` for subscriptions
- Tauri `invoke<T>()` for frontend → backend calls
- Tauri `listen()` for backend → frontend events
- TailwindCSS utility classes, shadcn/ui components
- Terminal managed outside React (terminalService.ts owns xterm instances)

### Project Conventions
- No TODO/FIXME — track in issues
- Comments only for WHY, never WHAT
- Absolute paths in tool calls
- Parallel independent operations

## Verification Commands

| Action | Command |
|--------|---------|
| Dev | `pnpm dev` (Vite + Tauri) |
| Build | `pnpm build` (TS check + Vite + Tauri bundle) |
| Rust build | `cd src-tauri && cargo build` |
| Rust test | `cd src-tauri && cargo test` |
| Rust lint | `cd src-tauri && cargo clippy -- -D warnings` |
| Rust format | `cd src-tauri && cargo fmt --check` |
| Full check | `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` |

Run full check after significant changes.

## Key Concepts

### Communication Patterns
1. **Frontend → Backend**: `invoke<T>(command, args)` via Tauri IPC
2. **Backend → Frontend**: Tauri `emit()` events (async)
3. **State sync**: Jotai atoms + `setupHookListeners()` updates atoms on events
4. **Terminal**: `wezterm-term` parses PTY bytes in Rust; an emitter task coalesces deltas and emits `terminal:grid-update` events; the frontend canvas renders changed rows via a glyph atlas and sends keystrokes back through `invoke("terminal_input")`

### Event Flow
1. Claude CLI runs inside a PTY spawned by the app
2. Hooks (async) write events to a Unix socket
3. App listens on the socket + watches transcript files via inotify
4. Events flow through tokio channels → Tauri emit → Jotai atom updates
5. React components auto-re-render on atom changes

### Plan Editing Flow (bidirectional)
1. Claude writes plan to `plansDirectory`
2. `PreToolUse[ExitPlanMode]` hook notifies the app
3. App loads plan in MDXEditor panel
4. User edits + adds inline annotations
5. On reject: `UserPromptSubmit` hook injects "re-read plan at <path>"
6. Claude re-reads the edited file and re-plans

### Hook Config (project settings.json)
CLI uses `cluihud hook send <event>` for async event forwarding and `cluihud hook inject-edits` for sync prompt modification.
```json
{
  "hooks": {
    "SessionStart": [{ "hooks": [{ "type": "command", "command": "cluihud hook send session-start", "async": true }] }],
    "PreToolUse": [{ "matcher": "ExitPlanMode", "hooks": [{ "type": "command", "command": "cluihud hook send plan-ready", "async": true }] }],
    "PostToolUse": [{ "hooks": [{ "type": "command", "command": "cluihud hook send tool-done", "async": true }] }],
    "TaskCompleted": [{ "hooks": [{ "type": "command", "command": "cluihud hook send task-done", "async": true }] }],
    "Stop": [{ "hooks": [{ "type": "command", "command": "cluihud hook send stop", "async": true }] }],
    "SessionEnd": [{ "hooks": [{ "type": "command", "command": "cluihud hook send session-end", "async": true }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "cluihud hook inject-edits" }] }]
  }
}
```

## Release Profile

```toml
[profile.release]
lto = true
codegen-units = 1
panic = "abort"
strip = true
```
