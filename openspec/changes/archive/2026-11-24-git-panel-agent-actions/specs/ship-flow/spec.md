## MODIFIED Requirements

### Requirement: Apply-with-Claude tooltip explains the disabled state
The Apply-with-Agent button SHALL surface its disabled-state reason via a Tooltip component (not a native browser title). The button label SHALL include the annotation count inline (`Apply with Agent (N)`).

#### Scenario: Tooltip on no matching session
- **WHEN** no local session has a worktree branch matching the PR's `head_ref_name`
- **THEN** the button is disabled and the tooltip reads "No local session matches this PR's branch (`<headRefName>`). Open a session whose worktree is on this branch to apply."

#### Scenario: Tooltip on wrong active session
- **WHEN** an owning session exists but is not the active session
- **THEN** the tooltip reads "Switch to the session on branch `<headRefName>` — its terminal will receive the prompt."

#### Scenario: Tooltip on enabled state
- **WHEN** the owning session is active
- **THEN** the tooltip reads "Send N annotation(s) as a structured prompt to the session terminal."
