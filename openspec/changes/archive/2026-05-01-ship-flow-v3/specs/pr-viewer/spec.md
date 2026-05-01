## ADDED Requirements

### Requirement: PR Viewer tab type
The right panel SHALL support a tab type `pr` that renders a single pull request. Each `pr` tab is keyed by `(workspaceId, prNumber)` and is opened by clicking a PR row in the GitPanel PRs sidebar or by a deep-link from elsewhere in the app.

#### Scenario: Click on PR row opens or focuses a pr tab
- **WHEN** the user clicks on a PR entry in the GitPanel PRs sidebar
- **THEN** if a `pr` tab for that PR already exists, it becomes the active tab; otherwise a new `pr` tab opens and becomes active

#### Scenario: pr tab persists across panel reloads within session
- **WHEN** the user closes and re-opens the right panel without quitting cluihud
- **THEN** previously-open `pr` tabs are restored in the same order

### Requirement: PR Viewer header surfaces PR metadata and CI
The PR Viewer SHALL render a header containing PR number (`#N`), title, base branch, state badge (OPEN/CLOSED/MERGED), CI status indicator (passing/failing/pending counts), and a link icon that opens the PR on GitHub in the system browser.

#### Scenario: Header reflects current PR state
- **WHEN** a PR Viewer renders for an OPEN PR with 2 passing checks and 0 failing
- **THEN** the header shows `#N`, title, base, an OPEN badge, a green check pill `2`, and an external-link icon

#### Scenario: CI status updates while viewer is open
- **WHEN** the PR Viewer is open and a poll cycle observes a check transition from pending to passing
- **THEN** the header CI indicator updates without requiring a tab close/reopen

### Requirement: PR diff data source
The PR Viewer body SHALL fetch the PR's diff via a backend Tauri command `get_pr_diff(workspace_id: String, pr_number: u32) -> Result<String, String>` that invokes `gh pr diff <pr_number>` in the workspace's repo and returns the unified diff text. The frontend SHALL parse the unified diff into chunks for rendering and navigation. Reusing local-file `DiffView` is NOT sufficient because it sources from the working tree, not from the PR.

#### Scenario: Backend fetches diff via gh
- **WHEN** the PR Viewer mounts for `(workspaceId, prNumber)`
- **THEN** it invokes `get_pr_diff` and renders chunks parsed from the returned unified diff

#### Scenario: Diff fetch failure surfaces an inline error
- **WHEN** `gh pr diff` fails (no auth, network, PR not found)
- **THEN** the body shows an inline error "Could not load PR diff: `<message>`" with a Retry button

### Requirement: PR Viewer body shows annotated diff with chunk navigation
The PR Viewer body SHALL render the parsed diff chunks (per the data source requirement above). The user SHALL navigate between diff chunks with `↑`/`↓` (or `j`/`k`) and create an annotation on the focused chunk by pressing `a` (or clicking an annotate affordance on the chunk header).

#### Scenario: Arrow navigation moves the focused chunk
- **WHEN** the PR Viewer body is focused and the user presses `↓`
- **THEN** the focus indicator moves to the next chunk and the chunk scrolls into view

#### Scenario: Annotate the focused chunk
- **WHEN** the user presses `a` with a chunk focused
- **THEN** an annotation input appears anchored to that chunk; on submit, the annotation is persisted in the per-PR annotations map keyed by `(workspaceId, prNumber)`

#### Scenario: Existing annotations render on the chunks they target
- **WHEN** the PR Viewer renders for a PR with prior annotations
- **THEN** each annotation appears on or near its anchor chunk with its text and a remove affordance

#### Scenario: Annotations are session-scoped (not persisted across cluihud restarts)
- **WHEN** the user creates annotations on a PR, then closes and reopens cluihud
- **THEN** the annotations are NOT restored — they live only in the in-memory `prAnnotationsMapAtom` for this MVP. Persistence to disk (annotation-persistence capability) is deferred to a later change to keep v3 scope bounded.

### Requirement: Apply annotations with Claude
When the focused PR Viewer's owning session is the currently active session AND the per-PR annotations list is non-empty, the footer SHALL expose an "Apply annotations with Claude" action that packages the PR diff + the list of annotations (with chunk anchors and text) into a structured prompt and writes it to the session's terminal as a user message. The system SHALL NOT auto-mark annotations as applied — that is the user's job after reviewing Claude's edits.

#### Scenario: Button enabled only when active session owns the PR
- **WHEN** the PR Viewer is open for a PR whose worktree session is not the currently active session
- **THEN** the "Apply annotations with Claude" button is disabled with a tooltip "Switch to the session that owns this PR to apply annotations"

#### Scenario: Apply sends a prompt to the session terminal
- **WHEN** the user clicks "Apply annotations with Claude" with 3 annotations on the focused PR
- **THEN** the system writes a prompt to the session's PTY containing the PR diff + the 3 annotations with their chunk anchors; the prompt is plain text written verbatim, no command flags

#### Scenario: User removes a resolved annotation manually
- **WHEN** Claude has applied an annotation's intent and the user clicks the annotation's remove affordance
- **THEN** the annotation is removed from the per-PR map and the chunk's annotation marker disappears

### Requirement: PR Viewer Merge action
The PR Viewer footer SHALL render a single primary action labeled `Merge into <base>` (e.g., `Merge into main`) with shortcut `Ctrl+Enter`. The button SHALL be disabled when (a) PR state is not OPEN, (b) `gh` is not authenticated, or (c) the PR has unresolved annotations and the user has not explicitly chosen to merge anyway.

When the PR state is not OPEN (e.g., MERGED externally while the user is viewing), the Merge button SHALL be hidden entirely and a banner inside the PR Viewer body SHALL explain "PR was `<state>` outside cluihud. Use the Cleanup session banner in the GitPanel to close this session." This avoids double-affordance with the GitPanel cleanup banner.

#### Scenario: Click triggers gh pr merge --squash
- **WHEN** the user clicks `Merge into main` on an OPEN PR with no unresolved annotations
- **THEN** the system invokes `gh pr merge --squash` for the PR; on success, triggers session-cleanup as defined in `session-cleanup`

#### Scenario: Merge blocked by unresolved annotations requires override
- **WHEN** the user clicks `Merge into main` on a PR with 2 unresolved annotations
- **THEN** the system shows an inline confirmation "2 annotations not yet applied. Merge anyway?" with explicit `Merge anyway` and `Cancel` buttons; only the `Merge anyway` action proceeds with the merge

#### Scenario: Merge fails due to conflict opens the conflicts tab
- **WHEN** `gh pr merge` fails with a `mergeable=false` reason
- **THEN** the system opens the `conflicts` tab as defined in `conflict-resolution` and shows an inline link in the PR Viewer "Resolve conflicts in tab"
