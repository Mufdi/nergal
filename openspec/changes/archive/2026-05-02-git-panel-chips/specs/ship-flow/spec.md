## ADDED Requirements

### Requirement: PR Viewer is embedded in the PRs chip
The PR Viewer SHALL render inside the `PRs` chip's body when a PR is selected from the picker. It is no longer wrapped in a document tab.

#### Scenario: Embedded viewer mounts on PR select
- **WHEN** the user picks a PR in `PrsChip` (Enter on focused row)
- **THEN** the chip swaps to viewer mode showing a back-strip ("All PRs" with `Backspace`) and the `<PrViewer>` body fills the rest

#### Scenario: PrViewer keyboard ownership via isActive
- **WHEN** `PrViewer` mounts inside a chip
- **THEN** it relies on its `isActive` prop (default true) to gate the body keydown listener; the previous `activeTabAtom`/`tabId` checks are removed

### Requirement: Apply-with-Claude tooltip explains the disabled state
The Apply-with-Claude button SHALL surface its disabled-state reason via a Tooltip component (not a native browser title). The button label SHALL include the annotation count inline (`Apply with Claude (N)`).

#### Scenario: Tooltip on no matching session
- **WHEN** no local session has a worktree branch matching the PR's `head_ref_name`
- **THEN** the button is disabled and the tooltip reads "No local session matches this PR's branch (`<headRefName>`). Open a session whose worktree is on this branch to apply."

#### Scenario: Tooltip on wrong active session
- **WHEN** an owning session exists but is not the active session
- **THEN** the tooltip reads "Switch to the session on branch `<headRefName>` — its terminal will receive the prompt."

#### Scenario: Tooltip on enabled state
- **WHEN** the owning session is active
- **THEN** the tooltip reads "Send N annotation(s) as a structured prompt to the session terminal."

### Requirement: PR failed-merge routes to Conflicts chip
When `gh_pr_merge` returns `mergeable=false`, PrViewer SHALL flip the workspace's `gitChipModeAtom` entry to `"conflicts"` instead of opening a Conflicts tab. The owning session's `selectedConflictFileMapAtom` entry is cleared so ConflictsPanel falls back to the first conflicted file from the new state.

#### Scenario: Failed merge with owning session
- **WHEN** `gh_pr_merge` returns an error containing `mergeable=false`
- **AND** the PR has an owning session in this workspace
- **THEN** the workspace's `gitChipModeAtom` is set to `"conflicts"`, the owning session's `selectedConflictFileMapAtom` entry is cleared, and a toast informs the user

#### Scenario: Failed merge without owning session
- **WHEN** `gh_pr_merge` returns `mergeable=false`
- **AND** no local session matches the PR's `head_ref_name`
- **THEN** no chip transition fires; an error toast surfaces the message verbatim
