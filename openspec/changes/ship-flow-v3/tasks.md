## 1. Cleanup of v2 leftovers

- [ ] 1.1 Remove `git_auto_merge_default` field from `Config` in `src-tauri/src/config.rs` and from `lib/types.ts` `Config` interface; remove `autoMergeDefaultAtom` from `src/stores/git.ts`
- [ ] 1.2 Remove `sessionsAutoMergedAtom` from `src/stores/git.ts` and all references (GitPanel, ShipDialog)
- [ ] 1.3 Remove the `prInfo.state === "MERGED"` auto-cleanup branch from GitPanel's PR poll (keep only the recovery banner path)
- [ ] 1.4 Remove the `<Merge>` button from GitPanel commit bar; remove the `triggerMergeAtom` listener in GitPanel; remove `MergeModal` rendering from GitPanel (keep MergeModal component for future external use)
- [ ] 1.5 Remove the auto-merge-conflict closed-loop wiring in ConflictsPanel (the `sessionsAutoMerged.has(sessionId)` template prefill effect)

## 2. Backend: plan archive + PR list + merge command

- [ ] 2.1 Extend `cleanup_merged_session` in `src-tauri/src/commands.rs` to copy `<worktree>/.claude/plans/*.md` into `<main_repo>/.claude/plans/archive/YYYY-MM/<session_id>/` BEFORE worktree removal; create archive dir if missing; append `-N` suffix on collision; return the archive path in `CleanupResult.archived_plans_path`
- [ ] 2.2 Add `list_prs(workspace_id: String) -> Vec<PrSummary>` Tauri command in `src-tauri/src/commands.rs` that invokes `gh pr list --json number,title,state,url,updatedAt,baseRefName --limit 20 --state all` in the workspace repo; parse + return ordered (OPEN first, then MERGED/CLOSED by `updatedAt` desc)
- [ ] 2.3 Add `gh_pr_merge(session_id: String, strategy: "squash" | "merge" | "rebase") -> Result<(), String>` Tauri command that invokes `gh pr merge --<strategy>`; returns specific error variant for `mergeable=false` so the frontend can open the conflicts tab. Default strategy: squash; spec calls out squash as primary, others available for advanced flows
- [ ] 2.3b Add `get_pr_diff(workspace_id: String, pr_number: u32) -> Result<String, String>` Tauri command that invokes `gh pr diff <pr_number>` in the workspace repo and returns the unified diff text; surface `gh` errors verbatim to the frontend for the inline error path
- [ ] 2.4 Register the new commands in `src-tauri/src/lib.rs` `invoke_handler`
- [ ] 2.5 Backend build + tests: `cargo build`, `cargo test --lib`, `cargo fmt --check`, `cargo clippy -- -D warnings`

## 3. Frontend: ShipDialog single-pane rewrite

- [ ] 3.1 Rewrite `src/components/git/ShipDialog.tsx` as a single-pane modal with three primary action buttons (Commit, Commit + Push, Commit + Push + PR); remove `step` state, Stepper, Step1Stage, Step2CommitPr subcomponents; keep BranchPicker as conditional sub-component
- [ ] 3.2 Add the colored warning banner above the form: "Ship leads to: commit → push → PR → review → merge. Once merged, this session is deleted (worktree, branch, plan files archived first)."
- [ ] 3.3 Wire `Ctrl+1`/`Ctrl+2`/`Ctrl+3` capture inside the modal (capture phase) to dispatch the corresponding action; ensure they bypass global session-switching shortcuts
- [ ] 3.4 Show the BranchPicker inline (next to the Body label) only when the focused or last-hovered action is Commit + Push + PR; hide for the other two
- [ ] 3.5 Update `git_ship` invocation: pass `target_branch` only for the PR variant; pass `null` for Commit and Commit + Push
- [ ] 3.6 Add a small "Action: <Commit | Commit + Push | Commit + Push + PR>" indicator next to the action buttons that updates on hover/focus so the user always knows which action is "armed" (helps with the conditional UI)
- [ ] 3.7 Remove the auto-merge checkbox; remove the Stepper indicator overlap with the Dialog X
- [ ] 3.8 Remove `autoFocus` from action buttons (already done in v2 — verify it stays)

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
