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

- [ ] 4.1 Add `pr` tab type to `src/stores/rightPanel.ts`; add `openPrTabAction` atom that opens or focuses a `pr` tab keyed by `(workspaceId, prNumber)`
- [ ] 4.2 Create `src/components/git/PrViewer.tsx`: header (PR meta + CI), body (annotated diff with chunk nav), footer (Merge button + Apply-with-Claude when annotations exist)
- [ ] 4.3 Body uses the existing diff component (reuse the chunk-rendering logic from DiffView); add `↑/↓` and `j/k` navigation; add `a` to annotate the focused chunk
- [ ] 4.4 Add per-PR annotations atom in `src/stores/git.ts`: `prAnnotationsMapAtom: Map<string, Annotation[]>` keyed by `${workspaceId}:${prNumber}`; persistence via existing annotation system if available (else session-only for v3 MVP)
- [ ] 4.5 "Apply annotations with Claude" footer button: enabled only when `activeSessionId === pr.sessionId AND annotations.length > 0`; on click, build a structured prompt (PR diff + annotations with chunk anchors) and write to the session's PTY via `terminal_paste` or similar; do NOT auto-mark annotations applied
- [ ] 4.6 "Merge into `<base>`" footer button: invokes `gh_pr_merge`; on success, triggers session-cleanup; on `mergeable=false` error, opens the `conflicts` tab; gated by unresolved annotations with a `Merge anyway` confirm
- [ ] 4.7 Register `pr` tab renderer in `src/components/layout/RightPanel.tsx`
- [ ] 4.8 Wire `Ctrl+Enter` shortcut for Merge when `pr` tab is the focused tab

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
