# Nergal

Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers.

## Naming — IMPORTANT

User-facing brand: **Nergal** (productName, identifier `com.nergal.app`, GitHub repo `Mufdi/nergal`).

Internal name **cluihud** is preserved everywhere else for backward compatibility with the developer machine: binary `cluihud`, hook subcommands (`cluihud hook ...`), env vars (`CLUIHUD_SESSION_ID`, `CLUIHUD_AGENT_ID`), IPC paths (`/tmp/cluihud.sock`, `/tmp/cluihud-plan-*.fifo`, `/tmp/cluihud-ask-*.fifo`), sentinel (`~/.cluihud-active`), config dir (`~/.config/cluihud/`), local repo dir (`cluihud/`). **Do NOT rename internal `cluihud` references.**

## What Nergal is NOT

- **NOT a standalone terminal.** It always runs `claude` (or another supported CLI) underneath. It does not replace bash/zsh/tmux.
- **NOT a Claude Code reimplementation.** It does not reimplement slash commands, skills, agents, hooks. It **observes** the session and **augments** the surrounding UX.
- **NOT an agent framework.** No training loops, no skill emergence, no model orchestration.

Recommendations should improve the experience of using the agent CLI (plan editing UX, task visibility, session navigation, hook-driven panels, workspace/worktree management). Recommendations that replace agent-native features should be redirected.

## Critical conventions

- **Read before Write/Edit.** Always read files before modifying.
- **Comments: WHY only, never WHAT.** Document non-obvious constraints, workarounds, invariants. Restating the next line is not a comment.
- **Keyboard shortcuts use `event.code`** (not `event.key`) — WebKitGTK Linux bug. Verify `src/stores/shortcuts.ts` before adding a binding (collisions silently break flows).
- **No `unwrap()` / `expect()` in Rust outside tests.** Propagate with `anyhow` and `?`.
- **No TODO/FIXME** — track in issues or OpenSpec changes.
- **Absolute paths in tool calls.**

## Verification commands

| Action | Command |
|---|---|
| Dev | `pnpm tauri dev` |
| Build | `pnpm tauri build` |
| Rust check | `cd src-tauri && cargo check` |
| Rust test | `cd src-tauri && cargo test` |
| Rust lint | `cd src-tauri && cargo clippy -- -D warnings` |
| Rust format | `cd src-tauri && cargo fmt --check` |
| TS check | `npx tsc --noEmit` |
| Full check | `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit` |
| Reinstall CLI binary | `cargo install --path src-tauri --force` (after editing `src-tauri/src/hooks/cli.rs`) |

Run the full check after significant changes.

## Documentation TOC

Read on demand when working in the relevant area:

- [`docs/architecture.md`](./docs/architecture.md) — stack, file tree, IPC patterns, event flow.
- [`docs/conventions.md`](./docs/conventions.md) — Rust + React/TS coding standards.
- [`docs/hooks.md`](./docs/hooks.md) — hook system, plan-review flow, ask-user interception, settings.json snippet.
- [`docs/design.md`](./docs/design.md) — design system: tokens, components, decision rules. Read before touching UI.
- [`openspec/specs/`](./openspec/specs/) — feature contracts. Read the relevant spec before implementing or proposing a change.

## Project hub (external)

`internal vault reference removed`. CLAUDE.md is the source of truth; the vault note is referential.
