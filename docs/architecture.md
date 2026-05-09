# Architecture

Tauri 2 backend ↔ React 19 frontend over an IPC bridge. The agent CLI runs in a real PTY; React mirrors state through Jotai atoms fed by hook events and transcript watchers.

## Stack

### Backend (Rust via Tauri)
- **Tauri 2.11** — desktop runtime + IPC bridge
- **tokio** — multi-threaded async runtime
- **portable-pty** — PTY session management
- **wezterm-term + termwiz** — VT emulator, pinned to the 2024-02-03 wezterm tag
- **rusqlite** — bundled SQLite for sessions, annotations, panel geometry
- **notify** + **notify-debouncer-full** — inotify file watching
- **clap** — `cluihud hook` subcommands
- **tracing** + **tracing-subscriber** — structured logging
- **reqwest** + **eventsource-stream** — OpenCode SSE adapter
- **bitflags** + **async-trait** + **dashmap** + **parking_lot** — agent registry

### Frontend (React in WebView)
- **React 19** + **TypeScript 5.9**
- **Vite 7** + **pnpm 10**
- **Jotai** — atomic state; shared `appStore` in `stores/jotaiStore.ts` so React Provider and imperative handlers see the same store
- **TailwindCSS 4** + **shadcn/ui** + **@base-ui/react** primitives
- **CodeMirror 6** — file editor + conflict-resolution merged pane
- **react-markdown** + **remark-gfm** + custom `AnnotatableMarkdownView` (no MDXEditor)
- **@xyflow/react** — DAG view in the activity drawer
- **web-highlighter** — annotation overlays

## File tree

```
src/                              # React frontend
├── main.tsx · App.tsx
├── components/
│   ├── layout/    # Workspace, TopBar, Sidebar, RightPanel, StatusBar
│   ├── terminal/  # canvas TerminalManager + terminalService + glyph atlas
│   ├── editor/    # CodeMirror 6 wrapper
│   ├── plan/      # PlanPanel + AnnotatableMarkdownView
│   ├── spec/      # SpecPanel (OpenSpec artifacts)
│   ├── tasks/ · activity/ · session/
│   ├── git/       # GitPanel + chips/ (Files, History, Stashes, PRs, Conflicts) + PrViewer + ConflictsPanel + ShipDialog
│   ├── files/ · panel/ · zen/ · command/
│   ├── ui/ · settings/
├── stores/        # Jotai atoms — workspace, rightPanel, hooks, shortcuts,
│                  # plan, tasks, git, annotations, conflict, ship, zenMode,
│                  # layout, files, activity, askUser, toast, session, config,
│                  # agent, buddy (frozen)
├── hooks/ · lib/

src-tauri/src/                    # Rust backend
├── main.rs        # CLI entry (clap)
├── lib.rs         # Tauri builder + setup
├── commands.rs    # ~110 invoke handlers
├── pty.rs · worktree.rs · db.rs · config.rs · setup.rs · models.rs
├── plan_state.rs  # FIFO state for blocking hooks
├── hooks/         # cli.rs, server.rs, events.rs, state.rs
├── agents/        # AgentAdapter trait + claude_code/ codex/ opencode/ pi/
├── openspec.rs · tasks/ · browser.rs
└── terminal/      # session, emitter, differ, input, transcript_parser
```

## Communication patterns

1. **Frontend → Backend**: `invoke<T>(command, args)` over Tauri IPC.
2. **Backend → Frontend**: Tauri `emit()` events (async).
3. **State sync**: Jotai atoms; `setupHookListeners()` translates Tauri events into atom updates.
4. **Terminal**: `wezterm-term` parses PTY bytes server-side; an emitter coalesces deltas at ~8 ms and emits `terminal:grid-update`. The canvas renders only changed rows via a glyph atlas and sends keystrokes through `invoke("terminal_input")`.

## Event flow

1. Agent CLI runs inside a PTY spawned by the app.
2. The CLI's hooks (async) write events to `/tmp/cluihud.sock` via `cluihud hook send`.
3. The app listens on the socket and watches transcript files via inotify.
4. Events flow through tokio channels → Tauri `emit()` → Jotai atom updates.
5. React components re-render on atom changes.

## Release profile

```toml
[profile.release]
lto = true
codegen-units = 1
panic = "abort"
strip = true
```
