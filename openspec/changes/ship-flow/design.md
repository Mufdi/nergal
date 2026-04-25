## Context

cluihud already has partial git surface: `GitPanel.tsx` handles stage/commit/create-PR; `MergeModal.tsx` handles merge with conflict detection that degrades to a toast; `Sidebar.tsx` exposes Merge as a quick action; `useKeyboardShortcuts` + `attachCustomKeyEventHandler` gate shortcuts globally (shortcuts with `Ctrl+Shift+*` fire regardless of focus). Claude's `/commit` skill (user-global) can also perform commits autonomously — cluihud today does not react to those commits beyond refreshing git status.

The change introduces two new capabilities (`ship-flow`, `conflict-resolution`) and extends three existing ones (`keyboard-shortcuts`, `git-panel-v2`, `tab-system`). The main design tensions are:

1. **Two entry paths to commit** (manual via GitPanel textarea, auto via `/commit` skill). Both must converge to the same post-commit surface so the user sees "Ship it" consistently.
2. **One shortcut, two actions** (`Ctrl+Shift+R` = revise-plan OR resolve-conflict). Must be resolved by active context without breaking current semantics.
3. **Conflict view lives in two places** (inline summary in git panel + dedicated tab) and must reuse Zen Mode plumbing for expansion.

## Goals / Non-Goals

**Goals:**
- Single keystroke (`Ctrl+Shift+Y`) from any focus zone to ship a session end-to-end.
- PR preview before hitting `gh pr create` — prevent accidental PRs, allow template-based body.
- Conflict resolution surface that doesn't require dropping to terminal/editor.
- Contextual shortcut dispatch transparent to the user (right thing happens in right place).
- Zero new external dependencies; reuse existing `gh` CLI invocations already in `worktree.rs`.

**Non-Goals:**
- Moving session actions to TopBar (separate sprint, item 6 of conversation).
- Replacing the `/commit` Claude skill. Cluihud observes and augments; it does not reimplement it.
- Auto-generating commit messages via `claude -p`. Discarded in conversation — redundant with existing `/commit`.
- Non-`gh` PR providers (GitLab, Bitbucket). Existing code assumes `gh`, change stays consistent.
- Interactive merge conflict editing character-by-character. The conflict tab shows 3-panel diffs and delegates to Claude via context injection for automated resolution, or opens in the user's external editor for manual resolution.

## Decisions

### 1. Ship = commit+push+PR, pure Rust orchestration

`git_ship` lives in `src-tauri/src/worktree.rs` as a composition of existing primitives: `commit()` (conditional on `message` being non-empty AND staged > 0), `push()`, `create_pr()`. Tauri command `git_ship` takes `{ session_id, message?, title, body }`.

**Alternatives considered:**
- *Delegate to a shell script / `/commit` skill extension*: Rejected. Mixes responsibilities, harder to surface progress events, and `/commit` is user-global so we can't assume its shape.
- *Separate calls from frontend (`commit` → `push` → `create_pr`)*: Rejected. Non-atomic; partial failures (e.g., push succeeds, PR fails) leave ambiguous state and require complex rollback UX. Single Rust command emits one result.

Progress is surfaced by emitting `ship:progress` events (`{ stage: "commit" | "push" | "pr", ok: bool }`) so the dialog shows step-by-step feedback.

### 2. Ship-it badge — observable via existing git refresh

After any commit (manual or Claude-driven via `/commit`), the existing `files:modified` listener in `GitPanel.tsx` already triggers `refreshCore()`. We extend the derived state: when `ahead > 0 && !prInfo && !committing`, the panel header renders a prominent "Ship it" badge (green, sticky). The badge is a pure derived UI artifact — no new events, no new atoms needed beyond what `get_session_git_info` + `get_pr_status` already return.

**Alternatives considered:**
- *Listen to `tool-done` hook for `Bash` tool-use with `git commit` in args*: Rejected. Fragile parsing; `files:modified` is already a reliable signal.

### 3. PR preview dialog — prefill strategy

The dialog, `ShipDialog.tsx`, pulls data from a new Tauri command `get_pr_preview_data(session_id)`:
- `title`: last commit subject (or `branch` name if no commit yet and dirty staged)
- `body`: concatenation of commit subjects in `base..HEAD` (deduped, newest first) + a `---` separator + a diffstat summary (`+X -Y across N files`) + optional project template from `.cluihud/pr-template.md` if present
- `commits: [{hash, subject}]`, `diffstat: {added, removed, files}`, `base: string`

The dialog shows editable title + body textareas, a preview of the commit list, and an "Enter to ship" hint. Pressing Enter (when a button is focused) or clicking "Ship" calls `git_ship`. Escape cancels.

**Alternatives considered:**
- *Auto-ship without dialog*: Rejected. Global shortcut to push to remote is too high-consequence to fire silently. The dialog acts as the confirmation guardrail.
- *Template only (no commit list)*: Rejected. The commit-list prefill is the highest-ROI automation — it's what most users paste manually.

### 4. CI polling — poll-on-active-session only

A new Tauri command `poll_pr_checks(session_id) -> PrChecks { state, passing, failing, pending }` shells out to `gh pr checks <n> --json state,conclusion`. Polling lives in the frontend: a `useEffect` in GitPanel sets an interval of 20s while `prInfo.state === "OPEN"` and the panel is mounted (so switching sessions pauses polling).

**Alternatives considered:**
- *Backend polling loop emitting events*: Rejected. More complex; polling is inherently wasteful and per-session scoping is clearer from the UI layer.
- *Webhooks*: Not viable without a server.

### 5. Conflict resolution — new tab type, three-panel layout

`conflict` joins the existing `Tab["type"]` union in `rightPanel.ts`. Data shape: `{ path: string, ours: string, theirs: string, merged: string }`. Tabs are singleton-per-(session, file) — opening the same file focuses the existing tab.

The tab renders three CodeMirror instances (ours / theirs / merged) in a horizontal split. The `merged` panel is editable; the other two are read-only with conflict markers highlighted. Bottom toolbar: "Accept ours" (copies ours to merged), "Accept theirs" (copies theirs to merged), "Ask Claude to resolve" (injects `{path, ours, theirs, merged}` + instruction into the next prompt via `set_pending_annotations`-style channel), "Save resolution" (writes `merged` to the file on disk, stages it, closes the tab, re-checks conflicts).

**Expansion to Zen Mode**: ConflictTab renders a "Expand" button in the top-right. Clicking it opens Zen Mode with a custom mode: Zen gets a new prop `mode: "diff" | "conflict"` and, for conflict, renders the same 3-panel layout at full-screen dimensions. The tab remains mounted; Zen is a visual overlay.

**Inline conflict list in git panel**: a new section at the top of the git panel (above "History") that appears ONLY when the active session has conflicted files (detected via an extended `merge_session` return or a new `get_conflicted_files` command polled after merge). Each row is filename + "Resolve" button that opens/focuses the conflict tab.

**Alternatives considered:**
- *Modal dialog instead of tab*: Rejected. Resolving a conflict is long-form work; a modal blocks everything else. Tab lets user switch to terminal to test.
- *Single merged panel with inline markers*: Rejected. Users benefit from seeing ours and theirs side-by-side for context. Three panels matches standard merge UI (VS Code, Zed).

### 6. Sidebar quick-action swap — PR replaces Merge

Currently `SessionRow` shows Commit + Merge on hover (with `onMerge` calling `check_session_has_commits` and opening `MergeModal`). We replace Merge with a "Ship" action that opens the ShipDialog directly for that session (must also set active session before showing dialog, since ShipDialog acts on active session context). Merge moves to an overflow menu (three-dots) in the session row, and remains accessible via `Ctrl+Shift+M` (unchanged) and command palette.

**Alternatives considered:**
- *Leave both buttons visible*: Rejected. Sidebar row is already tight; adding a 3rd action reduces click precision.
- *Remove Merge from row entirely (only shortcut + palette)*: Rejected by user — needs to remain discoverable via mouse.

### 7. Contextual `Ctrl+Shift+R` dispatch

The current handler in `shortcutRegistryAtom` dispatches a single `cluihud:revise-plan` CustomEvent. We change it to a single **dispatcher** that reads the current context and emits the appropriate event:
```ts
handler: () => {
  const activeTab = store().get(activeTabAtom);
  const focusZone = store().get(focusZoneAtom);
  const hasConflicts = store().get(conflictedFilesAtom).length > 0;
  if (activeTab?.type === "conflict") {
    document.dispatchEvent(new CustomEvent("cluihud:resolve-conflict-active-tab"));
  } else if (hasConflicts && (activeTab?.type === "git" || focusZone === "panel" && activeTab?.type === "git")) {
    document.dispatchEvent(new CustomEvent("cluihud:open-first-conflict"));
  } else {
    document.dispatchEvent(new CustomEvent("cluihud:revise-plan"));
  }
}
```

**Alternatives considered:**
- *Separate shortcut for resolve-conflict*: Rejected. Shortcut real-estate is scarce and the semantics are naturally contextual (R for "Resolve/Revise" fits both).
- *Key check at listener level in each component*: Rejected. Spreads decision logic across components; centralized dispatch keeps the truth in one place.

### 8. Global Ship vs. contextual Ship-Enter

Two bindings: `Ctrl+Shift+Y` (global, from registry) and `Ctrl+Shift+Enter` (only when commit textarea is focused, handled in GitPanel's `onKeyDown`). They differ in flow: global Ship always shows the preview dialog because intent is ambiguous (staged or not? message or not?). Textarea Ship-Enter skips the dialog if the textarea has a non-empty message AND staged files exist (intent is explicit), otherwise falls back to opening the dialog.

## Risks / Trade-offs

- [Ship fires accidentally from global shortcut] → Mitigation: preview dialog always shown; user confirms with Enter.
- [CI polling wastes resources for inactive PRs] → Mitigation: polls only while panel is mounted AND PR is OPEN; stops on session switch.
- [`gh` CLI not installed or not authenticated] → Mitigation: `create_pr` already surfaces errors cleanly; we add a check in `ShipDialog` mount that calls a new `gh_available` command to render an inline warning if `gh auth status` fails.
- [Conflict tab leaks state after session switch] → Mitigation: tab is session-scoped like all other tabs (follows existing `tabStateMapAtom` pattern).
- [Contextual Ctrl+Shift+R surprises users in new contexts] → Mitigation: label in command palette reads "Revise Plan / Resolve Conflict (contextual)"; actual action shown as toast on dispatch first time per session.
- [Claude's `/commit` skill commits but doesn't push] → This is the target behavior; Ship-it badge appears, user pushes with one click. Trade-off accepted: two-click minimum for Claude-driven path vs one-click for manual. Acceptable because Claude already did the work.
- [PR template file collision with other tools] → Mitigation: use `.cluihud/pr-template.md` specifically, not `.github/pull_request_template.md`, to avoid interfering with GitHub's native template flow. If user wants GitHub template, `gh pr create` picks it up automatically and we fall back.
- [Three CodeMirror instances in ConflictTab have perf cost] → Mitigation: virtualize/gate rendering on tab activation; ours/theirs are read-only which reduces CM complexity.

## Migration Plan

No data migration. New SQLite tables not required (conflict state is ephemeral). No config file changes. Rollback = revert the change; existing Merge/Commit/PR flows continue to work. Users on main who pull this update see the new PR button replace Merge in the sidebar — a one-time discoverability cost mitigated by the PR button being a superset of the prior Merge intent (PR = reviewed merge).
