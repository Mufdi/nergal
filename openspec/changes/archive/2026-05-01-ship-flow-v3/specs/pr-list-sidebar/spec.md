## ADDED Requirements

### Requirement: Files / PRs toggle in GitPanel sidebar
The GitPanel sidebar SHALL render a two-state toggle in its header: `Files` (default) shows the staged/unstaged/untracked sections; `PRs` swaps that area for a list of pull requests in the active workspace. The toggle persists per workspace across reloads.

#### Scenario: Default state is Files
- **WHEN** the user opens cluihud and switches to a workspace for the first time
- **THEN** the GitPanel sidebar shows the Files view with staged/unstaged/untracked sections

#### Scenario: Toggle switches to PRs and back
- **WHEN** the user clicks the `PRs` toggle
- **THEN** the sidebar swaps to a list of PRs; clicking `Files` swaps back; the chosen state persists for that workspace

### Requirement: PRs view lists open and recently-closed pull requests
The PRs view SHALL list pull requests for the active workspace, ordered with OPEN PRs first (by most recently updated), followed by recently MERGED or CLOSED PRs (capped at 20 most recent total). Each row shows PR number, title (truncated to fit), state badge, and CI status pill when applicable.

#### Scenario: Lists OPEN PRs first
- **WHEN** the workspace has 2 OPEN PRs and 5 MERGED PRs
- **THEN** the PRs view renders the 2 OPEN PRs at the top, then the 5 MERGED in descending update order

#### Scenario: Empty state when no PRs exist
- **WHEN** the workspace has no PRs (nothing returned by `gh pr list`)
- **THEN** the PRs view renders an empty state "No pull requests yet — Ship a session to open one."

#### Scenario: Click opens the PR Viewer tab
- **WHEN** the user clicks a PR row
- **THEN** a `pr` tab opens or focuses as defined in `pr-viewer`

### Requirement: PR list refresh
The PRs view SHALL refresh on (a) workspace switch, (b) successful Ship that creates a PR (the new PR appears at the top), and (c) every 60 seconds while the PRs view is the active sidebar mode.

#### Scenario: Newly-created PR appears at the top after Ship
- **WHEN** the user completes a Ship that creates PR #N and the PRs view is active
- **THEN** PR #N appears at the top of the list within 2 seconds

#### Scenario: Background refresh while Files view is active
- **WHEN** the user is in the Files view
- **THEN** the PR list does NOT poll (refresh only resumes when the user switches back to the PRs view, to save `gh` rate limits)
