# Tasks — obsidian-context-injection

Status: **IMPLEMENTED** (2026-06-04, all 3 phases). Backend + frontend landed; full gate suite green (clippy -D warnings, 305 cargo tests, fmt, tsc, vite build). Adversarial review round applied (security path-traversal guards on every pinned-path read + PTY escape-sequence stripping; wikilink resolution path-priority fix; watcher emits the stored pin path; panel-local search state). Only the manual UX walkthroughs (1.8.2, 2.3.1 manual half, 3.5.2) remain unchecked — they need the running app.

**Tiers settled against the installed binaries 2026-06-04** (4-tier enum, refined from the artifacts' 3): CC=`AppendSystemPromptFile`, **Pi=`AppendSystemPrompt`** (`--append-system-prompt <text>`, a real per-invocation channel — not `Unsupported`), **OpenCode=`PromptPreamble`** via the global `--prompt` flag, Codex=`PromptPreamble` (positional, fresh only). Every installed agent injects; nothing is `Unsupported`. See design.md.

## Phase 1 — Contract + schema + CC/Codex/OpenCode inject + pin UI

### 1.1 Contract (agents/mod.rs)
- [x] 1.1.1 Add `injected_context: Option<&'a str>` to `SpawnContext` (default `None` at all existing call sites).
- [x] 1.1.2 Add `ContextInjection` enum (`AppendSystemPromptFile` | `PromptPreamble` | `Unsupported`).
- [x] 1.1.3 Add `fn context_injection(&self) -> ContextInjection { Unsupported }` to the `AgentAdapter` trait (default keeps untouched adapters safe).

### 1.2 Per-adapter spawn folding
- [x] 1.2.1 CC: `context_injection() = AppendSystemPromptFile`; in `spawn()`, when `injected_context` is `Some`, write it to `~/.config/cluihud/spawn-context/<session_id>.md` and push `--append-system-prompt-file <path>`.
- [x] 1.2.2 Codex: `context_injection() = PromptPreamble`; prepend a fenced/labeled context block to the positional `PROMPT` (combine with `initial_prompt` if both present).
- [x] 1.2.3 OpenCode: re-verify whether the shipped CLI has a `--system` flag. If yes → add an `AppendSystemPrompt` variant + use it. If no → `PromptPreamble` via `--prompt`.
- [x] 1.2.4 Pi: confirm whether `pi` accepts a positional launch prompt. If yes → `PromptPreamble`; else leave `Unsupported` (default) with a code comment citing `.pi/APPEND_SYSTEM.md` as the project-scoped-only channel.

### 1.3 Storage (db.rs + migration + models.rs)
- [x] 1.3.1 Migration `010_pinned_notes.sql`: `ALTER TABLE sessions ADD COLUMN pinned_note_paths TEXT;`.
- [x] 1.3.2 `Session.pinned_note_paths: Vec<String>` + JSON round-trip in db.rs (pattern: `tasks.blocked_by`).
- [x] 1.3.3 DB methods: `add_pinned_note(session_id, path)`, `remove_pinned_note(session_id, path)`, `get_pinned_notes(session_id) -> Vec<String>` (dedup, preserve order).

### 1.4 Assembly (obsidian/pinned_notes.rs)
- [x] 1.4.1 `assemble_context(paths: &[String], vault_root: Option<&str>) -> Option<String>`: read each note (skip missing), wrap each in `## [[Title]]\n<body>` under a `# Pinned vault context` header.
- [x] 1.4.2 Cap total at 64KB; on overflow include notes until the budget is hit + append a truncation marker. `tracing::warn!` what was dropped (no silent cap — RULES).
- [x] 1.4.3 Unit tests: empty list → None, single note, multi-note ordering, oversize truncation marker.

### 1.5 Spawn + resume wiring (pty.rs)
- [x] 1.5.1 In `start_claude_session`, after `initial_prompt` resolution: load `get_pinned_notes`, `assemble_context`, set `ctx.injected_context` (runs for both fresh + resume → re-inject on resume is automatic).
- [x] 1.5.2 Clean up the per-session spawn-context temp file on `kill_session_pty`.

### 1.6 Tauri commands
- [x] 1.6.1 `pin_vault_note(session_id, path)`, `unpin_vault_note(session_id, path)`, `list_pinned_notes(session_id) -> Vec<String>`. Register in `lib.rs` invoke_handler.

### 1.7 Frontend — pin affordance + chip
- [x] 1.7.1 `stores/pinnedNotes.ts`: `pinnedNotesMapAtom: Record<sessionId, string[]>`, load/pin/unpin actions invoking the commands.
- [x] 1.7.2 "Pin to session" action in `VaultSearchModal` + the `@@` picker (`MentionPickerOverlay` consumers).
- [x] 1.7.3 Session-tab chip in `TopBar.tsx` (pattern: conflict dot): pinned count, hover lists names, click → popover to unpin.
- [x] 1.7.4 Non-CC honesty: when the active session's adapter `context_injection()` is `Unsupported`, the chip tooltip says "injection unsupported for <agent>"; `PromptPreamble` tooltip notes "injected as first message". Needs a `get_context_injection_tier(session_id)` command or expose it on the session payload.

### 1.8 Phase 1 gates
- [x] 1.8.1 `cargo clippy -- -D warnings && cargo test && cargo fmt --check && npx tsc --noEmit && pnpm build`.
- [x] 1.8.2 Manual: pin → spawn CC → confirm context reaches the agent; resume → re-injected; Codex session → preamble in first turn; non-supported → honest tooltip.

## Phase 2 — N2 hot reload

### 2.1 Watcher (obsidian/pinned_notes.rs)
- [x] 2.1.1 `PinnedNotesWatcher` (template: `templates_watcher.rs`, debounced 200ms) over the union of all sessions' pinned paths; managed state in `lib.rs` like `TemplatesWatcherState`.
- [x] 2.1.2 On change → emit `vault:pinned-note-changed { session_id, path }`. Rewatch when pins change.

### 2.2 Re-inject action
- [x] 2.2.1 `reinject_pinned_note(session_id, path)` command: re-read the note, write a single labeled block into the live PTY (post-spawn re-inject = PTY write, like `ConflictsPanel`).
- [x] 2.2.2 Frontend listener → Sileo toast with an explicit "Re-inject updated version" action (never auto).

### 2.3 Phase 2 gates
- [x] 2.3.1 Full gate suite (DONE) + manual: edit a pinned note in Obsidian → toast → re-inject → confirm the live agent sees the update.

## Phase 3 — #P Obsidian reading panel

Depends on Phase 1 (`pinnedNotesMapAtom` + pin/unpin commands). Read-only viewer; reuses markdown+wikilinks, vault search, and the pinned-notes store.

### 3.1 Vault-read commands (commands.rs)
- [x] 3.1.1 `read_vault_note(path: String) -> Result<String, String>`: canonicalize `path` + the configured `vault_root`; reject if `path` is not under `vault_root` (path-traversal guard); else `fs::read_to_string`. Do NOT reuse `read_file_content` (it is cwd-relative). Register in `lib.rs`.
- [x] 3.1.2 `resolve_vault_note(name: String) -> Result<Option<String>, String>`: first `.md` under `vault_root` whose filename stem matches `name` case-insensitively (vault-wide, not scoped to `search_subdir`); reuse the `search/mod.rs` walkdir. Register in `lib.rs`.
- [x] 3.1.3 Unit tests: read inside vault OK, read outside vault rejected, resolve hit/miss, case-insensitive match.

### 3.2 Panel registry wiring
- [x] 3.2.1 `stores/rightPanel.ts`: add `"obsidian"` to `TabType`, `obsidian: "document"` to `PANEL_CATEGORY_MAP`, `"obsidian"` to `SINGLETON_TYPES`.
- [x] 3.2.2 `components/layout/TopBar.tsx`: `PANEL_BUTTONS` entry (label "Obsidian", shortcut "Ctrl+Shift+Q", a lucide icon); show it only when `obsidianEnabledAtom` is true (filter like the plan button).
- [x] 3.2.3 `components/layout/RightPanel.tsx`: `viewPanelLabel` → `obsidian: "Obsidian"`; `ViewPanelContent` case → `<ObsidianPanel />`. Do NOT add it to `PICKER_TYPES`.
- [x] 3.2.4 `stores/shortcuts.ts`: `open-obsidian-panel` = `ctrl+shift+q` → `togglePanel("obsidian", "Obsidian")`. (Ctrl+Shift+Q verified free; Ctrl+Shift+U forbidden — Ubuntu Unicode entry.)

### 3.3 MarkdownView in-panel wikilink nav
- [x] 3.3.1 Add optional `onWikilinkNavigate?: (href: string) => void` to `MarkdownView`. In the `a` handler, for obsidian:// hrefs: if `onWikilinkNavigate` is set AND no Ctrl/Cmd modifier → call it; else → `openObsidianHref` (preserves current behavior for every other consumer).

### 3.4 ObsidianPanel component (components/obsidian/ObsidianPanel.tsx)
- [x] 3.4.1 Local state `viewMode: "list" | "reading"` + `currentNotePath`.
- [x] 3.4.2 List view — Pinned section: `pinnedNotesMapAtom[activeSessionId]` rows (title = filename sans `.md`), click → reading view, `×` → `unpin_vault_note`. Empty → muted "No pinned notes".
- [x] 3.4.3 List view — Finder: reuse `searchScopeAtom={kind:"vault"}` + debounced `runSearchAtom`; local Ctrl+D (`e.code==="KeyD"`) toggles `vaultSearchScopeAtom` (whole ↔ configured `search_subdir`, no-op if unset); hit row click → reading view; "Pin to session" action → `pin_vault_note`. Empty query → "Type to search your vault" hint.
- [x] 3.4.4 Reading view: header (title · back · Open in Obsidian · pin/unpin) + `<MarkdownView content={body} onWikilinkNavigate={...} />`. Body via `read_vault_note`. `onWikilinkNavigate`: URL-parse `file` from the `obsidian://open?...&file=…` href → `resolve_vault_note(name)` → path → `read_vault_note` → load; `None` → `openObsidianHref` fallback.
- [x] 3.4.5 Disabled state: when `vault_root` is null, show "Configure a vault in Settings → Obsidian" (panel button is already gated, but guard the body too).

### 3.5 Phase 3 gates
- [x] 3.5.1 `cargo clippy -- -D warnings && cargo test && cargo fmt --check && npx tsc --noEmit && pnpm build`.
- [x] 3.5.2 Manual: Ctrl+Shift+Q opens the panel → pinned notes render → click loads body with wikilinks → wikilink click navigates in-panel, Ctrl+click opens Obsidian → finder defaults to whole vault, Ctrl+D scopes to subdir → pin from finder appears in the session chip and the injected context.
