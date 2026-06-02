# Architect Brief — obsidian-context-injection

**Project mission**: Nergal is a Linux desktop wrapper for the Claude Code CLI (Tauri 2 + React 19). It runs AROUND the agent CLI in a real PTY, augmenting (not replacing) it via the hook pipeline + transcript watchers. This change adds the *active* half of the vault↔Nergal loop: the agent receives vault knowledge at spawn.

**Status**: artifacts only (2026-06-01). No code written. Implementation deferred to a future session, split into Phase 1 (contract + schema + CC/Codex/OpenCode inject + pin UI) and Phase 2 (N2 hot reload).

## Context

- This is the `obsidian-context-injection` change long deferred from `obsidian-bridge` (#3 + #H + N2). `obsidian-bridge` is archived (`2026-06-01-obsidian-bridge`).
- It is **not** related to the unrelated `context-bridge` change (cross-session CC↔CC comms) — do not touch that.
- The agent-agnostic "context injection at spawn" contract — the historical blocker — is designed here (design.md). It is capability-based across the 4 adapters, verified against 2026 CLI docs (see the research note in design.md). It is **not** CC-only.

## Key decisions (user-confirmed 2026-06-01)

1. **Contract**: three-tier `ContextInjection` capability — `AppendSystemPromptFile` (CC), `PromptPreamble` (Codex/OpenCode), `Unsupported` (Pi for now). Revised from an initial "CC-only" after web research showed Codex/OpenCode accept launch prompts and all four read AGENTS.md.
2. **Storage**: JSON-array column `pinned_note_paths` on `sessions` (migration `010`), pattern of `tasks.blocked_by`. No separate table.
3. **Scope**: artifacts only this iteration. Phase 1 is a verifiable slice; Phase 2 is hot reload.

## Risk tier: critical

- **Migration `010`** — schema change, irreversible once shipped. Additive column, low risk, but it is a migration: bump the latest migration number, follow the `007_agent_id.sql` pattern.
- **Adapter contract change** — `SpawnContext` gains a field; every call site + all 4 adapters must compile. The `injected_context: None` default keeps existing behavior byte-identical (assert in tests).
- **`spawn()` byte-identity when no pins** — a regression here would change every session launch. The "No pinned notes leaves spawn unchanged" scenario is the guard.

## Verification anchors (from the codebase map)

- Contract: `agents/mod.rs` SpawnContext ~L284-292, AgentAdapter trait ~L422-515.
- Adapters: `claude_code/adapter.rs` spawn ~L155 (initial_prompt already wired as positional), `codex/adapter.rs` ~L129, `opencode/adapter.rs` ~L158, `pi/adapter.rs` ~L129.
- Spawn site: `pty.rs::start_claude_session` SpawnContext build ~L331; pending-prompt consume pattern already there (mirror it for pinned-context load).
- Schema: `migrations/` (latest is `009`), `sessions` table in `001_initial.sql`, `Session` model `models.rs` ~L44.
- Watcher template: `obsidian/templates_watcher.rs` (debounced 200ms, `app.emit`).
- UI chip host: `TopBar.tsx` session tab render ~L334-387 (conflict-dot pattern at ~L440); atoms in `stores/workspace.ts`.
- Obsidian config: `obsidian/config.rs::resolve`.

## Phase-1 verification tasks flagged for the builder

- Re-verify OpenCode `--system` flag existence (one 2026 source claims it; CLI reference does not list it). If present, add an `AppendSystemPrompt` variant.
- Confirm whether `pi` accepts a positional launch prompt; if yes, promote Pi from `Unsupported` to `PromptPreamble`.
