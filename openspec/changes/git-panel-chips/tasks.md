## 1. Backend: git stash commands

- [x] 1.1 Add `StashEntry { index, message, branch, age_seconds, files_changed }` struct in commands.rs
- [x] 1.2 Add `git_stash_list(session_id) -> Vec<StashEntry>` parsing `git stash list --format='%gd|%gs|%cr|%H'`
- [x] 1.3 Add `git_stash_create(session_id, message)` invoking `git stash push -u -m <msg>`
- [x] 1.4 Add `git_stash_apply(session_id, index)` invoking `git stash apply stash@{N}`
- [x] 1.5 Add `git_stash_pop(session_id, index)` invoking `git stash pop stash@{N}`
- [x] 1.6 Add `git_stash_drop(session_id, index)` invoking `git stash drop stash@{N}`
- [x] 1.7 Add `git_stash_show(session_id, index) -> Vec<String>` invoking `git stash show --name-only stash@{N}`
- [x] 1.8 Add `git_stash_branch(session_id, index, branch_name)` invoking `git stash branch <name> stash@{N}`
- [x] 1.9 Register all 7 commands in `lib.rs` invoke_handler
- [x] 1.10 Unit tests for the `git stash list` output parser
- [x] 1.11 `cargo build` clean, `cargo test` passing, `cargo fmt --check` clean

## 2. Frontend: store refactor

- [x] 2.1 Add `ChipMode = "files" | "history" | "stashes" | "prs" | "conflicts"` + `gitChipModeAtom: Record<workspaceId, ChipMode>` in stores/git.ts
- [x] 2.2 ~~Add `gitPanelExpandedAtom: boolean`~~ — superseded: Ctrl+Shift+0 reuses the existing `cluihud:expand-zen-git` dispatch + chipMode-aware branch in shortcuts.ts; in-panel expansion atom is unnecessary
- [x] 2.3 Add `StashEntry` type mirroring backend
- [x] 2.4 Remove `gitSidebarModeAtom`, `GitSidebarMode` type
- [x] 2.5 Remove `openPrTabAction`, `prTabId` from stores/git.ts
- [x] 2.6 Remove `openConflictsTabAction` from stores/conflict.ts (or relocate to chip-internal helper)
- [x] 2.7 Remove `pr` and `conflicts` from `TabType` union in stores/rightPanel.ts
- [x] 2.8 Remove `pr` and `conflicts` entries from `PANEL_CATEGORY_MAP`
- [x] 2.9 `npx tsc --noEmit` clean

## 3. Files chip + GitPanel skeleton

- [x] 3.1 Create `src/components/git/chips/` directory
- [x] 3.2 Rewrite `GitPanel.tsx`: branch header (kept) + chip strip + `<ActiveChip>` body
- [x] 3.3 Chip strip renders 5 buttons with icons + active-state styling matching SpecPanel chip pattern
- [x] 3.4 Extract Files chip into `chips/FilesChip.tsx`: full-width Staged/Changes/Untracked + commit/ship bar at bottom
- [x] 3.5 Preserve current keyboard nav (↑/↓ + j/k + Space stage-toggle)
- [x] 3.6 Preserve current pendingMerge banner + cleanup banner (kept in GitPanel above chip strip — workspace-level state)
- [x] 3.7 `npx tsc --noEmit` clean

## 4. History chip

- [x] 4.1 Extract `chips/HistoryChip.tsx` from current GitPanel left column
- [x] 4.2 Full-width commit list (no graph view in v1 — defer)
- [x] 4.3 ↑/↓ + j/k navigate; Space expands commit's files inline
- [x] 4.4 Enter on a file row opens Zen view via existing openZenMode
- [x] 4.5 `npx tsc --noEmit` clean

## 5. Stashes chip

- [x] 5.1 Create `chips/StashesChip.tsx`: list rows show index/message/age/files_count
- [x] 5.2 Create-from-current-changes input box at bottom (mirrors commit box layout)
- [x] 5.3 Empty state: hint "No stashes yet — save current changes with the box below"
- [x] 5.4 Keyboard: ↑/↓ + j/k navigate; Space expands files; Enter applies; `p` or Shift+Enter pops; `d` drops with inline confirm; `b` opens branch-name input
- [x] 5.5 Toast on each action with refresh of list
- [x] 5.6 `npx tsc --noEmit` clean

## 6. PRs chip + PR Viewer migration

- [x] 6.1 Create `chips/PrsChip.tsx`: file-picker top (PR list with state pill + base→head + age) + viewer bottom
- [x] 6.2 Migrate PrViewer logic into the chip; viewer occupies full space below picker
- [x] 6.3 Apply-with-Claude button shows tooltip on disabled state explaining the gating reason
- [x] 6.4 Annotation count surfaced inline next to the button
- [x] 6.5 Remove `pr` case from `RightPanel.tsx` DocumentContent
- [x] 6.6 Remove `GitPullRequest` icon from `TAB_ICONS` in TabBar.tsx
- [x] 6.7 Delete `src/components/git/PrListSidebar.tsx`
- [x] 6.8 `npx tsc --noEmit` clean

## 7. Conflicts chip + auto-redirect

- [x] 7.1 Create `chips/ConflictsChip.tsx`: file-picker top + ConflictsPanel viewer bottom
- [x] 7.2 Migrate ConflictsPanel.tsx body into the chip; remove its tab-level chrome
- [x] 7.3 Chip label/badge: `Conflicts (N)` with red bg + pulse animation when N>0; opacity-50 when N=0
- [x] 7.4 Auto-switch to PRs chip when conflicts go from >0 → 0 (1.5s delay, activity-gated, disabled in Zen mode)
- [x] 7.5 Remove `conflicts` case from `RightPanel.tsx` DocumentContent
- [x] 7.6 `npx tsc --noEmit` clean

## 8. Chip nav + expansion

- [x] 8.1 Add `Shift+ArrowLeft` / `Shift+ArrowRight` handler in GitPanel cycling through chips (in `chips/ChipStrip.tsx`)
- [x] 8.2 `Ctrl+Shift+0` already dispatches `cluihud:expand-zen-git` from the existing branch-header button + the global shortcut; updated in shortcuts.ts so when chipMode=conflicts it opens conflictsZen instead
- [x] 8.3 ~~Wire expanded atom into right-panel layout~~ — superseded: existing Zen flow covers this
- [x] 8.4 Skip chip nav when focus is in input/textarea/contenteditable (implemented in ChipStrip)
- [x] 8.5 Persist active chip per-workspace via `gitChipModeAtom`
- [x] 8.6 `npx tsc --noEmit` clean

## 9. Cleanup deletions

- [x] 9.1 Delete `src/components/git/PrListSidebar.tsx`
- [x] 9.2 Remove `Ctrl+Alt+Q` shortcut from `stores/shortcuts.ts` + any consumers
- [x] 9.3 Remove dead imports (`gitSidebarModeAtom`, `openPrTabAction`, `openConflictsTabAction`) across the tree
- [x] 9.4 Remove `MergeModal.tsx` if still unused (was kept "for future repos-without-remote" — re-evaluate)
- [x] 9.5 grep `pr:`, `conflicts:`, `prTabId`, `openPrTabAction`, `openConflictsTabAction` → 0 matches in src/

## 10. Verification + spec deltas

- [x] 10.1 `cd src-tauri && cargo build` clean
- [x] 10.2 `cd src-tauri && cargo test --lib` passing
- [x] 10.3 `cd src-tauri && cargo fmt --check` clean
- [x] 10.4 `cd src-tauri && cargo clippy -- -D warnings` no new warnings
- [x] 10.5 `npx tsc --noEmit` clean
- [x] 10.6 Write spec deltas:
   - `specs/git-panel-v2/spec.md` → ADDED chip-strip, Stashes; REMOVED Files/PRs sidebar toggle
   - `specs/conflict-resolution/spec.md` → MODIFIED: surface is a chip not a tab
   - `specs/tab-system/spec.md` → REMOVED `pr` and `conflicts` tab types
   - `specs/ship-flow/spec.md` → MODIFIED: PR Viewer is a chip not a tab; Apply-with-Claude tooltip-on-disabled
- [ ] 10.7 Manual UX walks (run `pnpm tauri dev`) — **pending user verification**:
   - Default chip is Files; ↑/↓+Space stage flow works
   - Shift+→ cycles to History → Stashes → PRs → Conflicts; Shift+← reverses
   - Stashes: create from current changes, apply, pop, drop, branch all work
   - PRs: file-picker selects PR → viewer below; annotation flow works; Apply-with-Claude tooltip explains disabled state
   - Conflicts: chip atenuado when 0, glow+pulse when >0, auto-switch to PRs after resolve
   - Ctrl+Shift+0 expands GitPanel, second press collapses
   - No PR tab and no Conflicts tab in TabBar; Ctrl+Alt+Q does nothing
