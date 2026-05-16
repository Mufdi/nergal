# Nergal

Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers.

## Naming — IMPORTANT

User-facing brand: **Nergal** (productName, identifier `com.nergal.app`, GitHub repo `Mufdi/nergal`).

Internal name **cluihud** is preserved everywhere else for backward compatibility with the developer machine: binary `cluihud`, hook subcommands (`cluihud hook ...`), env vars (`CLUIHUD_SESSION_ID`, `CLUIHUD_AGENT_ID`), IPC paths (`/tmp/cluihud.sock`, `/tmp/cluihud-plan-*.fifo`, `/tmp/cluihud-ask-*.fifo`), sentinel (`~/.cluihud-active`), config dir (`~/.config/cluihud/`), local repo dir name (`cluihud/`). **Do NOT rename internal `cluihud` references.**

## Scope — qué es y qué no es Nergal

Nergal corre **alrededor** del agente CLI, no en su lugar. Esto define el filtro de inspiración, no una lista cerrada de features.

**Estable (no negociable):**
- Siempre corre `claude` (u otro agent CLI) underneath en un PTY real. No reemplaza bash/zsh/tmux.
- No reimplementa primitives nativos del agente (slash commands, skills, agents, hooks como motor). Los **observa y augmenta** vía hook pipeline + transcript watchers.

**Abierto (evolutivo):**
- **Multi-agent / agent-agnostic** ya es estable (4 OpenSpec changes archivados 2026-05-04). BYOA, coordinator patterns, parallel agent comparisons, switching entre CC / Codex / Gemini → bienvenidos.
- **Surfaces alrededor del ecosistema del agente**: skills marketplace para discovery/install, MCP server propio para que el agente consulte cluihud, usage dashboards, deep-link protocol — todos válidos. La línea está en "no reescribir lo que el agente ya hace", no en "ignorar el ecosistema".
- **Workflow integrations** (issue trackers, design tools, browser preview, voice input, Docker isolation) son evoluciones naturales si reducen fricción en el loop agente↔humano.

**Filtro para evaluar inspiración:**
- ¿Replica primitives del agente (tool calling propio, skill emergence, training loops)? → redirigir.
- ¿Augmenta el loop o organiza el ecosistema alrededor? → evaluar, no descartar por scope-creep.
- Tooling completamente ajeno (ej. IDEs, design tools, terminal-only utilities) puede aportar **patrones de UX** aplicables aunque la herramienta no se parezca a Nergal. La fuente es señal débil; la idea es la señal fuerte.

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
