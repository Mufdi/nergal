## 1. Cleanup of v2 leftovers

- [x] 1.1 Removed `git_auto_merge_default` field from Config (Rust + TS) + autoMergeDefaultAtom from stores/git.ts; configAtom default updated
- [x] 1.2 Removed `sessionsAutoMergedAtom` and all references (GitPanel state + cleanup hook; ShipDialog import + setter call)
- [x] 1.3 Auto-cleanup-on-MERGED branch already removed in prior pass — `prInfo.state !== "OPEN"` short-circuits poll, banner+button is the only cleanup path
- [x] 1.4 Removed `<Merge>` button + `MergeModal` render + `triggerMergeAtom` consumer + `mergeOpen` state + `canMergeLocal` from GitPanel; MergeModal component file kept for potential future repos-without-remote use
- [x] 1.5 Removed `sessionsAutoMerged.has(sessionId)` prefill effect from ConflictsPanel; intent stays user-controlled

## 2. Backend: plan archive + PR list + merge command

- [x] 2.1 Extended `cleanup_merged_session`: archive plans BEFORE worktree removal via new `archive_plans` helper. Source `<worktree>/.claude/plans/*.md`, dest `<main_repo>/.claude/plans/archive/YYYY-MM/<session_id>/`. Collision-safe with `-N` suffix. Returns `archived_plans_path: Option<String>` in `CleanupResult`. Inline `epoch_secs_to_year_month` helper avoids chrono dep.
- [x] 2.2 Added `list_prs(workspace_id) -> Vec<PrSummary>` invoking `gh pr list --state all --limit 20`; sorted OPEN first then by `updatedAt` desc
- [x] 2.3 Added `gh_pr_merge(session_id, strategy?)` defaulting to squash; detects `not mergeable` / `merge conflict` in stderr and returns `mergeable=false: <msg>` error so the frontend routes to the conflicts tab
- [x] 2.3b Added `get_pr_diff(workspace_id, pr_number) -> String` invoking `gh pr diff`; surfaces stderr verbatim for inline error rendering
- [x] 2.4 Registered `list_prs`, `get_pr_diff`, `gh_pr_merge` in lib.rs invoke_handler
- [x] 2.5 Backend: `cargo build` clean, 30/30 tests pass, `cargo fmt --check` clean

## 3. Frontend: ShipDialog single-pane rewrite

- [x] 3.1 Rewrote ShipDialog: single pane, 3 action buttons (Commit / Commit + Push / Commit + Push + PR). Removed Step1Stage, Step2CommitPr, Stepper subcomponents. BranchPicker kept as conditional sub-component
- [x] 3.2 Orange warning banner above form: "Ship leads to: commit → push → PR → review → merge. Once the PR is merged, this session is deleted (worktree, branch). Plans archived first to .claude/plans/archive/"
- [x] 3.3 Modal-scoped Ctrl+1/2/3 capture in `onKey` (capture phase); each fires `dispatchAction(<variant>)`; Esc closes; Ctrl+Enter fires the armed action
- [x] 3.4 BranchPicker rendered inline next to Body label only when `armedAction === "commit-push-pr"`; hidden for Commit and Commit+Push
- [x] 3.5 `git_ship` invoked only by `runCommitPushPr`; `runCommit` uses `git_commit`, `runCommitPush` uses `git_commit` + `git_push`. `targetBranch` is passed only in the PR variant
- [x] 3.6 "About to: <action>" indicator above footer; updates via onMouseEnter/onFocus on each button; default armed = `commit-push-pr` (or `commit-push` when an OPEN PR exists)
- [x] 3.7 Removed auto-merge checkbox entirely; Stepper subcomponent gone (no overlap with Dialog X)
- [x] 3.8 No `autoFocus` on action buttons; user must explicitly click or press Ctrl+1/2/3

## 4. Frontend: PR Viewer tab

- [x] 4.1 Added `pr` to `TabType` + `PANEL_CATEGORY_MAP` in `src/stores/rightPanel.ts`. `openPrTabAction` lives in `stores/git.ts` next to the PR state (keyed by `pr:${workspaceId}:${prNumber}` via `prTabId`); reuses `openTabAction` so it focuses an existing tab or pins a new one. Extended `PrSummary` with `head_ref_name` (backend + TS) so the action can pass everything the viewer needs in `tab.data`.
- [x] 4.2 `src/components/git/PrViewer.tsx` — header (PR # + title + state pill + CI pill + GitHub link), body (annotated diff with chunk nav), footer (Merge into base + Apply-with-Claude when annotations exist). New `get_pr_checks(workspace_id, pr_number)` Tauri command for workspace-scoped CI fetch (existing `poll_pr_checks` was session-scoped).
- [x] 4.3 Self-contained `parsePrDiff` (PrViewer.tsx) — handles multi-file `gh pr diff` output, emits `file` boundary rows, and tracks hunkIndex globally. `j/k` and `↑/↓` navigate hunks; `a` enters annotation edit mode for the focused hunk. Sticky file headers keep the path in view.
- [x] 4.4 Added `prAnnotationsMapAtom: Record<string, PrAnnotation[]>` (keyed by `prAnnotationsKey(wsId, prNumber)`) plus `PrAnnotation { id, hunkIndex, text }` in `src/stores/git.ts`. v3 MVP: in-memory only — annotations vanish on app restart, matching the "annotations only exist if they drive Claude" rule.
- [x] 4.5 Apply-with-Claude button is gated on `isOwningSessionActive` (active session's branch matches the PR's `head_ref_name`). Builds a structured prompt with `### <file> @@ <hunk label>` anchors per annotation and writes via `terminal_paste`. Annotations are not auto-resolved — user clears them manually after Claude confirms the change.
- [x] 4.6 Merge button calls `gh_pr_merge(workspaceId, prNumber)` (refactored from session-scoped to workspace-scoped — passes `gh pr merge <number>` so the Viewer can drive any PR). On `mergeable=false` it opens the conflicts tab via `openConflictsTabAction`. Unresolved annotations trigger a "Merge anyway" inline confirm. Success path runs `cleanup_merged_session(owningSessionId)` + toast (phase 7 will add the post-cleanup session switch).
- [x] 4.7 `RightPanel.tsx` — `pr` case in `DocumentContent` renders `<PrViewer data={...} tabId={...} />`; added `GitPullRequest` icon to `TAB_ICONS` (also in `TabBar.tsx`); `viewPanelLabel` gets `pr: "PR"`.
- [x] 4.8 PR-tab-scoped `Ctrl+Enter` lives inside the viewer's keydown handler (capture phase, gated on `activeTab.id === tabId`). When the annotation textarea is open, Ctrl+Enter saves the annotation instead — Esc cancels.

## 5. Frontend: PRs sidebar mode

- [ ] 5.1 Add `gitSidebarModeAtom` per workspace: `Map<workspaceId, "files" | "prs">` defaulting to "files"
- [ ] 5.2 Create `src/components/git/PrListSidebar.tsx`: invokes `list_prs`, renders open-first ordered list with PR number, title (truncated), state badge, CI pill
- [ ] 5.3 Add Files | PRs toggle to GitPanel sidebar header (segmented buttons)
- [ ] 5.4 Conditional rendering in GitPanel: show staged/unstaged/untracked sections OR PrListSidebar based on the toggle state
- [ ] 5.5 Click handler on a PR row dispatches `openPrTabAction`
- [ ] 5.6 Refresh logic: refresh PRs on workspace switch + after Ship-creates-PR + every 60s while PRs view is active (stop polling when toggled to Files)

## 6. Frontend: Conflicts as a tab

- [ ] 6.1 Convert `ConflictsPanel.tsx` body into a tab body component (lift any panel-level chrome into a tab host wrapper)
- [ ] 6.2 Add `conflicts` tab type to `src/stores/rightPanel.ts` if not already present; ensure tab opens via `Ctrl+Alt+Q` and via failed PR merge
- [ ] 6.3 Implement auto-close of the `conflicts` tab when `conflictedFiles.length === 0 && !pendingMerge`
- [ ] 6.4 Update GitPanel inline conflicts list to dispatch the tab opener instead of the prior panel toggle

## 7. Backend + Frontend: cleanup transition

- [ ] 7.1 In the cleanup success handler (in PR Viewer Merge flow OR GitPanel recovery banner), after `cleanup_merged_session` returns successfully, look up the workspace's most recently-`updated_at` remaining session; if found, set `activeSessionIdAtom` to it and open/focus its tab; otherwise clear `activeSessionIdAtom` and close the right panel
- [ ] 7.2 Toast on transition: "Switched to session: <name>" when a successor exists; "Session closed. Workspace empty." when none

## 8. Cleanup leftover code

- [ ] 8.1 Remove `Stepper` subcomponent and Step1/Step2 subcomponents from ShipDialog
- [ ] 8.2 Remove unused imports across the affected files
- [ ] 8.3 Remove the v2-era visible Merge button code from GitPanel
- [ ] 8.4 Remove `git_auto_merge_default` and any of its propagation (config defaults, atom, types)

## 9. Verification

- [ ] 9.1 Run `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 9.2 Run `npx tsc --noEmit` from project root
- [ ] 9.3 Manual UX walks (run `pnpm tauri dev`):
   - Modal opens with 3 buttons + warning + Ctrl+1/2/3 fire correctly
   - PR base picker appears only when Commit+Push+PR is the focused action
   - Ship a session with no commits but staged work using Commit+Push+PR — title used as commit subject — push + PR created
   - Click PR in PRs sidebar — opens pr tab — annotate chunks — Apply with Claude writes to session terminal
   - Merge into main — session-cleanup runs — toast confirms — switches to next session
   - Merge with conflict — conflicts tab opens — resolve — push — re-merge succeeds
   - Cleanup recovery banner appears when a PR is detected as MERGED externally
- [ ] 9.4 Update `CLAUDE.md` if any new project conventions emerge that affect documentation
