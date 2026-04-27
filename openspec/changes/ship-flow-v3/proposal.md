## Why

The v2 design pivoted during implementation. Live testing surfaced that the 2-step Ship dialog, auto-merge checkbox, visible local-merge button, and split conflict-resolution surface accreted complexity without unifying the user's actual workflow. The user's mental model is a single "Ship" verb that progresses through commit → push → PR → review → merge, with conflict resolution looping back through Claude in the same session, ending in total session cleanup. v3 collapses every git action into one coherent timeline driven by direct manipulation in the right panel: the GitPanel handles pre-PR state (files + commit) and the PR Viewer handles post-PR state (review + annotate + merge), unified by a single Ship modal that tells the user exactly what will happen.

## What Changes

- **BREAKING** ShipDialog rewritten as a single-pane modal with **three progressive action buttons** (no steps, no stepper):
  1. **Commit** — local commit only (`Ctrl+1`)
  2. **Commit + Push** — commit + push to remote, no PR (`Ctrl+2`)
  3. **Commit + Push + PR** — commit + push + create PR; reveals the PR base branch picker inline before final confirm (`Ctrl+3`)
- **BREAKING** Removed `auto_merge_default` config field, `sessionsAutoMergedAtom`, the auto-merge checkbox, and the auto-merge-conflict closed-loop poll. Merging is always an explicit user action inside the PR Viewer; no `gh pr merge --auto` is invoked.
- **BREAKING** Local merge action removed from the GitPanel commit bar. `merge_session` Tauri command stays for compatibility but has no UI surface in the primary flow.
- **BREAKING** Pre-PR Ship modal shows a colored warning banner: "Ship leads to: commit → push → PR → review → merge. Once merged, this session is deleted (worktree, branch, plan files archived first)."
- Add **PR Viewer** as a new right-panel tab type (`pr`):
  - Header: PR number, title, base branch, CI status, link to GitHub
  - Body: full PR diff with cluihud's annotation system + chunk-by-chunk navigation (`↑/↓` or `j/k`) + `a` to annotate the focused chunk
  - Footer: single primary action "Merge into `<base>`" with `Ctrl+Enter` shortcut + secondary "Apply annotations with Claude" when annotations exist
- Add **PRs sidebar mode** to GitPanel:
  - Toggle in the GitPanel sidebar header: **Files** (default) | **PRs**
  - PRs view lists open and recently-closed PRs for the workspace; click opens a `pr` tab
  - Files view stays unchanged (staged / unstaged / untracked)
- Add **annotations-as-instructions loop**:
  - User annotates chunks in the PR Viewer with intent ("use Set instead of Array", "drop this try/catch")
  - "Apply annotations with Claude" button packages PR diff + annotations into a prompt and writes it to the **same session's** terminal (the worktree session that owns the PR)
  - Claude edits the worktree, commits, pushes (existing flow); user refreshes the PR Viewer; applied annotations are marked resolved or removed
  - Loop continues until annotations are empty, then "Merge into `<base>`" closes the cycle
- **Conflicts as a tab**: replace the standalone ConflictsPanel surface with a tab type `conflicts` in the right panel. Accessed via:
  - The existing `Ctrl+Alt+Q` shortcut
  - An automatic link from the PR Viewer when "Merge into `<base>`" returns a mergeable=false reason
  - Manual list in GitPanel when `conflicted_files` is non-empty
- Add **plan archive on cleanup**: before removing a worktree, copy `<worktree>/.claude/plans/*.md` to `<main_repo>/.claude/plans/archive/YYYY-MM/<session_id>/`. Plans persist outside the deleted worktree for auditability. Specs and OpenSpec changes ride with the commit (already git-tracked) and need no separate archive.
- Add **post-merge transition**: when "Merge into `<base>`" succeeds, the system runs cleanup (archive plans → delete worktree/branch/DB row) and switches to the most recently-active remaining session. If none exists, closes the right panel and shows the empty workspace state.
- **Modal shortcut capture**: while the Ship modal is open, `Ctrl+1`/`Ctrl+2`/`Ctrl+3` are captured by the modal for action selection (override the global session-switching shortcuts). Capture phase + modal-open guard prevents bleed.

## Capabilities

### New Capabilities
- `pr-viewer`: A tab type that renders an open or closed pull request inside the right panel — header with PR meta and CI status, body with annotated diff + chunk navigation, footer with Merge action and Claude handoff for applying annotations.
- `pr-list-sidebar`: A toggle in the GitPanel sidebar that swaps the staged/unstaged/untracked list for a list of the workspace's pull requests (open and recently-closed), entry point to open a `pr` tab.
- `plan-archive-on-cleanup`: Pre-cleanup step that moves a session's plan files from the worktree to the main repository's `.claude/plans/archive/YYYY-MM/<session_id>/` directory so the plans persist after the worktree is deleted.

### Modified Capabilities
- `ship-flow`: Replaces the 2-step dialog and Ship/Push/Merge/auto-merge surface area with a single-pane Ship modal exposing three progressive action buttons (Commit / Commit+Push / Commit+Push+PR). Each button has a `Ctrl+N` shortcut while the modal is open. The PR base picker only appears for the third option.
- `conflict-resolution`: Repackaged as a right-panel tab (`conflicts`) instead of a standalone panel. Triggered by `Ctrl+Alt+Q`, by failed PR merge attempt in the PR Viewer, or by the GitPanel inline conflict list. Internal mechanics (3-pane Ours/Theirs/Merged + region nav + Claude handoff) stay.
- `git-panel-v2`: Sidebar gains a Files / PRs toggle. Commit bar drops the Merge button and the auto-merge-related affordances. Header keeps branch + ahead + PR badge + CI; expand-to-Zen button stays.
- `keyboard-shortcuts`: Ship action no longer opens the dialog if there is nothing to ship (existing pre-check stays). Adds modal-scoped `Ctrl+1/2/3` for action selection. Removes auto-merge-related entries.
- `session-cleanup`: Cleanup is invoked from a single trigger — the "Merge into `<base>`" success path in the PR Viewer (no auto-cleanup poll). Sequence: archive plans → delete worktree+branch+DB row → switch to most recent remaining session (or close panels if none).

## Impact

- **Frontend**
  - `src/components/git/ShipDialog.tsx` — full rewrite: single pane, 3 buttons, conditional branch picker, warning banner, `Ctrl+1/2/3` capture
  - `src/components/git/PrViewer.tsx` (new) — PR header + annotated diff body + Merge footer
  - `src/components/git/PrListSidebar.tsx` (new) — Files | PRs toggle + PR list view
  - `src/components/git/GitPanel.tsx` — drop Merge button, auto-merge UI, sessionsAutoMergedAtom usage, `prInfo.state !== "OPEN"` cleanup banner; integrate Files/PRs toggle
  - `src/components/git/ConflictsPanel.tsx` — convert to tab body component (lift any panel-level chrome into the tab host)
  - `src/stores/rightPanel.ts` — add `pr` tab type and openers
  - `src/stores/git.ts` — drop `autoMergeDefaultAtom`, `sessionsAutoMergedAtom`; add atoms for PR list, current PR Viewer state, annotations
  - `src/stores/shortcuts.ts` — drop auto-merge entries; add modal-scoped `Ctrl+1/2/3` (handled in dialog, not registry)
  - `src/components/layout/RightPanel.tsx` — register `pr` tab renderer

- **Backend (Rust)**
  - `src-tauri/src/commands.rs` — add `cleanup_merged_session` plan-archive step (rsync/copy plans before delete); add `list_prs(workspace_id)` for the sidebar list; add `gh_pr_merge(session_id)` Tauri command; remove `git_auto_merge_default` field plumbing if v2 left it
  - `src-tauri/src/worktree.rs` — add `list_prs` helper using `gh pr list --json`; reuse `pr_status` for current-branch fallback
  - `src-tauri/src/config.rs` — remove `git_auto_merge_default` field
  - `src-tauri/src/db.rs` — `delete_session` already exists; no change

- **Compatibility note**
  - The v2 change directory was moved to `openspec/_superseded/ship-flow-v2/` with a SUPERSEDED.md notice; nothing to archive into specs from v2.
  - v3 deltas reference the v1 specs that were already promoted (ship-flow, conflict-resolution, git-panel-v2, keyboard-shortcuts) plus introduce four new capabilities.

## Build contract

### Qué construyo
- Backend: `cleanup_merged_session` con plan-archive step + `list_prs` + `gh_pr_merge` + `get_pr_diff` Tauri commands. Drop `git_auto_merge_default` config field.
- Frontend stores: drop `autoMergeDefaultAtom` y `sessionsAutoMergedAtom`; add `prAnnotationsMapAtom`, `gitSidebarModeAtom`, `pr` tab type en `rightPanel.ts`.
- Frontend components: rewrite ShipDialog (single-pane + 3 progressive buttons + warning banner + Ctrl+1/2/3); new PrViewer + PrListSidebar; convert ConflictsPanel surface a tab body; remove Merge button + auto-merge UI de GitPanel; integrate Files/PRs sidebar toggle.
- Cleanup transition: post-merge switch a sesión más reciente o cold-start.

### Cómo verifico
- Backend: `cd src-tauri && cargo build && cargo test --lib && cargo fmt --check && cargo clippy -- -D warnings`
- Frontend: `npx tsc --noEmit` desde root
- Manual UX walks (run `pnpm tauri dev`):
  1. Open Ship modal sobre worktree con cambios → 3 buttons + warning visible → `Ctrl+3` revela branch picker → completes Commit+Push+PR
  2. PR aparece en sidebar PRs view → click abre tab pr con diff annotated
  3. Annotate 2 chunks → click Apply with Claude → prompt llega al terminal de la sesión
  4. Click Merge into main sin conflict → cleanup runs → toast → switch a próxima sesión
  5. Forzar conflict en otro PR → Merge falla → conflicts tab abre → resolve → Finish merge → cycle completes
  6. Mergear PR externamente desde GitHub → recovery banner aparece en GitPanel → click Cleanup → session deleted

### Criterio de done
- Todos los manual walks pasan sin regresión
- `gh pr` flows (list, diff, merge) responden correctamente con auth válida
- Cleanup deja worktree, branch, DB row, plans-en-worktree borrados; plans archivados a `<main_repo>/.claude/plans/archive/YYYY-MM/<sid>/`
- Sin código zombie de v2: `grep -r "autoMergeDefault\|sessionsAutoMerged\|git_auto_merge_default" src/ src-tauri/` retorna 0 matches
- Tab system soporta `pr` y `conflicts` tab types sin regresión en tabs existentes (diff, file, plan, spec)

### Estimated scope
- **files_estimate**: 13 (10 frontend .tsx/.ts + 3 backend .rs)
- **risk_tier**: critical
- **tags**: [breaking-change, refactor, feature, external-integration]
- **visibility**: private (personal project, no public BUILD-LOG)
