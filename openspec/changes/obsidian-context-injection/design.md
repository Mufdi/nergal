# Design — obsidian-context-injection

## The contract (agent-agnostic context injection at spawn)

The agent-adapter refactor gave us `SpawnContext` + `SpawnSpec` + a per-adapter `spawn()`. We extend that, never bypass it.

```rust
// agents/mod.rs
pub struct SpawnContext<'a> {
    pub session_id: &'a str,
    pub cwd: &'a Path,
    pub resume_from: Option<&'a str>,
    pub initial_prompt: Option<&'a str>,
    pub injected_context: Option<&'a str>, // NEW: assembled pinned-note bodies
}

/// How an adapter accepts a context block at spawn. The variant decides how
/// `injected_context` is folded into the launch command. Capability-based so
/// each agent uses its best available channel — verified against 2026 CLI docs
/// (see research note below), not a CC-only assumption.
pub enum ContextInjection {
    /// Ephemeral per-spawn system prompt via an arbitrary-path flag. The notes
    /// are system context, isolated per session, auto-cleaned. (Claude Code:
    /// `--append-system-prompt-file <path>`.) The gold standard.
    AppendSystemPromptFile,
    /// Fold the block into the launch prompt as a labeled preamble. Delivered
    /// at spawn non-interactively, but as the agent's first *turn*, not system
    /// context. (Codex positional `PROMPT`; OpenCode `--prompt`.)
    PromptPreamble,
    /// No clean spawn-time channel (only fixed-path project files like
    /// AGENTS.md / .pi/APPEND_SYSTEM.md, which are persistent + not per-session).
    /// Pin records but injection is skipped; the UI says so. (Pi, pending a
    /// per-invocation flag — its APPEND_SYSTEM.md is project-scoped.)
    Unsupported,
}

trait AgentAdapter {
    // ...existing...
    fn context_injection(&self) -> ContextInjection { ContextInjection::Unsupported }
}
```

`spawn()` reads `ctx.injected_context` and applies it per `self.context_injection()`:

- **Claude Code** → `AppendSystemPromptFile`: write the block to `~/.config/cluihud/spawn-context/<session_id>.md`, push `--append-system-prompt-file <path>`. System context, distinct from the user's first message, survives the whole session.
- **Codex** → `PromptPreamble`: prepend the context (fenced + labeled) to the positional `PROMPT` arg. Codex also auto-reads `AGENTS.md`, but that is project-scoped/persistent — wrong for per-session pins.
- **OpenCode** → `PromptPreamble`: fold into `--prompt`. (Re-verify whether the shipped CLI exposes a `--system` flag — one 2026 source claims it; the CLI reference does not list it. If it does, prefer a new `AppendSystemPrompt` variant for OpenCode.)
- **Pi** → `Unsupported` for now: its only system-prompt-append channels are fixed-path files (`.pi/APPEND_SYSTEM.md`, `AGENTS.md`) that are project-scoped and persistent, so they can't carry ephemeral per-session context cleanly. Revisit if Pi adds a per-invocation flag. (Verify Pi's launch-prompt arg in Phase 1 — if it takes a positional prompt, Pi can move to `PromptPreamble`.)

### Research note (2026 CLI capabilities)

The "only CC can inject" premise was wrong. Findings: CC `--append-system-prompt[-file]`; Codex positional `PROMPT` + hierarchical `AGENTS.md` (Linux-Foundation cross-tool standard) + interactive injection; OpenCode `--prompt`/`--agent`/`AGENTS.md`; Pi `.pi/SYSTEM.md`/`APPEND_SYSTEM.md` + `AGENTS.md`. The real differentiator is **ephemeral per-invocation arbitrary-path** injection (CC only) vs. **fixed-path project files** (everyone, but persistent + not session-scoped) vs. **launch prompt** (Codex/OpenCode, delivered-but-as-user-turn). The contract captures this as 3 capability tiers instead of a CC binary.

**Why not the AGENTS.md cross-tool standard for everyone?** It is project-scoped + persistent: it would pollute the user's repo, and sessions sharing a cwd (the first session of a workspace runs in the repo root, not a worktree) would clobber each other. Per-session ephemeral injection beats a shared persistent file. AGENTS.md stays the user's own concern.

**Why not a PTY-typed fallback?** Typing a large block into the REPL races boot, lands as a user turn, and isn't system context. `PromptPreamble` (via the CLI's own launch-prompt) is the clean version of that idea for agents that support it; `Unsupported` is honest for those that don't.

## Pinned notes = single source of truth (covers #3 and #H)

#3 (inject-at-creation) and #H (persistent pin + re-inject on resume) are the same data with different UI entry points. We store one list per session and drive both from it:

- **Schema**: migration `010_pinned_notes.sql` → `ALTER TABLE sessions ADD COLUMN pinned_note_paths TEXT;` storing a JSON array of absolute vault-note paths (same pattern as `tasks.blocked_by`). `Session.pinned_note_paths: Vec<String>`.
- **#3**: pinning notes before the first spawn (from the new-session / bootstrap surface) just populates the list early.
- **#H**: the chip + pin/unpin during a live session mutate the same list; resume re-assembles + re-injects.

## Assembly (obsidian/pinned_notes.rs)

`assemble_context(paths, vault_root) -> Option<String>`:
- Read each pinned note (skip missing); strip nothing — the agent gets the raw markdown.
- Wrap each in a labeled fence so the agent knows provenance:
  ```
  # Pinned vault context

  ## [[Note Title]]
  <body>
  ```
- Cap the total at a budget (proposed 64KB ≈ half the session-log cap) to keep the system prompt sane; if over, include notes until the budget is hit and append a truncation note. Log what was dropped (no silent cap — RULES).

## Spawn + resume wiring (pty.rs)

In `start_claude_session`, after resolving `initial_prompt`:
- Load `pinned_note_paths` from the DB session row, `assemble_context(...)`, set `ctx.injected_context`.
- This runs for BOTH fresh (`resume=None`) and resume paths → re-injection on resume is automatic (same code path). No separate resume hook needed.

## Hot reload (N2)

`PinnedNotesWatcher` (template: `obsidian/templates_watcher.rs`, debounced 200ms) watches the union of all sessions' pinned paths. On change → emit `vault:pinned-note-changed { session_id, path }`. Frontend shows a Sileo toast with a **"Re-inject updated version"** action that writes the refreshed single-note block into the live PTY (post-spawn re-inject is a PTY write, like ConflictsPanel) — explicit, never automatic.

## UI

- **Pin affordance**: a "Pin to session" action in the vault search modal (alongside open/cite) and the `@@` picker.
- **Chip**: session-tab badge (TopBar) showing pinned count; hover lists names; click opens a small popover to unpin. Pattern follows the conflict dot.
- **Non-CC honesty**: when pinning on a session whose adapter returns `Unsupported`, the pin still records (for when support lands) but the chip carries a muted "injection: CC only" tooltip.

## #P — Obsidian reading panel

A read-only right-panel for reading vault notes inside Nergal (avoids Alt+Tab to Obsidian). It also doubles as the pin-management surface, consuming the **same** `pinned_note_paths` as #3/#H — no new storage. Decided with the user 2026-06-02; added to this change because it shares the pinned-notes single source.

### Panel registry wiring (verified against the codebase)

The right panel is driven by a small registry that a new panel must extend in exactly these points (all verified 2026-06-02):

- `stores/rightPanel.ts`: add `"obsidian"` to the `TabType` union; add `obsidian: "document"` to `PANEL_CATEGORY_MAP` (a reading view is a document for the layout-preset proportions, like plan/spec/file); add `"obsidian"` to `SINGLETON_TYPES` (one Obsidian panel per session — internal nav, no per-note tabs).
- `components/layout/TopBar.tsx`: add a `PANEL_BUTTONS` entry `{ type: "obsidian", label: "Obsidian", shortcut: "Ctrl+Shift+Q", icon: <lucide> }`. Gate its visibility on `obsidianEnabledAtom` (vault configured) the way the plan button is filtered out for non-plan agents — never show a dead panel.
- `components/layout/RightPanel.tsx`: add `obsidian: "Obsidian"` to `viewPanelLabel`; add a `ViewPanelContent` case rendering `<ObsidianPanel />`. The panel is **not** in `PICKER_TYPES` (it owns its own finder, not the file-picker overlay).
- `stores/shortcuts.ts`: register `{ id: "open-obsidian-panel", keys: "ctrl+shift+q", ... togglePanel("obsidian", "Obsidian") }`. **Ctrl+Shift+Q is verified free** (occupied Ctrl+Shift letters as of 2026-06-02: A B D E F G H I J K L N O P R S T V X Y, plus Tab and 0–9). **Never use Ctrl+Shift+U** — Ubuntu/GTK intercepts it for Unicode codepoint entry.

### Component (`components/obsidian/ObsidianPanel.tsx`)

Two internal modes; no router, just local state (`viewMode: "list" | "reading"`, `currentNotePath`).

**List view (default):**
- **Pinned section** (top): reads `pinnedNotesMapAtom[activeSessionId]` (the store from task 1.7.1). Each pinned note is a clickable row (title = filename sans `.md`) that loads the reading view; an unpin `×` per row calls `unpin_vault_note` (the same command the chip uses). When empty, a muted "No pinned notes" line.
- **Vault finder** (below): a query-driven search box that **reuses the existing vault search infra verbatim** — `searchScopeAtom = { kind: "vault" }`, debounced `runSearchAtom`, and the `search` command with `vaultSubdir`. Scope defaults to the whole vault; **Ctrl+D toggles `vaultSearchScopeAtom` between `"whole"` and the configured `obsidianConfig.search_subdir`** (no-op when no subdir is configured) — byte-identical behavior to `VaultSearchModal` (`e.code === "KeyD"`, handled locally while the finder owns focus, never via the global registry). Each hit row: click → reading view; a "Pin to session" action → `pin_vault_note` (feeds the injection set, closing the loop with #3/#H). Empty query → the same "Type to search your vault" hint as the modal (the finder is query-driven; a browse-all-notes tree is an explicit non-goal — it would need a separate `list_vault_notes` capability).

**Reading view:**
- Header: note title · back-to-list (←) · "Open in Obsidian" (`openInObsidian` / `openObsidianHref`) · pin/unpin toggle for the current note.
- Body: `<MarkdownView content={body} />` — already wired to the wikilink remark plugin (G) via `useObsidianRemarkPlugin` + `obsidianUrlTransform`, gated on `render_wikilinks`. Body comes from the new `read_vault_note(path)` command.
- **Wikilink navigation (in-panel by default, user-chosen 2026-06-02):** `MarkdownView` today hardcodes `openObsidianHref` for obsidian:// links. Add an optional `onWikilinkNavigate?: (href: string) => void` prop; when the panel passes it, the `a` handler calls it for obsidian:// hrefs instead of `openObsidianHref`, **except** when `event.metaKey || event.ctrlKey` is held → then open in Obsidian (matches the scratchpad's `obsidianWikilinkExtension` modifier convention). The panel's handler URL-parses the `file` param out of the `obsidian://open?vault=…&file=…` href (built by `buildObsidianUri`), calls `resolve_vault_note(name)` → absolute path → `read_vault_note` → loads it in the reading view. If `resolve_vault_note` returns `None` (unresolved/new note), fall back to `openObsidianHref` so Obsidian handles creation.

### New backend commands

- `read_vault_note(path: String) -> Result<String, String>`: canonicalize `path` and the configured `vault_root`; reject (error) if the note is not under `vault_root` (path-traversal guard — mirrors the scheme-allowlist safety stance); else `fs::read_to_string`. **Do not** reuse `read_file_content` — it joins against `resolve_session_cwd` (the workspace), not the vault.
- `resolve_vault_note(name: String) -> Result<Option<String>, String>`: find the first `.md` under `vault_root` whose filename stem (case-insensitive) equals `name` (Obsidian resolves wikilinks vault-wide, not scoped to `search_subdir`). Return its absolute path or `None`. Reuse the vault walk already present in `search/mod.rs` (walkdir) rather than shelling out.

### Read-only by design (non-goals)

No graph view, no backlinks pane, no inline editing. Editing a note = the "Open in Obsidian" affordance (#12). This keeps the panel a thin reading/pinning surface over infra that already exists; the only genuinely new backend surface is the two guarded vault-read commands.

## Resolved decisions (user, 2026-06-01)

1. **Fallback for non-CC agents** → **capability-based, three tiers** (revised after the 2026 CLI research below — the initial "Unsupported for all non-CC" was based on the wrong premise that only CC can inject). CC = `AppendSystemPromptFile`; Codex + OpenCode = `PromptPreamble` (fold into their launch prompt); Pi = `Unsupported` for now (only fixed-path project files). Pins always record; the chip's tooltip reflects the actual tier per session. No PTY-typed fallback.
2. **Storage** → **JSON-array column on `sessions`** (`pinned_note_paths TEXT`, migration `010`). Matches `tasks.blocked_by`. Reverse-lookup ("which sessions pinned this note") is already covered by N1 backlinks via the MOC, so no separate table.
3. **Scope** → **artifacts only this iteration**. This change is fully specced + tasked, then implemented in a future session. Tasks are split into Phase 1 (contract + schema + CC inject at spawn/resume + chip + non-CC honesty), Phase 2 (N2 hot-reload watcher + re-inject action), and Phase 3 (#P reading panel) so the future builder can land each as a verifiable slice. Phase 3 depends on Phase 1 (the `pinnedNotesMapAtom` store + pin/unpin commands).

## Resolved decisions (user, 2026-06-02)

4. **#P reading panel** → in scope for this change (shares the pinned-notes single source). Read-only; reuses the markdown+wikilink pipeline, the vault search infra, and the pinned-notes store. New surface limited to two guarded vault-read commands + the panel component + registry wiring.
5. **Wikilink click in the panel** → **in-panel navigation by default; Ctrl/Cmd+click opens Obsidian** (matches the scratchpad modifier convention). Requires `resolve_vault_note`; unresolved names fall back to opening Obsidian.
6. **Panel finder behavior** → identical to `VaultSearchModal`: query-driven, scope defaults to the whole vault, **Ctrl+D** toggles to the configured `search_subdir`. A browse-all-notes tree is a non-goal this iteration.
7. **Open shortcut** → **Ctrl+Shift+Q** (verified free). Ctrl+Shift+U is forbidden (Ubuntu Unicode-entry collision).
