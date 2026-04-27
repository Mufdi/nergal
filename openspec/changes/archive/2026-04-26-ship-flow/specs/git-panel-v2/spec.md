## ADDED Requirements

### Requirement: Ship-it badge in panel header
The git panel header SHALL render a prominent "Ship it" badge whenever the active session has `ahead > 0 && !prInfo && !committing`. The badge SHALL include a Ship button (opens Ship preview dialog) and a Push button (push-only).

#### Scenario: Badge appears for ahead-only state
- **WHEN** session is `ahead > 0` with no open PR and no commit in progress
- **THEN** the header renders the badge with Ship and Push buttons adjacent to the branch indicator

#### Scenario: Badge hidden when PR exists
- **WHEN** `prInfo !== null` for the session
- **THEN** the badge is not rendered; the existing PR pill + link continues to appear

#### Scenario: Badge refreshes on Claude-driven commit
- **WHEN** Claude runs `/commit` and a new commit lands in the worktree, triggering a `files:modified` event
- **THEN** the panel refresh picks up `ahead > 0` and the badge becomes visible without user interaction

### Requirement: CI status indicator next to PR pill
When the active session has an OPEN PR, the git panel header SHALL display a CI status indicator (icon + counts) next to the PR pill, reflecting the latest polled state from `gh pr checks`.

#### Scenario: Passing state
- **WHEN** latest poll returns all checks passing
- **THEN** the indicator renders green ✓ with a tooltip "N checks passing"

#### Scenario: Failing state
- **WHEN** latest poll returns any failing check
- **THEN** the indicator renders red ✗ with a tooltip "X failing / Y total"

#### Scenario: Pending state
- **WHEN** latest poll returns any pending/in-progress check with none failing
- **THEN** the indicator renders yellow ⏳ with a tooltip "N pending"

#### Scenario: Indicator hidden when no PR
- **WHEN** `prInfo === null`
- **THEN** the indicator is not rendered

### Requirement: Conflict list section
When the active session has one or more conflicted files, the git panel SHALL render a "Conflicts (N)" section at the top of the panel above the History section. Each row SHALL display the file path, a C status letter, and a Resolve button that opens or focuses the conflict tab for that file.

#### Scenario: Section appears after conflicting merge
- **WHEN** `merge_session` returns `conflict: true`
- **THEN** the panel fetches conflicted files via `get_conflicted_files` and renders the section

#### Scenario: Resolve button opens conflict tab
- **WHEN** user clicks Resolve on a row
- **THEN** a `conflict` tab opens or focuses in the right panel for that file

#### Scenario: Section disappears when all resolved
- **WHEN** all conflicted files are resolved (merged saved + staged) and the panel refreshes
- **THEN** the conflict list section is not rendered

## MODIFIED Requirements

### Requirement: Fixed commit bar
The git panel SHALL render a fixed commit bar at the bottom with a commit message input and action buttons (Commit, Push, Ship). Push appears when `ahead > 0 && !prInfo`. Ship appears always when there is something to ship (staged OR ahead).

#### Scenario: Commit action
- **WHEN** user types a commit message and clicks Commit
- **THEN** the system SHALL create a git commit with the staged files and provided message

#### Scenario: Push action
- **WHEN** `ahead > 0 && !prInfo` and user clicks Push
- **THEN** the system SHALL push the current branch to the remote without creating a PR

#### Scenario: Ship action
- **WHEN** user clicks Ship
- **THEN** the Ship preview dialog opens for the active session

#### Scenario: Ctrl+Enter commits
- **WHEN** commit textarea is focused with non-empty message and staged files, and user presses Ctrl+Enter
- **THEN** Commit is triggered (unchanged from prior behavior)

#### Scenario: Ctrl+Shift+Enter ships
- **WHEN** commit textarea is focused and user presses Ctrl+Shift+Enter
- **THEN** Ship proceeds (with textarea message if non-empty and staged; otherwise opens Ship dialog)

#### Scenario: Create PR action replaced by Ship
- **WHEN** the prior "Create PR" button would have appeared (`ahead > 0 && !prInfo`)
- **THEN** the Ship button (preview + create PR) is shown in its place; no separate "Create PR" button exists
