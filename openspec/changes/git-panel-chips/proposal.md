# GitPanel: 5-chip dedicated navigation

## Why

The current GitPanel has grown two surfaces that fight each other:

1. A two-column layout (commit history left, files-or-PRs sidebar right) where the right column toggles between **Files** and **PRs** via a segmented control. Focus switching between the two columns is implicit; keyboard nav (`↑/↓` + `Space`) only works on the right column.
2. PR Viewer + Conflicts each live as **document tabs** in `TabBar`, opened via `openPrTabAction` / `openConflictsTabAction`. This bleeds git-specific UX into the global tab system, makes PRs feel transient (close-x button suggests they shouldn't be there), and forces session-scoped views into a workspace-scoped surface.

After ship-flow-v3 manual UX walks, the user reported:
- BranchPicker only appears when the **mouse hovers** the `Commit+Push+PR` button — keyboard-armed actions don't reveal it.
- The "Apply with Claude" button on PR Viewer renders disabled with no hover hint explaining why (gating is silent).
- The PR tab shows a close-x that doesn't fit the mental model — PRs are a list filter, not a document.
- Files vs Commits-history focus-switching is ambiguous.

The fix is to lift git-specific surfaces into a **dedicated chip strip inside GitPanel**, mirroring the pattern already proven in `SpecPanel` (`Shift+←/→` between sub-tabs). Each chip owns its full layout, has its own keyboard nav and lifecycle, and is reachable in <1 keystroke.

## What changes

### New chip system
Replace the Files/PRs sidebar toggle with a **5-chip strip** at the top of GitPanel, immediately below the branch header:

| Chip | Layout | Purpose |
|------|--------|---------|
| **Files** | Full-width sections (Staged/Changes/Untracked) + commit box at bottom | Stage/unstage + commit |
| **History** | Full-width commit list with expand-to-files | Browse commits + open file diff in Zen |
| **Stashes** | Full-width stash list + create-stash box at bottom | Manage `git stash` stack |
| **PRs** | File-picker of PRs (top) + PR Viewer (bottom, full space) | Browse + annotate + apply-with-claude + merge |
| **Conflicts** | File-picker of conflict files (top) + ConflictsPanel viewer (bottom) | Resolve conflicts |

### Chip navigation
- `Shift+←` / `Shift+→` cycle between chips (same pattern as SpecPanel:380).
- Per-workspace persistence of the active chip (`gitChipModeAtom: Record<workspaceId, ChipMode>`).
- `Ctrl+Shift+0` enters expansion mode for the whole GitPanel (parity with diff viewer + plan/spec panels).

### Conflicts chip dynamics
- Shown atenuated (50% opacity) when 0 conflicts.
- Pulse + red glow ring when count > 0; chip label shows `Conflicts (N)`.
- **Auto-switch to PRs chip** when conflicts transition from >0 → 0 (activity-gated, same logic as current ConflictsPanel auto-close in phase 6.3 of ship-flow-v3). 1.5s delay to let the user breathe.
- Disabled in Zen mode (no surprise jumps).

### Removals (kill list)
- `Ctrl+Alt+Q` shortcut (Conflicts tab toggle) — replaced by `Shift+←/→` to the Conflicts chip.
- `openPrTabAction` + `prTabId` from `stores/git.ts` (PRs no longer document tabs).
- `openConflictsTabAction` from `stores/conflict.ts` (Conflicts no longer document tab).
- `pr` + `conflicts` entries from `TabType` in `stores/rightPanel.ts` and from `PANEL_CATEGORY_MAP`.
- `gitSidebarModeAtom` + `GitSidebarMode` type — replaced by `gitChipModeAtom`.
- `PrListSidebar.tsx` — its rendering moves into the PRs chip's file-picker.
- The PR-tab `case` in `RightPanel.tsx` `DocumentContent` and the `conflicts` case in routing.

### Backend additions
New Tauri commands wrapping `git stash`:
- `git_stash_list(session_id) -> Vec<StashEntry>` — index, message, branch, age_seconds, files_changed
- `git_stash_create(session_id, message: String) -> ()` — `git stash push -m <msg>`
- `git_stash_apply(session_id, index: u32) -> ()` — `git stash apply stash@{N}`
- `git_stash_pop(session_id, index: u32) -> ()` — `git stash pop stash@{N}`
- `git_stash_drop(session_id, index: u32) -> ()` — `git stash drop stash@{N}`
- `git_stash_show(session_id, index: u32) -> Vec<String>` — files in the stash
- `git_stash_branch(session_id, index: u32, branch_name: String) -> ()` — `git stash branch <name> stash@{N}`

NOT included: `git stash clear` (destructive bulk delete, no GUI surface — CLI only).

### PR Viewer fixes (carried from ship-flow-v3 manual walks)
- "Apply with Claude" button shows a tooltip explaining gating: "Disabled — no active session matches the PR's branch (`<head_ref_name>`)" or "Disabled — no annotations to apply".
- Annotation count surfaced inline next to the button so users see why apply is enabled.

### NOT in scope (deferred)
- The remaining ship-flow-v3 manual-walk bugs (1, 2, 3, 7) — tracked separately, will be addressed after this refactor lands. The chip refactor doesn't fix them; it's a foundation that makes them easier to fix.
- `git stash clear` UI.
- Branches chip + Reflog chip — proposed as follow-up changes (not bundled here).
- Annotation persistence (still in-memory per ship-flow-v3 MVP rule).

## Build contract

### Qué construyo
1. Backend: 7 `git_stash_*` Tauri commands + `StashEntry` struct + tests for parser of `git stash list --format`.
2. Frontend stores:
   - `gitChipModeAtom: Record<string, ChipMode>` with `ChipMode = "files" | "history" | "stashes" | "prs" | "conflicts"`.
   - `gitPanelExpandedAtom: boolean` for `Ctrl+Shift+0` expansion mode.
   - Removal of `gitSidebarModeAtom`, `openPrTabAction`, `prTabId`, `openConflictsTabAction`, `pr`/`conflicts` from `TabType`.
3. Frontend components (5 new chip components, all under `src/components/git/chips/`):
   - `FilesChip.tsx` — extracted from current GitPanel right-sidebar logic, full width.
   - `HistoryChip.tsx` — extracted from current GitPanel left column, full width.
   - `StashesChip.tsx` — new.
   - `PrsChip.tsx` — file-picker (PR list) + PR Viewer below.
   - `ConflictsChip.tsx` — file-picker (conflict list) + ConflictsPanel below.
4. `GitPanel.tsx` rewritten as: branch header + chip strip + active chip body + global commit/ship bar (rendered only when chip is Files).
5. `Shift+←/→` chip navigation + `Ctrl+Shift+0` expansion handling in GitPanel.
6. PR Viewer "Apply with Claude" tooltip-on-disabled.
7. Conflicts chip pulse/glow + auto-switch to PRs on resolve.
8. Removal of `Ctrl+Alt+Q` shortcut + `RightPanel.tsx` PR/conflicts tab routing.

### Cómo verifico
- `cd src-tauri && cargo build` — clean.
- `cd src-tauri && cargo test --lib` — all pass + new tests for stash list parser.
- `cd src-tauri && cargo fmt --check` — clean.
- `cd src-tauri && cargo clippy -- -D warnings` — no new warnings.
- `npx tsc --noEmit` — clean.
- `pnpm tauri dev` — manual UX walks per chip (see Criterio de done).

### Criterio de done
1. **Files chip** — open GitPanel, default chip is Files. ↑/↓ + j/k navigate, Space stages/unstages. Commit box at bottom. Ctrl+Enter commits.
2. **History chip** — `Shift+→` from Files reaches History. ↑/↓ navigate, Space expands commit's files inline, Enter on a file opens Zen view.
3. **Stashes chip** — `Shift+→` reaches Stashes. Empty state shows "No stashes yet" + create box. Create from current changes works. Apply (Enter), Pop (p), Drop (d with inline confirm) all work. Space expands to show files.
4. **PRs chip** — `Shift+→` reaches PRs. File-picker lists PRs, ↑/↓ navigate, Enter opens PR Viewer below. Annotation flow (j/k for hunks, `a` for annotate, Ctrl+Enter saves) works inside the embedded viewer. "Apply with Claude" disabled state shows tooltip explaining why.
5. **Conflicts chip** — `Shift+→` reaches Conflicts. Atenuado when 0, pulse+glow when >0. After resolving all conflicts, auto-switches to PRs chip after 1.5s. Disabled jump in Zen mode.
6. **No PR tab in TabBar** — opening a PR no longer creates a document tab. The PR-tab close-x is gone.
7. **No conflicts tab in TabBar** — same.
8. **Ctrl+Shift+0** expands the entire GitPanel to fill the right panel; second press collapses.
9. **`Ctrl+Alt+Q` is gone** — pressing it in any context does nothing.

### Estimated scope
- files_estimate: 14
- risk_tier: medium
- tags: [feature, refactor]
- visibility: private
- spec_target: git-panel-v3
