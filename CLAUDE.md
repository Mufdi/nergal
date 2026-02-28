# cluihud

Desktop app wrapper para Claude Code CLI (Plan Pro) en Linux.
GPU-accelerated UI con GPUI Component (framework de Zed).

## Project Documentation

- Design doc completo: `/home/user/Documents/Obsidian/Projects/cluihud/cluihud.md`

## Stack

- **Language**: Rust (edition 2024)
- **UI Framework**: GPUI + gpui-component (60+ components, GPU-accelerated)
- **Async**: tokio (multi-threaded runtime)
- **Terminal embed**: portable-pty
- **Markdown**: pulldown-cmark
- **File watching**: notify (inotify backend)
- **Diffing**: similar
- **Serialization**: serde + serde_json

## Architecture

GPUI Entity system: Entity<T> → Render trait → reactive updates via cx.notify()

```
src/
├── main.rs              # Application::new(), gpui_component::init, open_window
├── app.rs               # AppState, Root view wrapper
├── workspace.rs         # Workspace con dock: terminal + plan + tasks
├── ui/
│   ├── mod.rs           # Re-exports
│   ├── terminal_panel.rs # Terminal PTY panel (portable-pty + GPUI render)
│   ├── plan_panel.rs    # Plan editor/viewer (markdown)
│   ├── task_panel.rs    # Task list panel
│   └── status_bar.rs    # Bottom status bar
├── hooks/
│   ├── mod.rs           # Re-exports
│   ├── server.rs        # Unix socket listener
│   └── events.rs        # HookEvent enum + serde
├── claude/
│   ├── mod.rs           # Re-exports
│   ├── session.rs       # Claude CLI PTY session
│   ├── transcript.rs    # .jsonl watcher + parser
│   └── plan.rs          # Plan file manager
└── config.rs            # Config, paths, defaults
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

### GPUI Patterns (enforced by gpui-component skills)
- `Render` trait for stateful views with Entity<T>
- `RenderOnce` for stateless consumable elements
- `div().flex().flex_col()` / `h_flex()` / `v_flex()` for layout
- `cx.listener(Self::method)` for event handlers
- `cx.notify()` after state mutations to trigger re-render
- `WeakEntity` in closures to avoid retain cycles
- `cx.subscribe()` / `cx.observe()` for inter-entity communication

### Project Conventions
- No TODO/FIXME — track in issues
- Comments only for WHY, never WHAT
- Absolute paths in tool calls
- Parallel independent operations

## Verification Commands

| Action | Command |
|--------|---------|
| Build | `cargo build` |
| Test | `cargo test` |
| Lint | `cargo clippy -- -D warnings` |
| Format | `cargo fmt --check` |
| Full check | `cargo clippy -- -D warnings && cargo test && cargo fmt --check` |

Run full check after significant changes.

## Key Concepts

### Event Flow
1. Claude CLI runs inside a PTY spawned by the app
2. Hooks (async) write events to a Unix socket
3. App listens on the socket + watches transcript files via inotify
4. Events flow through tokio mpsc channels to update app state
5. Entity state changes trigger GPUI re-renders via cx.notify()

### Plan Editing Flow (bidirectional)
1. Claude writes plan to `plansDirectory`
2. `PreToolUse[ExitPlanMode]` hook notifies the app
3. App loads plan in editor panel
4. User edits + adds inline comments
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

## System Dependencies

```bash
sudo apt-get install -y libxkbcommon-x11-dev libxcb1-dev libxkbcommon-dev
```

## Release Profile

```toml
[profile.release]
lto = true
codegen-units = 1
panic = "abort"
strip = true
```
