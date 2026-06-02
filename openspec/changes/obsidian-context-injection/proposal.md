## Why

M3 of `obsidian-bridge` closed the *passive* half of the vault↔Nergal loop: the agent's work flows back to the vault (session log, MOC, backlinks). The *active* half is missing — the agent never receives the vault's knowledge. Today, to give a session context from a note, the user copies its body into the prompt by hand every time, and re-does it on every resume.

`obsidian-bridge` deferred this (`#3`, `#H`) because injection is agent-specific: each CLI (Claude Code, Codex, OpenCode, Pi) exposes a different (or no) mechanism for seeding context at spawn. The agent-agnostic refactor (2026-05-04) never modeled a common "context injection at spawn" contract. This change models that contract — capability-based, so it degrades honestly on agents without native support instead of being Claude-only.

## What Changes

- **Agent-agnostic context-injection contract**: `SpawnContext` gains an assembled `injected_context` block; the `AgentAdapter` trait gains a `context_injection()` capability declaration with three tiers (`AppendSystemPromptFile` | `PromptPreamble` | `Unsupported`). Each adapter folds the context per its 2026 CLI mechanism — CC `--append-system-prompt-file`; Codex/OpenCode prepend to their launch prompt; Pi `Unsupported` until it exposes a per-invocation flag (its `APPEND_SYSTEM.md` is project-scoped). See design.md research note.
- **#H — Pin vault notes to a session** (the persistent superset of #3): a session stores a list of pinned note paths. Pinned notes' bodies are assembled into the injected context at spawn AND re-injected on resume. Visible as a chip in the session tab.
- **#3 — Inject-at-creation picker**: the project-bootstrap / new-session affordance to pin notes before the first spawn is just the pin UI surfaced earlier; one mechanism (pinned notes) covers both #3 and #H.
- **N2 — Hot reload of pinned notes**: a `notify` watcher over the union of pinned paths emits on change; the UI offers an explicit "re-inject updated version" action (never auto, to avoid surprising the running agent).

## Capabilities

### New Capabilities
- `obsidian-context-injection`: Pin vault notes to a session; inject their bodies as agent context at spawn and on resume via an agent-agnostic capability contract; hot-reload pinned notes with an explicit re-inject action.

### Modified Capabilities
<!-- agent-adapter's spawn contract gains injected_context — surfaced as a delta on the agent-adapter spec. -->

## Impact

- **Backend**: `agents/mod.rs` (`SpawnContext.injected_context`, `AgentAdapter::context_injection` + `ContextInjection` enum), the 4 adapters' `spawn()`, `pty.rs` (assemble context from pinned notes at spawn/resume), `db.rs` + migration `010_pinned_notes.sql`, `models.rs` (`Session.pinned_note_paths`), new `obsidian/pinned_notes.rs` (assembler + watcher), new Tauri commands (`pin_vault_note`, `unpin_vault_note`, `list_pinned_notes`, `reinject_pinned_note`).
- **Frontend**: `stores/pinnedNotes.ts`, a pin/unpin affordance in the vault search modal + `@@` picker, a session-tab chip listing pinned notes, a hot-reload toast/action.
- **File system**: reads arbitrary vault note paths (already allowed via vault_root); a per-session temp file for the assembled context passed to `--append-system-prompt-file`.

## Build contract

### Qué construyo
- `SpawnContext.injected_context` + `AgentAdapter::context_injection()` returning a `ContextInjection` enum (`AppendSystemPromptFile` | `PromptPreamble` | `Unsupported`).
- CC folds via `--append-system-prompt-file <tmp>`; Codex/OpenCode prepend a labeled preamble to their launch prompt; Pi `Unsupported` (verify its launch-prompt arg in Phase 1 — may promote to `PromptPreamble`).
- Migration `010_pinned_notes.sql` (+ `Session.pinned_note_paths: Vec<String>` round-trip in db.rs).
- `obsidian/pinned_notes.rs`: assemble pinned-note bodies into one context block (cap total size) + a `notify` watcher over the pinned set.
- Tauri commands: pin / unpin / list / reinject.
- Frontend: pin affordance (search modal + `@@`), session-tab chip, hot-reload re-inject toast.

### Cómo verifico
- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit` && `pnpm build`
- Manual: pin a note → spawn a CC session → confirm the note body reaches the agent (via `--append-system-prompt-file`); edit the note → re-inject toast; resume → context re-injected; pin on a non-CC session → honest "injection unsupported" hint.

### Criterio de done
- Pinning persists across restart; CC sessions receive pinned-note context at spawn + resume; non-CC sessions pin + show the chip but declare injection unsupported (no silent loss); hot-reload offers an explicit re-inject; all gates green.

### Estimated scope
- files_estimate: 16
- risk_tier: critical
- tags: [feature, migration, breaking-change]
- visibility: public
- spec_target: obsidian-context-injection
