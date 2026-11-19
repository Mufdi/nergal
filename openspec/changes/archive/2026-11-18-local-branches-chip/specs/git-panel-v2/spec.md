## REMOVED Requirements

### Requirement: Chip-strip navigation in the GitPanel
**Reason**: The strip gains a sixth chip (`Branches`), and the old "persists per workspace" scenario did not match the code, which keeps the active chip per session (`gitChipModeAtom` in `src/stores/git.ts`).
**Migration**: Replaced by "Six-chip strip navigation in the GitPanel". No user data changes.

## ADDED Requirements

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
