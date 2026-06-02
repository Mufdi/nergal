# Tasks — obsidian-context-injection

Status: **artifacts only** (2026-06-01). Implementation deferred to a future session. Phases let the future builder land Phase 1 as a verifiable slice before Phase 2.

## Phase 1 — Contract + schema + CC/Codex/OpenCode inject + pin UI

### 1.1 Contract (agents/mod.rs)
- [ ] 1.1.1 Add `injected_context: Option<&'a str>` to `SpawnContext` (default `None` at all existing call sites).
- [ ] 1.1.2 Add `ContextInjection` enum (`AppendSystemPromptFile` | `PromptPreamble` | `Unsupported`).
- [ ] 1.1.3 Add `fn context_injection(&self) -> ContextInjection { Unsupported }` to the `AgentAdapter` trait (default keeps untouched adapters safe).

### 1.2 Per-adapter spawn folding
- [ ] 1.2.1 CC: `context_injection() = AppendSystemPromptFile`; in `spawn()`, when `injected_context` is `Some`, write it to `~/.config/cluihud/spawn-context/<session_id>.md` and push `--append-system-prompt-file <path>`.
- [ ] 1.2.2 Codex: `context_injection() = PromptPreamble`; prepend a fenced/labeled context block to the positional `PROMPT` (combine with `initial_prompt` if both present).
- [ ] 1.2.3 OpenCode: re-verify whether the shipped CLI has a `--system` flag. If yes → add an `AppendSystemPrompt` variant + use it. If no → `PromptPreamble` via `--prompt`.
- [ ] 1.2.4 Pi: confirm whether `pi` accepts a positional launch prompt. If yes → `PromptPreamble`; else leave `Unsupported` (default) with a code comment citing `.pi/APPEND_SYSTEM.md` as the project-scoped-only channel.

### 1.3 Storage (db.rs + migration + models.rs)
- [ ] 1.3.1 Migration `010_pinned_notes.sql`: `ALTER TABLE sessions ADD COLUMN pinned_note_paths TEXT;`.
- [ ] 1.3.2 `Session.pinned_note_paths: Vec<String>` + JSON round-trip in db.rs (pattern: `tasks.blocked_by`).
- [ ] 1.3.3 DB methods: `add_pinned_note(session_id, path)`, `remove_pinned_note(session_id, path)`, `get_pinned_notes(session_id) -> Vec<String>` (dedup, preserve order).

### 1.4 Assembly (obsidian/pinned_notes.rs)
- [ ] 1.4.1 `assemble_context(paths: &[String], vault_root: Option<&str>) -> Option<String>`: read each note (skip missing), wrap each in `## [[Title]]\n<body>` under a `# Pinned vault context` header.
- [ ] 1.4.2 Cap total at 64KB; on overflow include notes until the budget is hit + append a truncation marker. `tracing::warn!` what was dropped (no silent cap — RULES).
- [ ] 1.4.3 Unit tests: empty list → None, single note, multi-note ordering, oversize truncation marker.

### 1.5 Spawn + resume wiring (pty.rs)
- [ ] 1.5.1 In `start_claude_session`, after `initial_prompt` resolution: load `get_pinned_notes`, `assemble_context`, set `ctx.injected_context` (runs for both fresh + resume → re-inject on resume is automatic).
- [ ] 1.5.2 Clean up the per-session spawn-context temp file on `kill_session_pty`.

### 1.6 Tauri commands
- [ ] 1.6.1 `pin_vault_note(session_id, path)`, `unpin_vault_note(session_id, path)`, `list_pinned_notes(session_id) -> Vec<String>`. Register in `lib.rs` invoke_handler.

### 1.7 Frontend — pin affordance + chip
- [ ] 1.7.1 `stores/pinnedNotes.ts`: `pinnedNotesMapAtom: Record<sessionId, string[]>`, load/pin/unpin actions invoking the commands.
- [ ] 1.7.2 "Pin to session" action in `VaultSearchModal` + the `@@` picker (`MentionPickerOverlay` consumers).
- [ ] 1.7.3 Session-tab chip in `TopBar.tsx` (pattern: conflict dot): pinned count, hover lists names, click → popover to unpin.
- [ ] 1.7.4 Non-CC honesty: when the active session's adapter `context_injection()` is `Unsupported`, the chip tooltip says "injection unsupported for <agent>"; `PromptPreamble` tooltip notes "injected as first message". Needs a `get_context_injection_tier(session_id)` command or expose it on the session payload.

### 1.8 Phase 1 gates
- [ ] 1.8.1 `cargo clippy -- -D warnings && cargo test && cargo fmt --check && npx tsc --noEmit && pnpm build`.
- [ ] 1.8.2 Manual: pin → spawn CC → confirm context reaches the agent; resume → re-injected; Codex session → preamble in first turn; non-supported → honest tooltip.

## Phase 2 — N2 hot reload

### 2.1 Watcher (obsidian/pinned_notes.rs)
- [ ] 2.1.1 `PinnedNotesWatcher` (template: `templates_watcher.rs`, debounced 200ms) over the union of all sessions' pinned paths; managed state in `lib.rs` like `TemplatesWatcherState`.
- [ ] 2.1.2 On change → emit `vault:pinned-note-changed { session_id, path }`. Rewatch when pins change.

### 2.2 Re-inject action
- [ ] 2.2.1 `reinject_pinned_note(session_id, path)` command: re-read the note, write a single labeled block into the live PTY (post-spawn re-inject = PTY write, like `ConflictsPanel`).
- [ ] 2.2.2 Frontend listener → Sileo toast with an explicit "Re-inject updated version" action (never auto).

### 2.3 Phase 2 gates
- [ ] 2.3.1 Full gate suite + manual: edit a pinned note in Obsidian → toast → re-inject → confirm the live agent sees the update.
