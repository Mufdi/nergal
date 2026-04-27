## ADDED Requirements

### Requirement: Single-pane Ship modal with three progressive action buttons
The Ship modal SHALL render as a single pane (no steps, no stepper, no Back/Next chrome) with three primary action buttons in the footer:

- **Commit** (`Ctrl+1`) — local commit only; requires non-empty title (used as commit subject); does not contact the remote.
- **Commit + Push** (`Ctrl+2`) — local commit + push to remote; does not create a PR.
- **Commit + Push + PR** (`Ctrl+3`) — local commit + push + create a pull request via `gh pr create`; requires title and body; reveals the PR base branch picker inline.

The modal SHALL display a colored warning banner above the form: "Ship leads to: commit → push → PR → review → merge. Once merged, this session is deleted (worktree, branch, plan files archived first)."

#### Scenario: Modal opens with three buttons and warning banner
- **WHEN** the user invokes Ship on a session with stageable changes
- **THEN** the modal renders with the warning banner, title input, body textarea, and three buttons: Commit, Commit + Push, Commit + Push + PR

#### Scenario: Ctrl+1 fires Commit action
- **WHEN** the modal is open and the user presses Ctrl+1
- **THEN** the Commit action runs (local commit using the title as the commit subject); on success, the modal closes and a toast confirms

#### Scenario: Ctrl+3 reveals PR base picker inline
- **WHEN** the user hovers or focuses the Commit + Push + PR button (or presses Ctrl+3 once)
- **THEN** the PR base branch picker appears inline in the modal (next to the body label) with the workspace base preselected; pressing Ctrl+3 again confirms and ships

### Requirement: PR base branch picker is conditional on the PR action
The PR base branch picker SHALL be hidden when the chosen action is Commit or Commit + Push and SHALL appear inline when the chosen action is Commit + Push + PR.

#### Scenario: Picker hidden for Commit and Commit+Push
- **WHEN** the user is hovering or focused on Commit or Commit + Push
- **THEN** no PR base picker is visible

#### Scenario: Picker visible for Commit+Push+PR
- **WHEN** the user is hovering or focused on Commit + Push + PR
- **THEN** the PR base picker (custom dropdown, OS-themed, keyboard-navigable) appears with the workspace's default base preselected

### Requirement: Ship modal pre-check still applies
The Ship modal SHALL not be opened when there is nothing to ship (zero commits ahead, zero staged, zero unstaged/untracked). This pre-check is enforced in the shortcut handler that triggers the modal.

#### Scenario: Nothing to ship surfaces a toast instead of opening modal
- **WHEN** the user invokes Ship via shortcut on a session with no commits, no staged, no unstaged
- **THEN** a toast appears "Nothing to ship — no commits ahead, no staged changes, no unstaged work" and the modal does NOT open

## REMOVED Requirements

### Requirement: Ship dialog uses 2-step progressive disclosure
**Reason**: Live testing showed the staging step felt like a separate concern even though it's implicit in committing. v3 collapses to a single pane with three named actions.
**Migration**: ShipDialog component is rewritten; consumers don't need to migrate state shape (the trigger atom signature is unchanged).

### Requirement: Auto-merge default persists in config
**Reason**: Auto-merge checkbox removed entirely. Merge is an explicit action in the PR Viewer, not a Ship-time toggle.
**Migration**: `git_auto_merge_default` config field SHALL be removed from `Config` (Rust). Frontend `autoMergeDefaultAtom` removed. No data migration needed (the config field is read on load; missing values default to `false`/absent and don't affect anything since the consumer is gone).

### Requirement: Ship triggers session cleanup on confirmed PR-merged state
**Reason**: Auto-cleanup poll caused "session vanished without confirmation" bug. v3 invokes cleanup only on explicit user action: clicking "Merge into `<base>`" in the PR Viewer (success path) or clicking the recovery banner in the GitPanel.
**Migration**: Remove the GitPanel poll's MERGED-detection cleanup invocation. The recovery-banner cleanup path stays.
