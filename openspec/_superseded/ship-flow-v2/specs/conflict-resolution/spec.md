## ADDED Requirements

### Requirement: Auto-merge conflict closed loop in same session
When auto-merge is enabled on a Ship and the PR-status poll detects the PR is blocked by conflict, the system SHALL surface the conflict in the same session that owns the PR (no new session creation). It SHALL render an inline alert in the GitPanel, open the Conflicts panel/tab in that session, and pre-fill an Ask-Claude prompt template that includes (a) PR identifier, (b) list of conflicted files, (c) summarized diff per conflict region. The pre-filled prompt SHALL NOT be sent to Claude automatically — the user MUST confirm via the dedicated send shortcut.

#### Scenario: Conflict detected on auto-merged PR opens Conflicts panel in same session
- **WHEN** poll cycle observes `pr.mergeable === false` with a conflict reason for a PR opened by Ship with auto-merge enabled
- **THEN** the GitPanel for that session shows an inline alert "Auto-merge blocked by conflict — review and resolve", the Conflicts panel/tab is opened in the same session, and the conflicted files list is loaded

#### Scenario: Ask-Claude prompt is pre-filled but not sent
- **WHEN** the Conflicts panel renders due to an auto-merge conflict
- **THEN** the Ask-Claude prompt input is pre-filled with a template containing `PR #<n>`, the list of conflicted files, and a summarized diff for each region; the prompt is NOT sent to Claude until the user activates the dedicated send shortcut

#### Scenario: User confirms shortcut to send the pre-filled prompt
- **WHEN** the user presses the Ask-Claude send shortcut while the Conflicts panel is focused with a pre-filled prompt
- **THEN** the prompt is sent to Claude in the same session, the prompt input is cleared, and a toast confirms "Sent to Claude"

#### Scenario: Session was closed before conflict detected
- **WHEN** the auto-merge-conflict poll fires for a session whose state has already been deleted (cleanup ran for unrelated reason)
- **THEN** a desktop notification appears with text "Conflict on PR #<n> — reopen workspace to resolve" and the poll deactivates for that PR

## ADDED Requirements

### Requirement: Conflict region header navigation and toggle
Within the Conflicts panel, the user SHALL navigate between conflict regions with `↑/↓` (or `j/k`), accept resolutions with `o/t/b` (Ours/Theirs/Both), and toggle the focused region's collapse state with `Space`. The Space binding SHALL fire exactly once per press: focus residue from clicking a region row or chevron button SHALL NOT cause the browser's default button activation to combine with the global keydown listener (which would result in double-toggle behavior).

#### Scenario: Click on region row leaves global listener owning Space
- **WHEN** the user clicks a region row to focus it, then presses Space without any further click
- **THEN** the focused region toggles its collapse state exactly once (no flicker, no double-fire)

#### Scenario: Click on chevron button leaves global listener owning Space
- **WHEN** the user clicks the chevron button on a region header to toggle collapse, then presses Space
- **THEN** the region toggles collapse exactly once on each Space press

#### Scenario: Region buttons retain native semantics
- **WHEN** the user Tab-navigates to a region button and presses Enter
- **THEN** Enter activates the button as expected (native button semantics preserved)
