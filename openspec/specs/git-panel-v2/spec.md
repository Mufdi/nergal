---
status: archived
implemented: 2026-03-25
archived: 2026-04-04
files:
  - src/components/git/GitPanel.tsx
  - src/stores/git.ts
---

## Purpose

Provide a git panel with commit history graph, staged/unstaged/stashed file sidebar, and commit/push/PR actions.

## Implementation Notes

All requirements implemented. Two-area layout: history (left) + file sections (right). Graph view with visual connectors. Commit (Ctrl+Enter), push, and PR creation. Zen mode integration via openZen function.

## Requirements

### Requirement: Git panel two-area layout
The system SHALL render the git panel with a main content area (history/timeline), a sidebar (staged/unstaged/stashed files), and a fixed commit bar at the bottom.

#### Scenario: Git panel layout renders correctly
- **WHEN** user opens the git panel
- **THEN** the panel SHALL show history/timeline in the main area, file sections in the sidebar, and commit controls fixed at the bottom

### Requirement: Session commit history with graph
The main area of the git panel SHALL display the session's commit history with two toggleable view modes: graph view (visual commit graph with branches/merges) and list view (simple commit list with message + timestamp).

#### Scenario: Graph view shows visual commit tree
- **WHEN** graph view is active
- **THEN** the main area SHALL render a visual commit graph showing branches, merges, and commit messages for the session

#### Scenario: List view shows simple history
- **WHEN** list view is active
- **THEN** the main area SHALL render a chronological list of commits with message, timestamp, and short hash

#### Scenario: Toggle between views
- **WHEN** user clicks the view toggle button in the history header
- **THEN** the view SHALL switch between graph and list modes

#### Scenario: Commit click opens Zen Mode
- **WHEN** user clicks a commit in either view mode
- **THEN** Zen Mode SHALL activate showing that commit's diff

### Requirement: Git sidebar with staged/unstaged/stashed sections
The git sidebar SHALL display three collapsible sections: Staged, Unstaged, and Stashed. Each section SHALL list files with stage/unstage actions.

#### Scenario: Staged files section
- **WHEN** there are staged files
- **THEN** the Staged section SHALL list each file with an option to unstage

#### Scenario: Unstaged files section
- **WHEN** there are unstaged modified files
- **THEN** the Unstaged section SHALL list each file with an option to stage

#### Scenario: Stashed section
- **WHEN** there are stashed changes
- **THEN** the Stashed section SHALL list stash entries with apply/drop options

#### Scenario: File click opens Zen Mode diff
- **WHEN** user clicks a file in any sidebar section
- **THEN** Zen Mode SHALL activate showing that file's diff

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

### Requirement: Git sidebar persists in Zen Mode
When Zen Mode is activated from the git panel, the git sidebar SHALL remain visible alongside the diff overlay for continuous file navigation.

#### Scenario: Sidebar visible during Zen Mode
- **WHEN** Zen Mode is activated from a git panel file click
- **THEN** the git sidebar SHALL remain visible to the right of the diff overlay

#### Scenario: File navigation updates diff
- **WHEN** user clicks a different file in the git sidebar during Zen Mode
- **THEN** the diff overlay content SHALL update to the newly selected file

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

### Requirement: Conflicts chip pulses when count > 0
The `Conflicts` chip SHALL render with a red ring + pulse animation whenever the active session has unresolved conflicts. When the count is zero and the chip is not active, it SHALL render dimmed (opacity-50) to deprioritize it visually.

#### Scenario: Pulse appears at first conflict
- **WHEN** `activeConflictedFilesAtom.length` transitions from 0 to a positive number
- **THEN** the Conflicts chip in the strip pulses + glows red and shows the count next to its label

#### Scenario: Auto-route to Conflicts chip on first conflict
- **WHEN** the active session has 0 conflicts and a poll surfaces ≥1 conflict
- **THEN** the workspace's chipMode flips to `conflicts`, the first conflicted file is pre-selected, and the right panel expands

### Requirement: Conflicts chip auto-resolves to PRs chip
When the user resolves all conflicts in the Conflicts chip, the GitPanel SHALL switch to the PRs chip 1.5s after activity drains, **provided** the panel had recorded activity (was non-empty at some point) and is not in Zen mode.

#### Scenario: Resolve last conflict in chip mode
- **WHEN** the user finalizes the last conflict resolution in the Conflicts chip
- **AND** files.length transitions from >0 to 0
- **THEN** after 1500ms the workspace's chipMode is set to `prs`

#### Scenario: Zen mode skips auto-resolve
- **WHEN** the user resolves the last conflict while ConflictsPanel is rendered in Zen mode (`inZen=true`)
- **THEN** no chip transition fires

### Requirement: Stashes chip exposes the git stash stack
The `Stashes` chip SHALL provide a list of `git stash list` entries (index, branch, message, age) plus a create-stash input box at the bottom. Each row supports apply (Enter), pop (`p` or `Shift+Enter`), drop (`d`, two-press confirm with 3s timeout), branch (`b` opens a name input), and expand (Space, lazy-loads files via `git_stash_show`).

#### Scenario: Empty state
- **WHEN** the chip is active and the repo has no stashes
- **THEN** the body shows "No stashes yet" + a hint pointing to the create input

#### Scenario: Drop requires double-press
- **WHEN** the user presses `d` on a focused stash row
- **THEN** the row enters confirm state for 3 seconds; pressing `d` again within that window drops the stash; otherwise it auto-cancels

#### Scenario: Branch from stash
- **WHEN** the user presses `b` on a focused row
- **THEN** an inline branch-name input appears; submitting with Enter calls `git stash branch <name> stash@{N}`

### Requirement: PRs chip is a picker → embedded viewer
The `PRs` chip SHALL render a file-picker-style PR list (j/k/Enter to select). Selecting a PR replaces the chip body with the `PrViewer` embedded in place plus a "All PRs" back-strip. `Backspace` returns to the picker.

#### Scenario: Backspace returns to picker
- **WHEN** the user is viewing a PR (`selected !== null`)
- **AND** presses `Backspace` outside an input/textarea
- **THEN** the chip returns to picker mode

#### Scenario: Apply with Claude tooltip on disabled
- **WHEN** the Apply-with-Claude button is disabled (no owning session, or owning session not active)
- **AND** the user hovers it
- **THEN** a Tooltip surfaces the specific gating reason (no matching branch / wrong active session)
- **AND** the button label includes the annotation count: `Apply with Claude (N)`

### Requirement: PR viewer results are scoped to the displayed PR

`PrViewer` SHALL apply diff and checks fetch results only if they were requested for the currently displayed `workspaceId`/`prNumber` pair; results for a previously displayed PR MUST be discarded (the shared diff cache MAY still be updated — it is keyed per PR).

#### Scenario: slow diff resolves after switching PRs

- **GIVEN** PR #12's diff fetch is in flight
- **WHEN** the user selects PR #15 in the picker before it resolves
- **THEN** #12's lines/hunks are not rendered under #15's header; #15 shows its own diff (or its loading state)

#### Scenario: stale checks are discarded

- **WHEN** a `get_pr_checks` result resolves for a PR that is no longer displayed
- **THEN** the checks state of the displayed PR is left untouched

### Requirement: Six-chip strip navigation in the GitPanel
The GitPanel SHALL render a 6-chip strip immediately below the branch header: `Files`, `History`, `Stashes`, `PRs`, `Conflicts`, `Branches`. Each chip owns a dedicated full-width body. The active chip SHALL be kept per session.

#### Scenario: Default chip is Files
- **WHEN** the GitPanel mounts for a session with no stored chip
- **THEN** the active chip is `Files`

#### Scenario: Active chip persists per session
- **WHEN** the user activates a non-default chip in session A, switches to session B, and returns to A
- **THEN** session A's chip restores to whatever was last active there
- **AND** session B shows its own chip, not A's

#### Scenario: Chip cycle via Shift+Arrow
- **WHEN** the user presses `Shift+ArrowRight`
- **THEN** the next chip in `[files, history, stashes, prs, conflicts, branches]` becomes active, wrapping at the end
- **AND** `Shift+ArrowLeft` cycles in the opposite direction
- **AND** the listener no-ops when an input/textarea/contenteditable holds focus

### Requirement: Commit file diffs show the commit's change
Opening a file of a commit from the History chip or the Branches chip SHALL show the diff of that file in that commit. That is the commit against its first parent, or against the empty tree for a root commit. It SHALL NOT show the working-tree diff. The Zen file list SHALL be the files of that commit only. The backend SHALL accept only a commit id of 4 to 64 hexadecimal characters that resolves to a commit. It SHALL accept only a file path inside the session worktree or workspace repository it reads from, matched literally.

#### Scenario: History chip shows the commit's diff
- **GIVEN** a session where commit `abc1234` added the line `x` to `src/a.ts`, and the working tree has a later uncommitted change `y` in `src/a.ts`
- **WHEN** the user expands `abc1234` in the History chip and opens `src/a.ts`
- **THEN** the diff SHALL contain the added line `x` and SHALL NOT contain `y`

#### Scenario: Zen file list is the commit's files
- **GIVEN** expanded commits `abc1234` (changes `a.ts`) and `def5678` (changes `b.ts`) in the History chip
- **WHEN** the user opens `a.ts` from `abc1234`
- **THEN** the Zen file list SHALL contain only `a.ts`

#### Scenario: Root commit
- **GIVEN** the repository's first commit adds `README.md`
- **WHEN** the user opens `README.md` from that commit
- **THEN** the diff SHALL show every line of `README.md` as added

#### Scenario: Invalid commit id or path is refused
- **WHEN** the command receives the commit id `--output=/tmp/x` or the path `../outside.txt`
- **THEN** it SHALL return an error without running `git`

#### Scenario: Pathspec magic is not interpreted
- **GIVEN** a commit that changes `a.ts` and `b.ts`
- **WHEN** the command receives the path `:(glob)*.ts`
- **THEN** the diff SHALL be empty

#### Scenario: Branch name that looks like a hash is not used
- **GIVEN** a branch named `beef` and no commit whose id starts with `beef`
- **WHEN** the command receives the commit id `beef`
- **THEN** it SHALL return an error

### Requirement: Git chip keys act only while the git panel has focus
Outside Zen mode, the keyboard listeners of the chip strip and of the Files, History, Stashes, PRs and Branches chips SHALL act only while the focus zone is `panel`. With the focus zone `sidebar`, `terminal` or `quake`, those keys SHALL leave the chips unchanged. Inside Zen mode the existing rule stays: the Zen sidebar zone owns them.

#### Scenario: Sidebar navigation does not move a chip cursor
- **GIVEN** the git panel shows the Branches chip with the cursor on its first row, and the focus zone is `sidebar`
- **WHEN** the user presses `ArrowDown` twice
- **THEN** the Branches chip cursor SHALL stay on its first row

#### Scenario: Chip cycle needs the panel zone
- **GIVEN** the git panel shows the Files chip and the focus zone is `sidebar`
- **WHEN** the user presses `Shift+ArrowRight`
- **THEN** the active chip SHALL stay `Files`

#### Scenario: Panel zone keeps the keys
- **GIVEN** the git panel shows the Branches chip and the focus zone is `panel`
- **WHEN** the user presses `ArrowDown`
- **THEN** the cursor SHALL move to the next row

### Requirement: Git panel reads run off the main thread
The commands the git panel calls when it opens and while it polls SHALL run off the application's main thread: `get_session_git_info`, `get_pr_status`, `has_pending_merge`, `git_stash_list`, `get_conflicted_files`, `list_prs`, `poll_pr_checks`, `get_git_status` and `get_recent_commits`. A slow `git` or `gh` call SHALL NOT block the window from drawing or from handling input.

#### Scenario: Slow gh does not freeze the window
- **GIVEN** `gh pr view` takes 1 second for the active session's branch
- **WHEN** the user opens the git panel
- **THEN** the panel header and chip strip SHALL be drawn before `gh pr view` returns
- **AND** the PR badge SHALL fill in when it returns

### Requirement: History chip keyboard reaches commit files
The History chip SHALL keep one cursor over its commit rows and the file rows of the expanded commit, in display order. `ArrowDown`/`j` and `ArrowUp`/`k` SHALL move the cursor one row, wrapping at the ends. `Space` and `Enter` on a commit row SHALL expand or collapse it. `Enter` on a file row SHALL open that file as "History chip shows the commit's diff" describes. Collapsing the commit that holds the cursor SHALL move the cursor to that commit. A list refresh SHALL keep the cursor on the same row when that row still exists.

#### Scenario: Open a commit's file with the keyboard
- **GIVEN** the History chip lists `abc1234` then `def5678`, and `abc1234` changes `src/a.ts` and `src/b.ts`
- **WHEN** the cursor is on `abc1234` and the user presses `Space`, `j`, `j`, `Enter`
- **THEN** Zen mode SHALL show the diff of `src/b.ts` in `abc1234`

#### Scenario: Space on a file row does nothing
- **GIVEN** the cursor is on the file row `src/a.ts` of `abc1234`
- **WHEN** the user presses `Space`
- **THEN** `abc1234` SHALL stay expanded and Zen mode SHALL stay closed

### Requirement: Push and ship run off the main thread
The commands of the commit, push and ship flow SHALL run off the application's main thread: `git_commit`, `git_push`, `git_ship` and `get_pr_preview_data`. A slow push, commit hook or `gh` call SHALL NOT block the window from drawing or from handling input.

#### Scenario: Slow push does not freeze the window
- **GIVEN** `git push` for the session's branch takes 3 seconds
- **WHEN** the user pushes from the git panel
- **THEN** the window SHALL keep drawing and handling input until the push returns
