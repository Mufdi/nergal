## Why

M3 of `obsidian-bridge` closed the *passive* half of the vault↔Nergal loop: the agent's work flows back to the vault (session log, MOC, backlinks). The *active* half is missing — the agent never receives the vault's knowledge. Today, to give a session context from a note, the user copies its body into the prompt by hand every time, and re-does it on every resume.

`obsidian-bridge` deferred this (`#3`, `#H`) because injection is agent-specific: each CLI (Claude Code, Codex, OpenCode, Pi) exposes a different (or no) mechanism for seeding context at spawn. The agent-agnostic refactor (2026-05-04) never modeled a common "context injection at spawn" contract. This change models that contract — capability-based, so it degrades honestly on agents without native support instead of being Claude-only.

## What Changes

- **Agent-agnostic context-injection contract**: `SpawnContext` gains an assembled `injected_context` block; the `AgentAdapter` trait gains a `context_injection()` capability declaration with three tiers (`AppendSystemPromptFile` | `PromptPreamble` | `Unsupported`). Each adapter folds the context per its 2026 CLI mechanism — CC `--append-system-prompt-file`; Codex/OpenCode prepend to their launch prompt; Pi `Unsupported` until it exposes a per-invocation flag (its `APPEND_SYSTEM.md` is project-scoped). See design.md research note.
- **#H — Pin vault notes to a session** (the persistent superset of #3): a session stores a list of pinned note paths. Pinned notes' bodies are assembled into the injected context at spawn AND re-injected on resume. Visible as a chip in the session tab.
- **#3 — Inject-at-creation picker**: the project-bootstrap / new-session affordance to pin notes before the first spawn is just the pin UI surfaced earlier; one mechanism (pinned notes) covers both #3 and #H.
- **N2 — Hot reload of pinned notes**: a `notify` watcher over the union of pinned paths emits on change; the UI offers an explicit "re-inject updated version" action (never auto, to avoid surprising the running agent).
- **#P — Obsidian reading panel**: a new read-only right-panel (`TabType` "obsidian", TopBar icon + Ctrl+Shift+Q) for reading vault notes inside Nergal without Alt+Tab to Obsidian. It surfaces the session's pinned notes (same `pinned_note_paths` — single source with #H, no new storage) and a query-driven vault finder whose scope defaults to the whole vault and toggles to the configured `search_subdir` with Ctrl+D — identical to the existing vault search modal. Note bodies render via the existing markdown+wikilink pipeline; wikilinks navigate within the panel by default (Ctrl/Cmd+click opens Obsidian). Read-only by design: no graph, no backlinks, no inline edit — editing stays "Open in Obsidian".

## Capabilities

### New Capabilities
- `obsidian-context-injection`: Pin vault notes to a session; inject their bodies as agent context at spawn and on resume via an agent-agnostic capability contract; hot-reload pinned notes with an explicit re-inject action; read pinned + searched vault notes in a dedicated read-only panel that doubles as the pin-management surface.

### Modified Capabilities
<!-- agent-adapter's spawn contract gains injected_context — surfaced as a delta on the agent-adapter spec. -->

## Impact

- **Backend**: `agents/mod.rs` (`SpawnContext.injected_context`, `AgentAdapter::context_injection` + `ContextInjection` enum), the 4 adapters' `spawn()`, `pty.rs` (assemble context from pinned notes at spawn/resume), `db.rs` + migration `010_pinned_notes.sql`, `models.rs` (`Session.pinned_note_paths`), new `obsidian/pinned_notes.rs` (assembler + watcher), new Tauri commands (`pin_vault_note`, `unpin_vault_note`, `list_pinned_notes`, `reinject_pinned_note`). **#P panel** adds two vault-read commands: `read_vault_note(path)` (body for an absolute path guarded under `vault_root` — `read_file_content` is cwd-relative and cannot serve vault paths) and `resolve_vault_note(name)` (wikilink `[[name]]` → first matching `.md` path under `vault_root`, or `None`).
- **Frontend**: `stores/pinnedNotes.ts`, a pin/unpin affordance in the vault search modal + `@@` picker, a session-tab chip listing pinned notes, a hot-reload toast/action. **#P panel**: new `components/obsidian/ObsidianPanel.tsx` (list view = pinned section + query-driven vault finder; reading view = `MarkdownView` body with in-panel wikilink nav); registry wiring in `stores/rightPanel.ts` (`TabType` "obsidian" + `PANEL_CATEGORY_MAP` "document" + `SINGLETON_TYPES`), `TopBar.tsx` (`PANEL_BUTTONS` entry, icon, gated on `obsidianEnabledAtom`), `RightPanel.tsx` (`viewPanelLabel` + `ViewPanelContent` case), `stores/shortcuts.ts` (Ctrl+Shift+Q open), an optional `onWikilinkNavigate` callback on `MarkdownView`.
- **File system**: reads arbitrary vault note paths (already allowed via vault_root); a per-session temp file for the assembled context passed to `--append-system-prompt-file`. The #P panel reads vault `.md` bodies on demand (guarded under `vault_root`); strictly read-only.

## Build contract

### Qué construyo
- `SpawnContext.injected_context` + `AgentAdapter::context_injection()` returning a `ContextInjection` enum (`AppendSystemPromptFile` | `PromptPreamble` | `Unsupported`).
- CC folds via `--append-system-prompt-file <tmp>`; Codex/OpenCode prepend a labeled preamble to their launch prompt; Pi `Unsupported` (verify its launch-prompt arg in Phase 1 — may promote to `PromptPreamble`).
- Migration `010_pinned_notes.sql` (+ `Session.pinned_note_paths: Vec<String>` round-trip in db.rs).
- `obsidian/pinned_notes.rs`: assemble pinned-note bodies into one context block (cap total size) + a `notify` watcher over the pinned set.
- Tauri commands: pin / unpin / list / reinject.
- Frontend: pin affordance (search modal + `@@`), session-tab chip, hot-reload re-inject toast.
- `#P` Obsidian reading panel: `read_vault_note` + `resolve_vault_note` commands; `ObsidianPanel.tsx` (pinned section + vault finder + reading view); panel-registry + TopBar + shortcut wiring; `MarkdownView` `onWikilinkNavigate` hook for in-panel wikilink nav.

### Cómo verifico
- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit` && `pnpm build`
- Manual: pin a note → spawn a CC session → confirm the note body reaches the agent (via `--append-system-prompt-file`); edit the note → re-inject toast; resume → context re-injected; pin on a non-CC session → honest "injection unsupported" hint.
- `#P` panel: open via Ctrl+Shift+Q → pinned notes listed → click renders the body with wikilinks → click a wikilink navigates in-panel → Ctrl+click opens Obsidian → finder defaults to whole vault, Ctrl+D scopes to the configured subdir → pinning from the finder feeds the injection set.

### Criterio de done
- Pinning persists across restart; CC sessions receive pinned-note context at spawn + resume; non-CC sessions pin + show the chip but declare injection unsupported (no silent loss); hot-reload offers an explicit re-inject; the #P panel reads pinned + searched notes, navigates wikilinks in-panel, and shares the pinned set with #H; all gates green.

### Estimated scope
- files_estimate: 24
- risk_tier: critical
- tags: [feature, migration, breaking-change]
- visibility: public
- spec_target: obsidian-context-injection
