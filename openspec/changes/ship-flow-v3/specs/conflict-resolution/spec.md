## ADDED Requirements

### Requirement: Conflicts render as a right-panel tab
Conflict resolution SHALL be presented as a tab type `conflicts` in the right panel. The tab is opened by:

1. The shortcut `Ctrl+Alt+Q`.
2. An automatic open by the PR Viewer when a "Merge into `<base>`" attempt fails with a `mergeable=false` reason.
3. A click on a conflicted file row in the GitPanel inline conflicts list.

The standalone ConflictsPanel surface SHALL be removed; its internal mechanics (3-pane Ours/Theirs/Merged + region nav + Claude handoff via `Ctrl+Shift+R`) are preserved inside the tab body.

#### Scenario: Ctrl+Alt+Q opens or focuses the conflicts tab
- **WHEN** the user presses Ctrl+Alt+Q
- **THEN** if a `conflicts` tab exists for the active session, it focuses; otherwise a new `conflicts` tab opens

#### Scenario: Failed PR merge opens the conflicts tab automatically
- **WHEN** the user clicks "Merge into main" in the PR Viewer and the merge fails because of a conflict
- **THEN** a `conflicts` tab opens, the PR Viewer footer shows an inline link "Resolve conflicts in tab", and the conflicted files list is loaded

#### Scenario: Conflict tab closes when no conflicts remain (after explicit completion)
- **WHEN** the user clicks "Finish in-progress merge" inside the conflicts tab AND that action succeeds (creating the merge commit)
- **THEN** the `conflicts` tab auto-closes after a short toast confirming "Merge commit created"

#### Scenario: Tab does NOT auto-close just because resolution save emptied the conflicts list
- **WHEN** the user resolves the last conflicted file via Save (so `conflicted_files === 0` momentarily) but has not yet clicked "Finish in-progress merge"
- **THEN** the tab stays open with the empty-conflicts view + Finish-merge button visible — closing here would lose the user's path back to completing the merge

### Requirement: Conflict tab toolbar is consistent with prior Conflicts surface
The `conflicts` tab body SHALL preserve the toolbar from the prior ConflictsPanel: region count + currently-focused region, Ours/Theirs/Both buttons, Reset, Save, and the Ask-Claude prompt textarea + Send button. Keyboard shortcuts (`o`/`t`/`b`, `↑/↓` or `j/k`, `Space` to collapse, `Ctrl+Shift+Z` to reset, `Ctrl+Shift+Enter` to save) are preserved.

#### Scenario: All shortcuts work inside the tab body
- **WHEN** the `conflicts` tab is focused and the user presses `o`
- **THEN** the focused region's "Ours" choice is applied (same behavior as the standalone panel had)

#### Scenario: Ask-Claude prompt sends to the active session terminal
- **WHEN** the user has an Ask-Claude prompt typed and presses `Ctrl+Shift+R` (or clicks Send)
- **THEN** the prompt is written to the active session's terminal (same behavior as before)
