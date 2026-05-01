## ADDED Requirements

### Requirement: GitPanel sidebar Files / PRs toggle
The GitPanel sidebar header SHALL render a two-state toggle (`Files` | `PRs`). The Files state shows the existing staged/unstaged/untracked sections. The PRs state replaces that area with the PR list defined in `pr-list-sidebar`.

#### Scenario: Toggle visible in sidebar header
- **WHEN** the GitPanel renders
- **THEN** the sidebar header shows two adjacent buttons or a segmented control: `Files` (selected by default) and `PRs`

#### Scenario: Toggle state persists per workspace
- **WHEN** the user switches to PRs in workspace A, switches to workspace B (also defaulted Files), and returns to A
- **THEN** workspace A's sidebar shows PRs again

### Requirement: PR-merged-or-closed recovery banner stays
When the active session's PR is in a non-OPEN state (MERGED or CLOSED), the GitPanel header SHALL render a banner with "PR #N is `<state>`. Cleanup will delete this session, its worktree, branch and plan files." plus an explicit "Cleanup session" button. This is the recovery path for sessions whose PRs were merged outside cluihud (no auto-cleanup runs in v3).

#### Scenario: Banner appears when PR transitions to MERGED
- **WHEN** the GitPanel poll observes PR state become MERGED
- **THEN** the banner renders with the explicit Cleanup button; cleanup runs only on click

## REMOVED Requirements

### Requirement: Visible local-merge action with dedicated shortcut
**Reason**: User does not use local merge in their workflow. Removed from the GitPanel commit bar entirely. The `merge_session` Tauri command stays for future use but has no UI surface.
**Migration**: Remove the `<Merge>` button from GitPanel commit bar. Remove the `triggerMergeAtom` listener in GitPanel. The `merge-session` shortcut was already removed from the registry.

### Requirement: Single source of git actions in GitPanel
**Reason**: Replaced by v3 architecture: GitPanel sidebar offers Files | PRs; the Ship modal handles commit/push/PR; PR Viewer handles merge. No "single source" line of actions in the commit bar — the commit bar's role narrows to staging visibility (which the Files sidebar shows) and the commit-shortcut textarea (which Ship now subsumes).
**Migration**: Remove the commit bar's Push/Ship/Merge buttons. The textarea + Commit button stay (kept for in-place quick commits without opening the Ship modal).
