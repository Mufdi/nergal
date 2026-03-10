# cluihud Migration Plan: GPUI → Tauri v2 + React

## Decision Record

**From**: Rust + GPUI + gpui-component + gpui-ghostty (GPU-native)
**To**: Tauri v2 (Rust backend) + React 19 (webview frontend)
**Why**: Better UI flexibility, mature ecosystem, faster iteration, superior documentation
**Performance impact**: Imperceptible for a CLI wrapper app

---

## Stack (Latest Stable Versions — March 2026)

### Backend (Rust — Tauri)
| Dependency | Version | Purpose |
|------------|---------|---------|
| tauri | 2.10.3 | Desktop runtime |
| tauri-build | 2.x | Build script |
| portable-pty | 0.9 | PTY spawn/management (reuse) |
| tokio | 1.x | Async runtime (reuse) |
| notify + notify-debouncer-full | 8 / 0.7 | File watching (reuse) |
| similar | 2 | Diff algorithm (reuse) |
| pulldown-cmark | 0.13 | Markdown parsing (optional, can move to frontend) |
| serde + serde_json | 1 | Serialization (reuse) |
| anyhow | 1 | Error handling (reuse) |
| clap | 4 | CLI subcommands (reuse) |
| dirs | 6 | Path resolution (reuse) |
| tracing + tracing-subscriber | 0.1 / 0.3 | Logging (reuse) |

### Frontend (React — Webview)
| Dependency | Version | Purpose |
|------------|---------|---------|
| react + react-dom | 19.x | UI framework |
| @tauri-apps/api | 2.x | Tauri IPC bridge |
| @tauri-apps/cli | 2.10.1 | Dev tooling |
| vite | 7.x | Build tool + HMR |
| @vitejs/plugin-react | latest | React fast refresh |
| typescript | 5.x | Type safety |
| tailwindcss | 4.x | Styling |
| @xterm/xterm | 6.0.0 | Terminal emulator |
| @xterm/addon-webgl | 6.0.0 | GPU-accelerated rendering |
| @xterm/addon-fit | 6.0.0 | Auto-resize terminal |
| react-markdown | 10.x | Markdown rendering |
| remark-gfm | latest | GitHub-flavored markdown |
| shiki | latest | Syntax highlighting |
| jotai | latest | Lightweight state management |

### Removed (GPUI-specific)
- gpui, gpui-component, gpui-component-assets
- gpui_ghostty_terminal (vendor/gpui-ghostty)
- vendor/zed-xim
- futures (replaced by Tauri async patterns)

---

## Architecture

```
┌══════════════════════════════════════════════════════════════════┐
│                    TAURI v2 APPLICATION                          │
│                                                                  │
│  RUST BACKEND (src-tauri/)              REACT FRONTEND (src/)    │
│  ========================              ====================      │
│                                                                  │
│  ┌──────────────────┐                  ┌────────────────────┐   │
│  │ PtyManager       │   Tauri Events   │ TerminalPanel      │   │
│  │                  │  =============>  │                    │   │
│  │ portable-pty     │  "pty:output"    │ @xterm/xterm       │   │
│  │ spawn(shell)     │  (raw bytes)     │  + addon-webgl     │   │
│  │                  │                  │  + addon-fit        │   │
│  │ read thread ─────┼──→ emit()        │                    │   │
│  │ write handler ◄──┼─── invoke()      │ term.write(bytes)  │   │
│  │ resize handler◄──┼─── invoke()      │ term.onData(cb)    │   │
│  └──────────────────┘                  └────────────────────┘   │
│                                                                  │
│  ┌──────────────────┐                  ┌────────────────────┐   │
│  │ HookManager      │   Tauri Events   │ App State (Jotai)  │   │
│  │                  │  =============>  │                    │   │
│  │ Unix socket      │  "hook:event"    │ PlanPanel          │   │
│  │ listener         │                  │ TaskPanel          │   │
│  │                  │                  │ ActivityLog        │   │
│  │ File watchers    │  Tauri Commands  │ StatusBar          │   │
│  │ (plan, transcript│  <============   │ NavSidebar         │   │
│  │  .jsonl)         │  plan:approve    │ SettingsPanel      │   │
│  └──────────────────┘  plan:reject     └────────────────────┘   │
│                        task:get                                  │
│  ┌──────────────────┐  config:*        ┌────────────────────┐   │
│  │ ConfigManager    │                  │ Layout             │   │
│  │                  │                  │                    │   │
│  │ JSON config      │                  │ Resizable panels   │   │
│  │ Read/Write       │                  │ Multi-tab sessions │   │
│  │ Validation       │                  │ Keyboard shortcuts │   │
│  └──────────────────┘                  └────────────────────┘   │
│                                                                  │
│  ┌──────────────────┐                                           │
│  │ CLI Subcommands  │                                           │
│  │                  │                                           │
│  │ cluihud hook send│                                           │
│  │ cluihud hook     │                                           │
│  │   inject-edits   │                                           │
│  │ cluihud setup    │                                           │
│  └──────────────────┘                                           │
└══════════════════════════════════════════════════════════════════┘
```

### Data Flow: PTY I/O

```
Write: keystroke → xterm.onData() → invoke("pty_write") → pty.write()
Read:  pty.read() → emit("pty-output", bytes) → listen() → term.write()
Resize: addon-fit → invoke("pty_resize") → pty.resize() → SIGWINCH
```

### Data Flow: Hook Events

```
Claude CLI → cluihud hook send <event> → Unix socket
  → HookManager (tokio) → emit("hook:event", payload)
  → React listener → Jotai store update → component re-render
```

---

## Project Structure

```
cluihud/
├── src-tauri/                    # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   ├── src/
│   │   ├── lib.rs                # Tauri plugin setup, command registration
│   │   ├── main.rs               # Entry point (Tauri + Clap CLI)
│   │   ├── pty.rs                # PtyManager: spawn, read, write, resize
│   │   ├── hooks/
│   │   │   ├── mod.rs
│   │   │   ├── server.rs         # Unix socket listener (reuse)
│   │   │   ├── events.rs         # HookEvent enum (reuse)
│   │   │   ├── cli.rs            # CLI subcommands (reuse)
│   │   │   └── state.rs          # Pending plan state (reuse)
│   │   ├── claude/
│   │   │   ├── mod.rs
│   │   │   ├── plan.rs           # PlanManager (reuse)
│   │   │   ├── transcript.rs     # Transcript watcher (reuse)
│   │   │   └── cost.rs           # Cost calculator (reuse)
│   │   ├── tasks/
│   │   │   ├── mod.rs            # TaskStore (reuse)
│   │   │   └── transcript_parser.rs  # (reuse)
│   │   ├── config.rs             # Config manager (reuse)
│   │   └── setup.rs              # Hook auto-setup (reuse)
│   └── icons/
├── src/                          # React frontend
│   ├── main.tsx                  # React entry point
│   ├── App.tsx                   # Root layout + routing
│   ├── stores/
│   │   ├── session.ts            # Session state (Jotai atoms)
│   │   ├── tasks.ts              # Task list state
│   │   ├── plan.ts               # Plan state
│   │   └── hooks.ts              # Hook event listeners setup
│   ├── components/
│   │   ├── layout/
│   │   │   ├── Workspace.tsx     # Main layout with resizable panels
│   │   │   ├── NavSidebar.tsx    # Session switcher
│   │   │   └── StatusBar.tsx     # Bottom bar
│   │   ├── terminal/
│   │   │   ├── TerminalPanel.tsx # xterm.js container
│   │   │   └── useTerminal.ts   # Hook: PTY IPC + xterm lifecycle
│   │   ├── plan/
│   │   │   ├── PlanPanel.tsx     # View/Edit/Diff modes
│   │   │   ├── MarkdownView.tsx  # react-markdown renderer
│   │   │   ├── PlanEditor.tsx    # Editing with comments
│   │   │   └── DiffView.tsx      # Side-by-side diff
│   │   ├── tasks/
│   │   │   ├── TaskPanel.tsx     # Task list
│   │   │   └── TaskItem.tsx      # Individual task row
│   │   ├── activity/
│   │   │   └── ActivityLog.tsx   # Event feed
│   │   └── settings/
│   │       └── SettingsPanel.tsx # Config UI
│   ├── lib/
│   │   ├── tauri.ts              # Typed invoke/listen wrappers
│   │   └── types.ts              # Shared TypeScript types
│   └── styles/
│       └── globals.css           # Tailwind imports + theme vars
├── index.html                    # Vite entry
├── package.json
├── tsconfig.json
├── vite.config.ts
├── tailwind.config.ts
└── CLAUDE.md                     # Updated project instructions
```

---

## Migration Phases

### Phase 0: Scaffold Tauri + React project
- `cargo install tauri-cli`
- `npm create tauri-app@latest` with React + Vite + TypeScript template
- Configure `tauri.conf.json` (window size, title, permissions)
- Verify empty app builds and launches on Linux
- Set up Tailwind CSS v4
- **Deliverable**: Empty Tauri+React app that opens a window

### Phase 1: Port backend (Rust logic)
Migrate portable Rust code (~950 lines) to `src-tauri/src/`:
- `hooks/*` → copy directly, adapt to emit Tauri events instead of std_mpsc
- `claude/*` → copy directly (plan.rs, transcript.rs, cost.rs)
- `tasks/*` → copy directly (mod.rs, transcript_parser.rs)
- `config.rs` → copy, add Tauri command wrappers
- `setup.rs` → copy directly
- Register Tauri commands: `get_config`, `save_config`, `get_tasks`, `approve_plan`, `reject_plan`, etc.
- **Deliverable**: Backend compiles, CLI subcommands work

### Phase 2: Terminal (PTY + xterm.js)
- Implement `pty.rs`: PtyManager with `create`, `write`, `resize`, `kill`
- Expose as Tauri commands + events
- Frontend: `useTerminal` hook connecting xterm.js ↔ Tauri IPC
- TerminalPanel component with WebGL addon + fit addon
- Multi-tab support (multiple PTY instances)
- **Deliverable**: Working terminal that can run shell commands

### Phase 3: Hook pipeline + session detection
- Start Unix socket listener on app launch
- Forward hook events as Tauri events to frontend
- React listeners update Jotai atoms
- Session detection: SessionStart activates panels, SessionEnd deactivates
- Status bar shows session info, mode, cost
- **Deliverable**: App detects when Claude starts/stops

### Phase 4: Task panel
- TaskPanel component reading from Jotai task store
- Real-time updates from hook events + transcript parsing
- Task detail expansion on click
- Status indicators (pending → in_progress → completed)
- **Deliverable**: Live task tracking during Claude sessions

### Phase 5: Plan panel + editing
- MarkdownView with react-markdown + remark-gfm + shiki
- PlanEditor with inline comment support (`<!-- COMMENT: ... -->`)
- DiffView using `similar` results from backend
- 4-option approval workflow (same as current)
- UserPromptSubmit hook injection for reject-with-edits
- **Deliverable**: Full plan editing flow working

### Phase 6: Activity log + settings
- ActivityLog component (chronological event feed, max 200)
- SettingsPanel with form inputs (paths, shell, theme)
- Config persistence via Tauri commands
- Theme support (dark/light)
- **Deliverable**: Complete feature parity with GPUI version

### Phase 7: Polish + keyboard shortcuts
- Keyboard shortcuts (Ctrl+1/2/3 panels, Ctrl+T/W tabs, Ctrl+Y/N plan)
- Resizable panels (CSS resize or react-resizable-panels)
- Window state persistence (size, position, panel widths)
- Desktop entry + icon
- Release build optimization
- **Deliverable**: Production-ready app

---

## Tauri Commands (API Surface)

```rust
// PTY
#[tauri::command] fn pty_create(id: String, cwd: String) -> Result<()>
#[tauri::command] fn pty_write(id: String, data: Vec<u8>) -> Result<()>
#[tauri::command] fn pty_resize(id: String, cols: u16, rows: u16) -> Result<()>
#[tauri::command] fn pty_kill(id: String) -> Result<()>

// Config
#[tauri::command] fn get_config() -> Result<Config>
#[tauri::command] fn save_config(config: Config) -> Result<()>

// Plan
#[tauri::command] fn get_plan(path: String) -> Result<String>
#[tauri::command] fn save_plan(path: String, content: String) -> Result<()>
#[tauri::command] fn diff_plan(original: String, modified: String) -> Result<Vec<DiffLine>>
#[tauri::command] fn approve_plan(session_id: String, option: ApprovalOption) -> Result<()>
#[tauri::command] fn reject_plan(session_id: String, feedback: String) -> Result<()>

// Tasks
#[tauri::command] fn get_tasks(session_id: String) -> Result<Vec<Task>>

// Session
#[tauri::command] fn get_sessions() -> Result<Vec<SessionInfo>>

// Setup
#[tauri::command] fn setup_hooks() -> Result<()>
```

## Tauri Events (Backend → Frontend)

```rust
app.emit("pty:output", PtyOutput { id, data: Vec<u8> })
app.emit("hook:event", HookEvent { session_id, event_type, payload })
app.emit("plan:ready", PlanReady { session_id, path })
app.emit("session:start", SessionStart { session_id, cwd })
app.emit("session:end", SessionEnd { session_id })
app.emit("task:update", TaskUpdate { session_id, task })
app.emit("cost:update", CostUpdate { session_id, input_tokens, output_tokens, total_cost })
```

---

## Terminal: Future ghostty-web Upgrade Path

Current plan uses xterm.js 6.0 (stable, proven). When ghostty-web matures (currently 0.4.0 alpha):

1. `npm install ghostty-web`
2. Copy `ghostty-vt.wasm` to public/
3. In `useTerminal.ts`, swap xterm.js Terminal for ghostty-web Terminal
4. API is compatible — same `term.write()`, `term.onData()`, `term.resize()`

No architectural changes needed. The swap is isolated to the terminal component.

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| WebKitGTK canvas performance | Low | Medium | WebGL addon, test early |
| Tauri IPC throughput for PTY | Low | High | Binary payloads, buffer coalescing |
| xterm.js input latency | Very Low | Low | ~16ms vs ~5ms, imperceptible |
| Feature parity gaps | Medium | Medium | Phase-by-phase validation |
| Linux-specific WebView bugs | Low | Medium | Test on Ubuntu + Arch |

---

## What Gets Better

- Hot reload during development (Vite HMR)
- DevTools for debugging UI
- CSS/Tailwind for styling (vs programmatic div().flex())
- React ecosystem (thousands of components)
- Markdown rendering (react-markdown + plugins vs manual pulldown-cmark → GPUI)
- Settings UI (HTML forms vs custom GPUI components)
- Documentation and community support
- Easier to contribute (web skills vs GPUI knowledge)

## What We Lose

- Single-language stack (now Rust + TypeScript)
- ~5ms native input latency (now ~16ms, imperceptible)
- GPU-native rendering (now WebView, still GPU-accelerated via WebGL)
- Smaller binary (Tauri includes WebView runtime)
