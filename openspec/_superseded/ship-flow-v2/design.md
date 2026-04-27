## Context

cluihud's git surface evolved organically: the v1 `ship-flow` change introduced commit/push/PR atomicity, conflict resolution panel, and CI polling. The implementation is in `main` and used daily. After ~2 weeks of single-user usage on a Linux desktop, several friction points surfaced:

- **Cognitive load in ShipDialog**: 7 concerns rendered together (auth check, preview, staging, title, body, auto-merge toggle, progress, errors). The user reported having to scan the dialog before deciding what to do, even on routine ships.
- **Two-button-for-same-action pattern in GitPanel**: header has Ship/Push, commit bar has Commit/Push/Ship. Two zones for the same outcome — violates "one primary action per screen" UX rule.
- **MergeModal as Sidebar-first action**: the modal opens from a Sidebar context menu, framing local-merge as equal weight to Ship. The user's actual workflow uses Ship 95%+ of the time and considers local-merge an exception (offline, throwaway projects, no remote).
- **Session lifecycle ends with `update_session_status("completed")`**: the row stays in the DB indefinitely. The user wants the same "close conversation" behavior Claude Code has — wipe everything related to the session.
- **No closed loop on auto-merge conflict**: `gh pr merge --auto --squash` may queue and later block on conflict. Today the user has to notice the failed PR check, manually open Conflicts panel, and craft a prompt for Claude. Three context switches.
- **Space-toggle bug in ConflictsPanel**: clicking a region row leaves focus on the inner button. Pressing Space then triggers both the global keydown handler AND the browser's default button activation, causing the region to expand-then-collapse-then-expand.
- **Shortcuts in `title` attributes**: hover-only on Linux desktop, never discoverable through normal flow. The user explicitly stated TODO action must have a shortcut visible in the UI itself, not via Tab navigation discovery.

The backend Tauri commands (`git_ship`, `git_push`, `git_commit`, `merge_session`, `cleanup_merged_session`, `list_branches`, `complete_pending_merge`, `poll_pr_checks`) are stable and adequate. This is a **frontend-driven refactor** with one backend extension (full-deletion cleanup) and one config addition (`auto_merge_default`).

The v1 change `openspec/changes/ship-flow/` is implemented but not yet archived to `openspec/specs/`. v2's spec deltas extend v1's deltas in place; recommended order is to archive v1 first.

## Goals / Non-Goals

**Goals:**
- Reduce ShipDialog to one decision per screen via 2-step progressive disclosure.
- Make the user's mental flow (worktree → commit → push → PR → auto-merge → if conflict, Claude resolves) directly visible in the UI (stepper) and accessible via shortcuts.
- Single source of truth for git actions (no duplicate buttons in GitPanel).
- Local-merge stays available but visually subordinated as a sibling button, not a peer modal opened from another surface.
- Total session deletion on terminal states, mirroring Claude Code's own cleanup model.
- Closed-loop conflict handoff: same session, auto-prefill, user confirms shortcut, Claude receives full context.
- Keyboard-first: every git action discoverable via visible `<kbd>` chip in the UI.
- Persist auto-merge as user preference, not per-Ship choice.

**Non-Goals:**
- Touch/mobile/tablet ergonomics — cluihud is desktop-only, single user.
- Multi-user / team review workflows — solo workflow only.
- PR description templates beyond what v1 already supports (`.cluihud/pr-template.md`).
- Background auto-conflict resolution without user confirmation — explicit safety: user must press shortcut to send Claude prompt.
- Replacing the v1 backend command surface — v2 is frontend refactor + minimal backend extension.
- Reintroducing the worktree-as-disposable-experiment pattern (the v1 already removed that thinking).

## Decisions

### Decision 1: Total deletion vs soft-archive on session cleanup

**Choice:** Total deletion (DB row, worktree dir, branch, plan files, transcript) when the session reaches a terminal state (PR merged + remote cleaned, or local merge confirmed via cleanup button).

**Rationale:** The user's mental model treats sessions as ephemeral working memory tied to a single feature/branch. Once shipped, the worktree, branch, and conversation are no longer addressable — keeping a row marked `completed` adds list noise without enabling any retrieval. Claude Code itself wipes session state on close (no persistent "completed conversations" list); cluihud should match that to avoid two competing mental models.

**Alternatives considered:**
- *Soft-archive (current `update_session_status("completed")`):* Preserves history. Rejected because the user reports zero use of historical sessions for retrieval; the row only adds to the sidebar list as dead weight.
- *Soft-archive but hide from default view:* A middle ground (keep DB row, filter from sidebar). Rejected because adds backend complexity (filter logic, reveal toggle) without solving the core "I want it gone" intent.

**Trade-off:** Once deleted, there's no "undo last session close" — user must rely on git history (which is preserved on remote post-PR-merged). Mitigation: the cleanup is triggered by an explicit user action OR by a confirmed-merged PR poll, never silently. Toast confirms deletion.

### Decision 2: Same session vs new session for conflict resolution

**Choice:** Auto-merge conflict handoff happens **in the same session** that opened the PR. The Conflicts panel opens in that session, Claude receives a prefilled prompt with PR/file/diff context, user confirms shortcut to actually send.

**Rationale:** The session that produced the change has the full conversational context — what was attempted, why this approach, prior tradeoffs. Spawning a new session forces Claude to rebuild that context from git diffs alone, losing the "why" behind the changes. Conflict resolution often hinges on intent, not just text.

**Alternatives considered:**
- *New session per conflict:* Cleaner separation, but loses context. Rejected.
- *Inject conflict context into a fresh prompt in the same session:* Closer to chosen solution but blocks the user from reviewing the prompt before it's sent. Rejected for safety — Claude prompts can affect a real PR.
- *Open a "branched" sub-session inheriting parent context:* Architectural overkill for the use case; cluihud doesn't have sub-session primitives.

**Trade-off:** The session may be in the middle of a different sub-task when the conflict is detected. Mitigation: the auto-handoff is non-blocking (inline alert + Conflicts panel opens, but Claude isn't sent the prompt until user explicitly confirms via shortcut), so the user can finish their current train of thought before switching contexts.

### Decision 3: Single source vs duplicate Ship/Push buttons in GitPanel

**Choice:** Commit bar (bottom of panel) is the sole accommodation for Commit / Push / Ship / Merge actions. Header bar removes its Ship/Push duplicates and becomes info-only (branch, ahead, PR badge, CI checks).

**Rationale:** The duplicate creates ambiguity ("which one am I supposed to use?") and violates the `primary-action` UX rule (one primary CTA per screen region). The commit bar is contextually richer (already shows commit message input + staged count) and is where the user's eye lands when committing — the header buttons were redundant noise.

**Alternatives considered:**
- *Keep both, but visually demote header buttons:* Rejected — still requires the user to perceive a hierarchy that the screen doesn't reinforce.
- *Move all actions to header, remove from commit bar:* Header lacks the commit-message context and would force the user's eye to travel up after typing a message. Rejected.
- *Floating action button:* Out of scope for desktop ergonomics.

**Trade-off:** When the GitPanel is scrolled deep into history, the commit bar may scroll out of view. Mitigation: commit bar is `shrink-0` in flex layout (already pinned at bottom, won't scroll). No regression.

### Decision 4: Visible `<kbd>` chips vs `title` attributes for shortcuts

**Choice:** Every git action surface (button, menu item) renders its keyboard shortcut as a visible `<kbd>` chip alongside the label.

**Rationale:** The user explicitly stated all actions must have keyboard-discoverable shortcuts that don't require Tab navigation to find. `title` attributes only appear on hover (and not at all on touchscreens or for keyboard-first users), violating discoverability. `<kbd>` chips are the standard pattern for surfaces like VS Code, Linear, Raycast — they teach shortcuts during normal use.

**Alternatives considered:**
- *`title` only (current):* Rejected per user requirement.
- *Right-click context menu showing shortcuts:* Adds a hidden discovery layer; doesn't solve the "I shouldn't have to look" problem.
- *Tooltip on hover only when keyboard focus is active:* Inconsistent across input modalities.

**Trade-off:** Adds ~20-40px width per button. Mitigation: chips use `text-[9px]` muted styling so they don't compete with primary labels visually; only show OS-appropriate modifier glyphs (⌃⇧Y on macOS, Ctrl+Shift+Y on Linux/Windows).

### Decision 5: 2-step ShipDialog vs collapsible sections in single pane

**Choice:** Two explicit steps with a stepper indicator. Step 1 = Stage picker (full pane). Step 2 = Commit + PR (title, body, target branch, auto-merge, ship button).

**Rationale:** The user's workflow is sequential: first decide what's going in, then decide what to call it and where to send it. Modeling that as two steps matches the mental model. Collapsible sections in a single pane preserve all options at once but reintroduce the "scan-everything-before-deciding" cost.

**Alternatives considered:**
- *Collapsible sections (accordion):* User can expand/collapse but everything is one pane. Rejected — doesn't reduce decision density, just hides default-collapsed state.
- *3 steps (Stage / Commit / PR):* Over-fragmented; commit message and PR title are tightly coupled in the user's flow. Rejected.
- *Single pane (current v1):* Rejected as the originating problem.

**Trade-off:** Adds a "Next" click between staging and committing. Mitigation: Step 1 has Enter-to-advance shortcut; for users who frequently stage everything-default, the cost is one keystroke total.

### Decision 6: Persist `autoMergeDefault` in config vs per-session preference

**Choice:** Persist `git.auto_merge_default: bool` in `~/.cluihud/config.json` (or wherever `config.rs` writes). ShipDialog initialises the toggle from config; toggle change auto-saves.

**Rationale:** The user's flow uses auto-merge ON by default. Resetting per-Ship is friction without value. Per-session would be too granular (the workflow doesn't vary per session) and would require new schema fields.

**Alternatives considered:**
- *Per-session preference stored on session row:* Over-engineered for a binary preference. Rejected.
- *No persistence (current):* Rejected per user friction.

**Trade-off:** A user who experiments with auto-merge OFF for one session would have to manually re-enable. Mitigation: the toggle is a single click in Step 2, no extra clicks vs current behavior.

### Decision 7: ConflictsPanel Space-toggle fix via `blur()` vs `<div role="button">`

**Choice:** Keep `<button>` semantics; in the row/chevron `onClick` handlers, call `(e.currentTarget as HTMLElement).blur()` after state updates so the global keydown listener owns Space cleanly.

**Rationale:** Native `<button>` preserves Enter activation, focus rings, ARIA semantics, and screen-reader announcements out of the box. The bug is a focus-residue problem, not a semantics problem. `blur()` is the smallest fix.

**Alternatives considered:**
- *`<div role="button" tabIndex={0}>`:* Loses Enter activation by default (must be re-implemented). Adds maintenance burden across the panel. Rejected.
- *`e.preventDefault()` on the button's mousedown:* Prevents focus from landing on the button, but also prevents click event from firing in some browsers depending on event order. Brittle. Rejected.
- *Capture-phase listener with `stopImmediatePropagation()`:* Could swallow Space before the button sees it but adds complexity and depends on listener ordering. Rejected.

**Trade-off:** Brief focus loss after click means the user can't immediately Tab from that element. Mitigation: focus moves back to the panel container's natural focus target on next interaction; for keyboard navigation the global handler picks up regardless of focus.

### Decision 8: Backward-compat with v1 sessions

**Choice:** v2 deltas extend v1 deltas in place; the v1 change in `openspec/changes/ship-flow/` should be archived first to promote its specs into `openspec/specs/`. Sessions created under v1 remain functional; the only schema change is the cleanup behavior, which only triggers on terminal-state actions going forward.

**Rationale:** No data migration needed — the change is in trigger logic for cleanup (full-delete vs soft-mark). Existing `completed`-marked sessions in the DB can be left as-is (they don't affect new flows) or batch-deleted by the user via a future maintenance command (not in scope).

**Alternatives considered:**
- *One-shot migration to delete all `completed` sessions:* Out of scope; user can manually clean if desired.
- *Keep dual-mode (full-delete for new, soft-mark for old):* Adds permanent compat code with no benefit. Rejected.

## Risks / Trade-offs

- **Risk: Session cleanup deletes data the user wanted to preserve.**
  → Mitigation: Cleanup only triggers on (a) confirmed PR-merged-on-remote (poll detects merged state), (b) explicit "Cleanup session" action by user. Toast announces the deletion. No silent cleanup paths.
- **Risk: Auto-handoff prefills a Claude prompt the user didn't intend to send.**
  → Mitigation: Prefilled prompt is staged in the input area but NOT sent until user confirms via shortcut. Inline alert explains the situation.
- **Risk: 2-step dialog adds friction for users who want one-shot Ship.**
  → Mitigation: Step 1 has Enter-to-advance + Ctrl+Enter to skip-to-Ship if everything is default-staged. Power users can reach Ship in 2 keystrokes.
- **Risk: Removing duplicate header buttons in GitPanel breaks user muscle memory.**
  → Mitigation: Commit bar is always visible at the bottom of the panel. Shortcuts still work globally. Brief migration toast on first GitPanel mount post-update could acknowledge the change (optional).
- **Trade-off: `<kbd>` chips add visual density to button rows.**
  → Mitigation: Use muted styling (`text-[9px] text-muted-foreground/60`); chips disappear if the button is too narrow (responsive hide via `@container` query).
- **Trade-off: Closed-loop conflict resolution requires the session to still be open.**
  → Mitigation: If the user closed the session before the conflict arrived, fall back to a notification with "Reopen session to resolve" CTA.
