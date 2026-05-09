<div align="center">
  <img src="src-tauri/icons/icon.png" alt="Nergal" width="128" />
  <h1>Nergal</h1>
  <p><strong>Keyboard-first Linux desktop HUD that wraps the Claude Code CLI.</strong><br />
  The agent stays in a real terminal at the center; plan, task, git, and activity panels light up live from the hook stream.</p>
</div>

---

## What is Nergal?

Nergal is a thin, opinionated wrapper around an agent CLI. It does not replace the terminal, the agent, or the editor — it spawns the real `claude` (or any supported agent CLI) inside a PTY and listens to the hook pipeline. Hook events flow into Jotai atoms, transcript JSONL files are tailed with inotify, and `PermissionRequest` / `AskUserQuestion` calls block on FIFOs that the GUI writes back to. The terminal itself is canvas-rendered with `wezterm-term` server-side; there is no `xterm.js` anywhere.

The result is a keyboard-first HUD where the agent stays in the centerpiece terminal and everything around it (plan editing, task tracking, git ops, conflict resolution, live preview) reacts in real time without breaking the agent's flow.

## Features

### 🤖 Agent integration

- **Multi-session PTY terminal** — Real `claude` (or other registered CLI agent) running in a PTY. Multiple sessions across multiple workspaces, each with its own state indicator (idle / thinking / working / attention / completed).
- **Plan review with inline annotations** — Blocks `ExitPlanMode`. Surfaces the plan in an annotatable view; approve, or reject with structured feedback that points the agent back to the edited plan file.
- **Live task tracking** — `TodoWrite` events stream into a session-scoped task panel. State and progress visible without scrolling the transcript.
- **Multi-agent support** — Adapter foundation for Claude Code, Codex, OpenCode, and Pi. Each adapter declares its capabilities; the UI gates panels accordingly.

### 🌿 Git workflow

- **Git panel** — Files / History / Stashes / PRs / Conflicts as chip tabs. Stage, commit, pull, push, stash, browse history, review PRs from one surface.
- **Atomic ship-flow** — A single action composes commit (if needed) + push + open PR. Editable preview dialog with title, body, commit list, and base..HEAD diff stat.
- **Three-pane conflict resolution** — Side-by-side ours / theirs / merged editor with one-click "Accept ours / theirs / Ask agent to resolve".
- **Side-by-side diff viewer** — Keyboard-navigable hunks (`j` / `k`), used for file diffs, commit diffs, and PR review.

### 📝 Code & docs

- **File panel with quick editing** — Project tree plus a CodeMirror 6 editor with syntax for TS / JS / JSON / MD / Rust / CSS / HTML. Single-click preview, double-click to pin.
- **OpenSpec viewer** — Read-only viewer of `openspec/` artifacts (proposals, designs, specs, tasks) with the same annotation engine used for plans.
- **Live preview browser** — Embedded iframe panel plus a localhost port scanner. Listening dev servers appear as status-bar chips → click opens the URL.

### ✨ Session UX

- **Floating scratchpad** — Multi-tab notes anchored to a configurable directory. Semi-transparent floating panel that survives across sessions, with content-hash own-write tracking so the watcher does not echo.
- **Activity timeline + DAG graph** — Timeline strip, event list (thinking blocks expandable inline), and an interactive DAG of tool calls for the active session.
- **Theme system** — 13 built-in themes (v1-dark, gothic, dracula, monokai, tokyo-night, …) plus a custom theme editor with live preview.

## Stack

- **Backend** — Rust on Tauri 2.11, tokio, portable-pty, `wezterm-term` (VT emulator), rusqlite (bundled SQLite), notify (inotify), clap, tracing.
- **Frontend** — React 19 + TypeScript, Vite 7, Jotai, TailwindCSS 4 + shadcn/ui + @base-ui/react, CodeMirror 6, react-markdown + `web-highlighter`.
- **Build** — pnpm 10, Vite 7, `tauri-bundler` for `.deb` / `.rpm` / `.AppImage` targets.

See [`docs/architecture.md`](./docs/architecture.md) for the full breakdown.

## Quick start

```bash
git clone https://github.com/Mufdi/nergal.git
cd nergal
pnpm install
pnpm tauri dev
```

For a production build:

```bash
pnpm tauri build
# Bundles in src-tauri/target/release/bundle/{deb,rpm,appimage}/
```

The agent CLI's hook entries point at the `cluihud` binary (the internal name — see *Status* below). Install it on your `PATH` and let Nergal write the hook config:

```bash
cargo install --path src-tauri --force
cluihud setup
```

## Status

**Linux-only.** Tauri's bundle config supports more platforms, but the PTY layer, hook server, FIFO IPC, port scanner, and WebKitGTK chrome assumptions are tested only on Linux today.

**Active development.** Features land in [OpenSpec changes](./openspec/changes/) before they ship as [specs](./openspec/specs/). Expect iteration; the surface is not yet stable.

**Naming.** The user-facing brand is **Nergal**. Internally — the binary, hook subcommands (`cluihud hook ...`), env vars (`CLUIHUD_SESSION_ID`), IPC paths (`/tmp/cluihud.sock`), and config (`~/.config/cluihud/`) — the project keeps the original name `cluihud` for backward compatibility with developer machines already running it. Both names refer to the same project.

## Inspiration

Nergal is shaped by the wider community of Claude Code wrappers, AI pair-programming tools, and worktree managers. A few projects whose work directly informed our thinking:

- [Plannotator](https://github.com/backnotprop/plannotator) — review UI for agent plans and diffs that intercepts `ExitPlanMode` via hooks.
- [Aider](https://aider.chat) — long-running benchmark for AI pair programming, codebase-aware coding agents, and `tree-sitter` repo maps.
- [GitButler](https://gitbutler.com) — opinionated Git client whose parallel-branch philosophy informs our worktree workflow.
- [Spacecake](https://www.spacecake.ai) — the closest stack analogue (Tauri + React + Rust + WebView terminal).
- [Conductor](https://www.conductor.build) — agent-status reporting via lightweight HTTP plus localhost dev-server detection.
- [dmux](https://dmux.ai) — multi-agent orchestration with isolated worktrees and lifecycle hooks.
- [Glass](https://github.com/Glass-HQ/Glass) — unified workspace philosophy and standalone GPU framework.

## Disclaimer

Nergal is an **independent, unaffiliated project**. It is not endorsed by, sponsored by, or otherwise connected to Anthropic, Claude, Claude Code, OpenAI, Codex, the `opencode` project, Pi Code, or any other organization. All product names, trademarks, and registered trademarks referenced here are the property of their respective owners and are used only for interoperability and identification.

## Credits

The Nergal growl sound effect (played when you click the mark in the sidebar) is by [dogwolf123](https://pixabay.com/es/users/dogwolf123-53439420/?utm_source=link-attribution&utm_medium=referral&utm_campaign=music&utm_content=467890), sourced from [Pixabay](https://pixabay.com/sound-effects/?utm_source=link-attribution&utm_medium=referral&utm_campaign=music&utm_content=467890).

## License

[BSD Zero Clause License (`0BSD`)](./LICENSE) — copy, modify, redistribute, sell, ship, fork, vendor, repackage. No attribution required.
