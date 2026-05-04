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

- Project hub (Obsidian): `/home/felipe/Documents/Obsidian23/Projects/cluihud/cluihud.md` — referencial; este `CLAUDE.md` es la source of truth viva.
- Design system (UI tokens, components, decision rules): [`./DESIGN.md`](./DESIGN.md) — read before touching UI
- OpenSpec specs (feature contracts): [`./openspec/specs/`](./openspec/specs/) — read before implementing or proposing a feature

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
- **Editor**: CodeMirror 6 (syntax highlighting)
- **Markdown / plan editing**: react-markdown + remark-gfm + custom `AnnotatableMarkdownView` (inline annotations, no MDXEditor)
- **Layout**: react-resizable-panels
- **Flow diagrams**: @xyflow/react
- **Icons**: lucide-react
- **Components**: shadcn/ui + @base-ui/react (headless primitives)
- **Toasts**: sileo
- **Highlighter**: web-highlighter (annotation overlays)

## Architecture

Tauri IPC bridge: Rust backend ↔ React frontend via `invoke()` / `listen()`

```
src/                              # React frontend (TypeScript)
├── main.tsx                      # Entry point (StrictMode + Jotai Provider)
├── App.tsx                       # Root (ErrorBoundary + Workspace)
├── components/
│   ├── layout/                   # Workspace, TopBar, Sidebar, RightPanel, StatusBar, BuddyWidget (frozen)
│   ├── terminal/                 # TerminalManager + terminalService + fontAtlas + theme (canvas + wezterm-term)
│   ├── editor/                   # CodeEditor (CodeMirror 6)
│   ├── plan/                     # PlanPanel, AnnotatableMarkdownView
│   ├── spec/                     # SpecViewer / SpecPanel (OpenSpec artifacts)
│   ├── tasks/                    # TaskPanel
│   ├── activity/                 # ActivityDrawer
│   ├── session/                  # AskUserModal, SessionRow
│   ├── git/                      # GitPanel + chips/ (Files, History, Stashes, PRs, Conflicts) + PrViewer + ConflictsPanel + ShipDialog
│   ├── files/                    # FileBrowser, ModifiedFiles
│   ├── panel/                    # FileSidebar, FileListView, PlanListView, SpecListView (sidebars per panel category)
│   ├── zen/                      # ZenMode (full-screen contextual editor for diff/files/conflicts)
│   ├── command/                  # CommandPalette (Cmd+K)
│   ├── ui/                       # shadcn components (button, dialog, tabs, etc.)
│   └── settings/
├── stores/                       # Jotai atoms (jotaiStore.ts exports the shared appStore)
│   ├── workspace.ts              # Workspaces, sessions, costs
│   ├── rightPanel.ts             # Tab state, panel categories (document | tool)
│   ├── hooks.ts                  # Hook event listeners → atom updates
│   ├── shortcuts.ts              # Keyboard shortcuts (event.code based)
│   ├── plan.ts, tasks.ts, git.ts # Domain state
│   ├── annotations.ts            # Inline plan/spec annotations
│   ├── conflict.ts, ship.ts      # Conflict resolution + ship-flow state
│   ├── zenMode.ts, layout.ts     # Zen + layout config
│   ├── files.ts, activity.ts     # File browser + activity drawer
│   ├── askUser.ts, toast.ts      # Modals + sileo toasts
│   ├── session.ts, config.ts     # Session + app config
│   ├── agent.ts                  # Agent metadata + capability gating (sync, no async fetch)
│   └── buddy.ts                  # Buddy widget (frozen — disconnected from UI)
├── hooks/                        # React hooks (useKeyboardShortcuts)
└── lib/                          # tauri.ts, types.ts, utils.ts

src-tauri/src/                    # Rust backend
├── main.rs                       # CLI entry (hook subcommands via clap)
├── lib.rs                        # Tauri app init, plugins, commands registration
├── commands.rs                   # 60+ Tauri invoke handlers
├── pty.rs                        # PTY session management (portable-pty)
├── worktree.rs                   # Git worktree + stash + ship-flow ops
├── db.rs                         # SQLite wrapper (rusqlite, bundled)
├── config.rs                     # Config, paths, defaults
├── setup.rs                      # `cluihud setup` — auto-configure hooks
├── models.rs                     # Shared serde types
├── plan_state.rs                 # Plan FIFO state for blocking hooks
├── hooks/                        # Unix socket server + event types + CLI subcommands
│   ├── cli.rs                    # `cluihud hook send|inject-edits|plan-review|ask-user`
│   ├── server.rs                 # async socket listener + dispatch
│   ├── events.rs                 # event payloads
│   └── state.rs                  # in-memory hook state
├── agents/                       # Agent-agnostic foundation (CC + future OpenCode/Pi/Codex)
│   ├── mod.rs                    # AgentAdapter trait + AgentId + AgentCapability bitflags + Transport
│   ├── registry.rs               # AgentRegistry: register/get/scan + priority list
│   ├── state.rs                  # AgentRuntimeState (registry + cache + typed CC handle)
│   ├── cost_aggregator.rs        # SessionCostAggregator (per-session running totals)
│   └── claude_code/              # First adapter wrapping CC-specific logic
│       ├── mod.rs                # re-exports ClaudeCodeAdapter
│       ├── adapter.rs            # ClaudeCodeAdapter implementing AgentAdapter
│       ├── transcript.rs         # `.jsonl` watcher with notify
│       ├── plan.rs               # plan file watcher + PlanManager
│       ├── cost.rs               # parse_cost_from_transcript (legacy) + parse_cost_line + legacy_usd_for_sonnet4
│       └── tasks_parser.rs       # transcript → TaskStore parser
├── openspec.rs                   # OpenSpec artifact reader (workspace-scoped, agent-agnostic)
├── tasks/                        # TaskStore types (TodoWrite-driven, CC-specific today)
└── terminal/                     # wezterm-term VT session + grid emitter + input
    ├── session.rs                # PTY ↔ VT bridge, grid state
    ├── emitter.rs                # coalesced `terminal:grid-update` events
    ├── differ.rs                 # row-level diff for delta emission
    ├── input.rs                  # keystroke encoding (xterm protocol)
    ├── config.rs, types.rs       # terminal config + shared types
    └── transcript_parser.rs      # in-terminal transcript hooks
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
- Terminal managed outside React: `terminalService.ts` owns canvas + glyph atlas; renders rows on `terminal:grid-update` events from the wezterm-term backend (no xterm.js)
- Keyboard shortcuts use `event.code` not `event.key` (WebKitGTK Linux bug)
- Verify `stores/shortcuts.ts` before proposing new keybindings — collisions silently break existing flows

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
| TS check | `npx tsc --noEmit` |
| Reinstall CLI binary | `cargo install --path src-tauri --force` (run after editing `hooks/cli.rs` so the installed `cluihud` binary picks up changes) |
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

### Plan Review Flow (blocking via PermissionRequest)
1. Claude calls `ExitPlanMode` → `PermissionRequest[ExitPlanMode]` hook fires
2. CLI `cluihud hook plan-review` blocks on a FIFO at `/tmp/cluihud-plan-{pid}.fifo`
3. App loads plan in `AnnotatableMarkdownView` (panel `plan/`); user can add inline annotations during `pending_review` state
4. User accepts → GUI writes `allow` to FIFO → Claude proceeds
5. User rejects → GUI writes `deny` with a Plannotator-style message that points Claude back to the edited plan file → Claude re-reads and re-plans
6. State machine lives in `planReviewStatusMapAtom`: `idle | pending_review | submitted`

### AskUserQuestion Interception
- `PreToolUse[AskUserQuestion]` → `cluihud hook ask-user` (blocking, FIFO)
- GUI shows modal; user-typed answer returned via `permissionDecision: "allow"` + `updatedInput`

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
