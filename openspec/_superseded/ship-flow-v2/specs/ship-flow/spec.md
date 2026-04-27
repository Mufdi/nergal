## ADDED Requirements

### Requirement: Ship dialog uses 2-step progressive disclosure
The Ship dialog SHALL render in two explicit steps with a stepper indicator showing current position. Step 1 is the Stage picker (file selection, full pane). Step 2 is Commit + PR (title, body, target branch picker, auto-merge toggle, Ship button). A "Next" action advances Step 1 → Step 2; a "Back" action returns Step 2 → Step 1 without losing field state.

#### Scenario: Dialog opens on Step 1 with all unstaged + untracked files pre-selected
- **WHEN** the user opens the Ship dialog
- **THEN** the dialog renders Step 1 with the file picker showing unstaged and untracked files (excluding tooling paths), all pre-selected, cursor on the first row, and a stepper showing `● ○`

#### Scenario: Enter on Step 1 advances to Step 2
- **WHEN** the user is on Step 1 and presses Enter (and no input field is focused)
- **THEN** the dialog stages the selected files in the background and renders Step 2 with the commit/PR form, stepper showing `● ●`

#### Scenario: Step 2 preserves Step 1 selections on Back
- **WHEN** the user navigates from Step 2 back to Step 1 via the Back action
- **THEN** Step 1 displays the previously made selections and cursor position; no staging is undone

#### Scenario: Ctrl+Enter on Step 1 skips directly to Ship when defaults are acceptable
- **WHEN** the user is on Step 1 with all files default-selected and presses Ctrl+Enter
- **THEN** the dialog stages files, generates the default commit/PR title and body from the preview data, and immediately invokes Ship without rendering Step 2

### Requirement: PR target branch picker on Step 2
Step 2 SHALL include a compact PR target branch picker (dropdown) sourced from `list_branches`. The default selection SHALL be `main` (or the workspace's configured base branch when set). The list SHALL filter out `cluihud/*` worktree branches.

#### Scenario: Picker defaults to main and lists remote branches
- **WHEN** Step 2 renders and `list_branches` returns `[main, develop, release/v2, cluihud/abc123]`
- **THEN** the picker shows `main` selected by default and lists `main, develop, release/v2` (the `cluihud/*` branch is hidden)

#### Scenario: Selecting a non-default target updates the PR base before Ship
- **WHEN** the user selects `develop` in the picker and proceeds to Ship
- **THEN** the PR is created with `develop` as the base branch instead of `main`

#### Scenario: Picker disabled when no remote branches available
- **WHEN** `list_branches` returns an empty list (no remotes configured)
- **THEN** the picker is disabled with placeholder text "No remote branches" and Ship falls back to the worktree's tracked base

### Requirement: Auto-merge default persists in config
The system SHALL persist the auto-merge toggle preference as `git.auto_merge_default: bool` in the application config. The Ship dialog SHALL initialize the toggle from this config value, and changes to the toggle SHALL write back to config (auto-save) without requiring an explicit Save action.

#### Scenario: First-run default is true (auto-merge enabled)
- **WHEN** the user opens the Ship dialog for the first time after upgrade and config has no `git.auto_merge_default` value set
- **THEN** the toggle initializes to `true` (checked) and the config writes `git.auto_merge_default: true`

#### Scenario: Toggle change persists across sessions
- **WHEN** the user toggles auto-merge off during one Ship and closes the dialog
- **THEN** the next Ship dialog opens with the toggle off, sourced from the saved config

### Requirement: Ship triggers session cleanup on confirmed PR-merged state
After Ship creates a PR with auto-merge enabled, the system SHALL poll PR status. When the poll observes the PR transition to `MERGED` on remote, it SHALL invoke the session-cleanup capability (total deletion) without requiring explicit user action.

#### Scenario: Auto-merge succeeds and remote-merged poll triggers cleanup
- **WHEN** Ship was invoked with auto-merge enabled, the PR is created, and a subsequent poll cycle observes `state: MERGED`
- **THEN** the system invokes total session cleanup as defined in `session-cleanup`

#### Scenario: User-disabled auto-merge does not auto-cleanup
- **WHEN** Ship was invoked with auto-merge disabled
- **THEN** the system does not auto-cleanup on PR merged; the user must explicitly trigger cleanup from the GitPanel

## MODIFIED Requirements

### Requirement: PR preview dialog before create
Before creating a pull request, the system SHALL render the Ship dialog as defined in "Ship dialog uses 2-step progressive disclosure" (Step 2 holds title/body/target/auto-merge). Title, body, and target branch SHALL be editable. The user SHALL confirm via Enter on the Ship button or `Ctrl+Enter` from any field, or cancel via Escape from any step.

#### Scenario: Dialog prefills from last commit and range diffstat
- **WHEN** Step 2 renders for a session with commits in `base..HEAD`
- **THEN** title is set to the subject of the most recent commit, body is populated with the commit subjects list + a diffstat summary, target branch is set to the workspace's base, and the commits list preview shows each commit's short hash and subject

#### Scenario: Dialog loads project template
- **WHEN** `.cluihud/pr-template.md` exists in the workspace repo root
- **THEN** the template content is appended to the body prefill after the commits list and diffstat

#### Scenario: Enter confirms from Step 2, Escape cancels from any step
- **WHEN** Step 2 is rendered with a non-empty title and the user presses Enter on the Ship button or Ctrl+Enter from any field
- **THEN** Ship proceeds with the current title, body, target branch, and auto-merge value
- **WHEN** the user presses Escape at any time
- **THEN** the dialog closes and no action is taken

#### Scenario: Inline warning when gh CLI not authenticated
- **WHEN** the dialog mounts and `gh_available` returns false (not installed or not authenticated)
- **THEN** Step 2 shows an inline warning and disables the Ship button, with a link to `gh auth login` instructions
