## Context

v2 of `ship-flow` reached partial implementation, then live testing exposed structural issues:

- The 2-step dialog (Stage → Commit+PR) made staging feel like a separate concern when it's actually implicit in committing.
- The `auto_merge_default` checkbox split the flow across opaque `gh pr merge --auto --squash` invocations the user never sees finishing — confusing for a tool that's supposed to make the workflow transparent.
- The "Merge into… (local)" button in the GitPanel commit bar surfaced a path the user explicitly does not use in their solo PR-driven workflow.
- The auto-merge-conflict closed-loop poll fired silently and triggered destructive cleanup without explicit user confirmation; user reported "session vanished without confirmation".
- The standalone ConflictsPanel sat outside the tab system, breaking the "everything is a tab in the right panel" mental model.
- Annotations existed only as an idea but were never wired into Claude — they were decorative, not actionable.

The user's actual workflow, restated from their feedback:

> Start a new session in a worktree. Make changes (commit at any point or not). Run `Ship`. Modal warns of consequences and offers three actions: just Commit, Commit+Push, or Commit+Push+PR. If I take the third path, a PR opens. The GitPanel sidebar can switch between showing files and showing my PRs (open and closed). Click a PR → opens a tab with the diff. I annotate chunks with intent. Click "Apply with Claude" → Claude rewrites in the same session. Cycle until clean. Click "Merge into main". If conflict, a Conflicts tab opens — Claude helps. After successful merge, the session is archived and deleted; I switch to my next session or back to an empty workspace.

v3 is the architectural reset to make that one paragraph the literal implementation.

## Goals / Non-Goals

**Goals:**
- One Ship modal. Three buttons. Each is a complete, named action — no implicit composition.
- Annotations are a first-class loop control, not decoration.
- The right panel composes everything: GitPanel (state) + PR Viewer tab (review) + Conflicts tab (resolution). All accessible via shortcuts and direct manipulation.
- Cleanup is always user-triggered (the Merge click) and always announced (toast + transition to next session).
- Plan files survive the cleanup via archive into the main repo.
- Modal owns its keyboard space (`Ctrl+1/2/3` while open) without leaking to global session shortcuts.

**Non-Goals:**
- Auto-merge / `gh pr merge --auto` — gone.
- Local-merge primary action — gone (`merge_session` backend kept for future repos-without-remote scenarios but no UI).
- 2-step / wizard-style modal flows — gone.
- Per-session `auto_merge` preferences — gone.
- Decorative annotations — annotations only exist if they drive Claude.
- Auto-cleanup poll with grace periods, countdowns, or undo — explicit user click is the trigger.
- Multi-workspace PR aggregation — PR list scoped to the active workspace.

## Decisions

### Decision 1: Single-pane modal with 3 progressive buttons (vs. 2-step wizard or multi-pane tabs)

**Choice:** One modal pane. Title field, body field, optional PR base picker (revealed only when "Commit+Push+PR" is the chosen action), warning banner above. Three primary buttons in the footer: Commit (`Ctrl+1`), Commit+Push (`Ctrl+2`), Commit+Push+PR (`Ctrl+3`). The button the user hovers/focuses determines which fields are required (e.g., title is required only for the PR action).

**Rationale:** The user's mental model is "I'm done — what do I want to do with this?" not "let me walk through staging then composing then sending". The 3 buttons name the three reasonable destinations. No stepper, no Back/Next chrome.

**Alternatives considered:**
- *Single button "Ship" with a dropdown for variants*: hides the choice, fewer keystrokes but worse discoverability.
- *Confirm-checkbox before Ship*: adds friction without information.
- *Keep 2-step modal*: rejected — user explicitly asked to drop it.

**Trade-off:** The same modal renders differently per chosen action (PR base picker visibility, validation rules). Slightly more conditional rendering, but the alternative (separate modals per action) duplicates 90% of the code.

### Decision 2: PR Viewer as a tab type (vs. transforming GitPanel)

**Choice:** PR Viewer is a new tab type `pr` rendered in the right panel via the existing tab system. GitPanel stays as the "git state" panel and gains a sidebar toggle to show PRs (entry points to open the tab).

**Rationale:** The user wants to be able to navigate to historical PRs without losing the GitPanel's current-state view. Tabs compose better with the existing layout primitives. Transforming GitPanel would couple two unrelated concerns (git filesystem state vs. PR review).

**Alternatives considered:**
- *Transform GitPanel when PR is open*: my original v3 idea. Rejected per user feedback — loses access to the PR list and other state.
- *PR Viewer as a separate panel (left/right of GitPanel)*: doesn't fit the existing layout model and conflicts with how Conflicts and Diff tabs already work.

**Trade-off:** The user can have many `pr` tabs open simultaneously (one per PR they've clicked into). This is desirable for review workflows but means tab management gets richer.

### Decision 3: Annotations are instructions to Claude (vs. inert markup)

**Choice:** Annotations on a PR Viewer chunk are typed text (e.g., "use Set", "drop the try/catch"). The PR Viewer footer's "Apply annotations with Claude" button packages the PR diff + the list of annotations with their chunk anchors into a structured prompt and writes it to the active session's terminal. Claude makes the edits in the worktree, commits, pushes; the PR Viewer refreshes; applied annotations are marked resolved (or deleted).

**Rationale:** Annotations only earn their place if they drive action. cluihud already has the annotation primitive (planning, specs); reusing it for PR review keeps the surface coherent and turns the PR into an iterative document, not just a viewer.

**Alternatives considered:**
- *Annotations as comments synced to GitHub*: out of scope (would require GitHub PR review API, multi-user concerns).
- *Annotations purely visual, no Claude wiring*: kept the v2 idea — already proven to add no value, user explicitly called this out.
- *One-shot annotations sent on Merge click*: less control; user wants a review loop.

**Trade-off:** Sending annotations to Claude is a destructive action (it modifies the worktree). Mitigation: explicit "Apply with Claude" button (no auto-send), and Claude's normal session UX (the user sees what's being done in real time in the terminal).

### Decision 4: Conflicts as a tab type (vs. standalone panel)

**Choice:** Conflicts render in a tab with type `conflicts` in the right panel. Triggered by `Ctrl+Alt+Q`, by failed merge in PR Viewer, or by clicking the inline conflicts list in GitPanel.

**Rationale:** The right panel already has tabs for diffs, files, plans, specs, conflicts. A standalone panel for conflicts breaks the unified mental model. The user explicitly proposed this consolidation.

**Alternatives considered:**
- *Standalone panel (current)*: rejected per user feedback.
- *Modal for conflicts*: too disruptive when the user is mid-flow and needs to see the surrounding context (PR, files).

**Trade-off:** Tab-based means users can have a Conflicts tab open even when there are no conflicts (stale state). Mitigation: tab auto-closes when `conflicted_files` becomes empty AND `pendingMerge` is false.

### Decision 5: Cleanup runs only on explicit "Merge into `<base>`" success (vs. auto-poll)

**Choice:** No background poll observes the PR for merged state. Cleanup is invoked synchronously when the user clicks "Merge into `<base>`" in the PR Viewer and `gh pr merge` succeeds. Sequence:

1. Archive plans (`<worktree>/.claude/plans/*.md` → `<main_repo>/.claude/plans/archive/YYYY-MM/<session_id>/`).
2. Delete worktree directory + branch.
3. Delete the session DB row.
4. Remove the session from the open tab list and clear active selection if it was active.
5. Switch to the most recently-`updated_at` remaining session in the workspace; if none, close the right panel and show empty state.
6. Toast: "Session merged and archived. Plans saved to `<archive_path>`."

**Rationale:** Auto-cleanup-on-poll was the source of "session vanished" bug. Explicit-on-click puts the destructive action behind the user's intent every time.

**Alternatives considered:**
- *Auto-cleanup with countdown toast (e.g., "session deletes in 5s, click to keep")*: still surprising; the user's prior session may also still be in the deleted-session-list path; cluttered UX.
- *Poll AND require user confirmation*: redundant with explicit click.
- *Cleanup as a separate manual action*: makes the user do extra work after merge; the click already implied closure.

**Trade-off:** If `gh pr merge` succeeds but the user closes cluihud before the cleanup completes, the worktree could outlive the merged PR. Mitigation: cleanup runs synchronously inside the merge handler; if cluihud is killed mid-operation, the worktree stays — the user can re-open and trigger cleanup manually via the GitPanel "Cleanup session" banner (kept for this case).

### Decision 6: Switch to most-recent session post-cleanup (vs. cold start always)

**Choice:** After cleanup, find the workspace's session with the highest `updated_at` that is not the just-deleted one. If found, switch to it. Otherwise, close the right panel and show the empty workspace state.

**Rationale:** Multi-session workflow is the norm. Cold start every time disrupts continuity. Falling back to "most recent" is the natural "go back to what I was working on before".

**Alternatives considered:**
- *Cold start always*: simpler but disruptive for multi-session work.
- *Switch to "next" session in tab order*: tab order is arbitrary; recency is more meaningful.
- *Prompt the user with a picker*: extra friction.

**Trade-off:** "Most recent" might land on a session the user wasn't actively considering. Acceptable: better than cold start, and the user can immediately Cmd+K or click another session.

### Decision 7: Modal `Ctrl+1/2/3` capture (vs. global session shortcuts always winning)

**Choice:** While the Ship modal is open, the dialog's keydown handler captures `Ctrl+1`, `Ctrl+2`, `Ctrl+3` in capture phase and dispatches the corresponding action. The global session-switching shortcuts that normally bind these key combos are bypassed by the capture-phase guard.

**Rationale:** Modal-scoped shortcuts are standard UX (the modal "owns" the keyboard while open). The user wants the same key glyph for the action shortcut as the visible Kbd chip on each button.

**Alternatives considered:**
- *Use different shortcuts (Alt+1/2/3)*: harder to reach, doesn't match the user's preference for `Ctrl+number`.
- *Disable global shortcuts modally*: the global shortcuts already check focus zones; modal-open guard is the standard pattern.

**Trade-off:** Users who muscle-memory the global session-switching shortcut while the Ship modal is open will accidentally fire an action button. Mitigation: the modal is a deliberate context (user opened it explicitly); the warning banner up top reminds them what they're about to do.

### Decision 8: PR base picker only for "Commit+Push+PR" (vs. always visible)

**Choice:** The PR base branch picker (custom dropdown, OS-themed) renders inline in the modal **only when** the focused/last-clicked action is "Commit+Push+PR". For Commit and Commit+Push, the picker is hidden (no PR is created, so no base to choose). Default value remains `main` (or workspace-configured base).

**Rationale:** Reducing visual noise. Showing a PR base picker for Commit-only is a UX wart.

**Alternatives considered:**
- *Always visible*: noisy when not relevant.
- *Conditional revealed via a "More options" toggle*: extra click.

**Trade-off:** When the user changes intent from Commit+Push to Commit+Push+PR mid-flow, the picker appears (with a small fade-in to acknowledge). Slight layout shift; acceptable.

### Decision 9: Plan archive lives in main repo's `.claude/plans/archive/` (vs. global ~/.claude/plans/archive/)

**Choice:** Plans archive at `<main_repo>/.claude/plans/archive/YYYY-MM/<session_id>/`. Per-project, gitignored, persistent across worktree lifecycle.

**Rationale:** The plan-archive skill convention uses `$PWD/.claude/plans/archive/`. Per-project archive scopes plans to the project they document. Global archive would mix plans across all projects (worse for retrieval).

**Alternatives considered:**
- *Global `~/.claude/plans/archive/`*: cross-project bleed, harder to retrieve.
- *Inside the deleted worktree*: lost on worktree cleanup (defeats the purpose).
- *Push to git as a separate `plans/` branch*: too heavyweight for personal-tool scope.

**Trade-off:** The archive grows over time inside each repo. Mitigation: it's gitignored (not pushed); user can prune via the existing `/plan-archive` skill.

## Risks / Trade-offs

- **Risk: Modal shortcut capture conflicts with future global shortcuts using `Ctrl+1/2/3`.**
  → Mitigation: The capture happens only in the modal's keydown handler (capture phase, scoped to modal-open state). Global shortcuts on these key combos still fire when the modal is closed. Documenting in `keyboard-shortcuts` spec that `Ctrl+1/2/3` may be re-bound contextually.
- **Risk: PR Viewer annotations may stack up across multiple sessions and become hard to reconcile.**
  → Mitigation: Annotations live in a per-PR map keyed by `(workspaceId, prNumber)` so multi-PR review remains clean. "Apply annotations with Claude" only sends annotations for the focused PR.
- **Risk: `gh pr merge` succeeds but cluihud crashes before cleanup runs.**
  → Mitigation: User can re-open cluihud, see the merged PR in the PRs sidebar, and trigger cleanup manually via the GitPanel "PR is merged. Cleanup session?" banner (kept from v2 for exactly this recovery case).
- **Risk: Plan archive directory name collision (two sessions in the same month with the same `session_id`).**
  → Mitigation: `session_id` is a UUID, collisions effectively impossible. Append `-N` suffix if the directory already exists, just in case.
- **Risk: The "switch to most recent session" post-cleanup may land on a session the user didn't expect.**
  → Mitigation: A short toast announces the transition: "Switched to session: `<name>`". The user can immediately switch elsewhere.
- **Trade-off: Single-pane modal with 3 buttons means the chosen-action state is implicit (depends on which button got clicked last/focused). The form fields' validation depends on this state.**
  → Mitigation: Add a small "About to: <action>" indicator near the action buttons that updates on hover/focus, so the user always knows which action is "armed".
- **Trade-off: Annotations-as-instructions creates a tighter coupling between the PR Viewer and the active terminal session.**
  → Mitigation: The "Apply with Claude" button is disabled when the session that owns the PR is not the currently active one (avoids cross-session prompt injection).
