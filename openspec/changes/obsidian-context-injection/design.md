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

## Resolved decisions (user, 2026-06-01)

1. **Fallback for non-CC agents** → **capability-based, three tiers** (revised after the 2026 CLI research below — the initial "Unsupported for all non-CC" was based on the wrong premise that only CC can inject). CC = `AppendSystemPromptFile`; Codex + OpenCode = `PromptPreamble` (fold into their launch prompt); Pi = `Unsupported` for now (only fixed-path project files). Pins always record; the chip's tooltip reflects the actual tier per session. No PTY-typed fallback.
2. **Storage** → **JSON-array column on `sessions`** (`pinned_note_paths TEXT`, migration `010`). Matches `tasks.blocked_by`. Reverse-lookup ("which sessions pinned this note") is already covered by N1 backlinks via the MOC, so no separate table.
3. **Scope** → **artifacts only this iteration**. This change is fully specced + tasked, then implemented in a future session. Tasks are split into Phase 1 (contract + schema + CC inject at spawn/resume + chip + non-CC honesty) and Phase 2 (N2 hot-reload watcher + re-inject action) so the future builder can land Phase 1 as a verifiable slice before Phase 2.
