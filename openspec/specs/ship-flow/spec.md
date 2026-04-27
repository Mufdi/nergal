# ship-flow Specification

## Purpose
TBD - created by archiving change ship-flow. Update Purpose after archive.
## Requirements
### Requirement: Ship action composes commit, push, and PR atomically
The system SHALL expose a single Ship action that, given a session context, performs (conditionally) commit of staged changes, push to remote, and creation of a pull request in one atomic operation. Ship SHALL be implemented as a single Tauri command that emits progress events per stage.

#### Scenario: Ship with staged changes, no existing PR
- **WHEN** session has staged files, `ahead > 0` or `dirty`, and no existing PR
- **THEN** Ship commits the staged files with the provided message, pushes the branch, creates a PR with the provided title and body, and returns the created PR info

#### Scenario: Ship skips commit when nothing staged
- **WHEN** session has no staged files but `ahead > 0` and no existing PR
- **THEN** Ship skips the commit stage, pushes, creates the PR, and returns PR info

#### Scenario: Ship fails partway and surfaces the failing stage
- **WHEN** commit succeeds but push fails (e.g., network or auth)
- **THEN** Ship returns an error identifying the failed stage (`push`), leaving the local commit intact; no PR is created

#### Scenario: Ship emits progress events per stage
- **WHEN** Ship executes
- **THEN** the backend emits `ship:progress` events with `{ stage, ok }` for each of commit, push, pr as they complete

### Requirement: Explicit Push action
The system SHALL expose an explicit Push action that pushes the current branch of the active session to its remote without creating a PR or committing.

#### Scenario: Push from git panel button
- **WHEN** `ahead > 0 && !prInfo` and user clicks the Push button in the git panel
- **THEN** the system pushes the current branch and refreshes `ahead` to 0

#### Scenario: Push via keyboard shortcut
- **WHEN** user presses the Push shortcut from any focus zone and the active session has `ahead > 0`
- **THEN** the system pushes without showing a dialog and refreshes git state

#### Scenario: Push no-op when nothing ahead
- **WHEN** `ahead === 0`
- **THEN** the Push button is hidden and the Push shortcut shows a toast "Nothing to push"

### Requirement: Ship-it badge surfaces after any commit
The git panel SHALL display a prominent "Ship it" badge in its header whenever the active session has `ahead > 0 && !prInfo && !committing`, regardless of whether the preceding commit was made manually via the git panel or autonomously by Claude via the `/commit` skill.

#### Scenario: Badge appears after manual commit
- **WHEN** user commits via the git panel textarea and the push has not yet happened
- **THEN** the git panel header shows the "Ship it" badge with a Ship button and a Push button

#### Scenario: Badge appears after Claude-driven commit
- **WHEN** Claude runs `/commit` and creates a commit in the session's worktree, and the `files:modified` listener triggers a refresh
- **THEN** the same badge appears with the same actions

#### Scenario: Badge hidden when PR already exists
- **WHEN** `prInfo !== null` for the session
- **THEN** the badge is hidden regardless of `ahead` value; PR status indicator is shown instead

### Requirement: PR preview dialog before create
Before creating a pull request (via Ship or explicit PR action), the system SHALL render a preview dialog with editable title, editable body, and a read-only commit list preview. The user SHALL confirm via Enter or a Ship button, or cancel via Escape.

#### Scenario: Dialog prefills from last commit and range diffstat
- **WHEN** dialog opens for a session with commits in `base..HEAD`
- **THEN** title is set to the subject of the most recent commit, body is populated with the commit subjects list + a diffstat summary, and the commits list preview shows each commit's short hash and subject

#### Scenario: Dialog loads project template
- **WHEN** `.cluihud/pr-template.md` exists in the workspace repo root
- **THEN** the template content is appended to the body prefill after the commits list and diffstat

#### Scenario: Enter confirms, Escape cancels
- **WHEN** dialog is open with a non-empty title and user presses Enter on the confirm button
- **THEN** Ship proceeds with the current title and body
- **WHEN** user presses Escape at any time
- **THEN** dialog closes and no action is taken

#### Scenario: Inline warning when gh CLI not authenticated
- **WHEN** dialog mounts and `gh_available` returns false (not installed or not authenticated)
- **THEN** dialog shows an inline warning and disables the Ship button, with a link to `gh auth login` instructions

### Requirement: PR preview data endpoint
The system SHALL expose a `get_pr_preview_data` Tauri command that returns the structured data needed to prefill the Ship/PR dialog: base branch, commits in `base..HEAD` (hash + subject), diffstat totals (added, removed, files), and optional project template contents.

#### Scenario: Returns commits newest-first, deduplicated by subject
- **WHEN** the session has 5 commits in `base..HEAD` with 2 sharing the same subject
- **THEN** the returned commits list has 4 entries ordered newest-first, duplicates removed by subject

#### Scenario: Returns null template when file absent
- **WHEN** `.cluihud/pr-template.md` does not exist
- **THEN** the response's template field is null (or empty)

### Requirement: CI status polling for active session
When a session has an open PR and its git panel is mounted, the frontend SHALL poll `gh pr checks` for that PR every 20 seconds and display a status indicator (✓ passing, ✗ failing, ⏳ pending) in the git panel header near the PR badge. Polling SHALL stop on session switch or panel unmount.

#### Scenario: Poll starts on mount when PR is open
- **WHEN** git panel mounts for a session with `prInfo.state === "OPEN"`
- **THEN** a polling interval starts and triggers `poll_pr_checks` every 20 seconds

#### Scenario: Indicator reflects latest result
- **WHEN** `poll_pr_checks` returns `{ passing: 3, failing: 1, pending: 0 }`
- **THEN** the indicator renders the failing state (✗) with tooltip showing counts

#### Scenario: Poll stops on session switch
- **WHEN** user switches to a different session
- **THEN** the polling interval for the previous session is cleared

#### Scenario: Poll stops when PR merges or closes
- **WHEN** polling result includes a PR state transition to MERGED or CLOSED
- **THEN** the interval is cleared and the indicator is hidden

### Requirement: Ship via global shortcut
The system SHALL bind a global keyboard shortcut that opens the Ship preview dialog for the active session from any focus zone (terminal, sidebar, panel), subject to the existing focus-zone gating (terminal-zone bypass does not apply to `Ctrl+Shift+*` combinations).

#### Scenario: Ship shortcut from terminal zone
- **WHEN** focus is in terminal and user presses the Ship shortcut
- **THEN** the Ship preview dialog opens for the active session

#### Scenario: Ship shortcut without active session
- **WHEN** no session is active and user presses the Ship shortcut
- **THEN** a toast "No active session" is shown and no dialog appears

### Requirement: Ship via contextual textarea shortcut
When the git panel commit textarea is focused, an additional shortcut SHALL trigger Ship directly using the current textarea contents as the commit message, skipping the preview dialog if both the message is non-empty and there are staged files.

#### Scenario: Ship-Enter with message and staged files
- **WHEN** commit textarea is focused with a non-empty message and there are staged files, and user presses the contextual shortcut
- **THEN** Ship proceeds directly (commit with that message, push, open preview dialog for PR title/body confirmation only)

#### Scenario: Ship-Enter with empty message falls back to dialog
- **WHEN** commit textarea is focused with an empty message and user presses the contextual shortcut
- **THEN** the full Ship preview dialog opens (same as global shortcut path)

