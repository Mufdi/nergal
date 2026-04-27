## ADDED Requirements

### Requirement: Command palette exposes git actions
The command palette SHALL expose first-class entries for the four git actions: "Ship session", "Push", "Open conflicts", "Merge locally…". Each entry SHALL display its keyboard shortcut alongside the label and SHALL trigger the same effect as the dedicated shortcut.

#### Scenario: Command palette lists Ship session
- **WHEN** the user opens the command palette and types "ship"
- **THEN** the entry "Ship session" appears with its shortcut chip; activating it opens the Ship dialog for the active session

#### Scenario: Command palette lists Open conflicts
- **WHEN** the user opens the command palette and types "conflict"
- **THEN** the entry "Open conflicts" appears; activating it switches to the Conflicts tab for the active session (or shows an empty-state if no conflicts)

#### Scenario: Command palette lists Merge locally
- **WHEN** the user opens the command palette and types "merge"
- **THEN** the entry "Merge locally…" appears with `Ctrl+Shift+M` chip; activating it opens the MergeModal

### Requirement: Every git action SHALL have a discoverable shortcut
Every interactive git action in the UI (Commit, Push, Ship, Merge, Open conflicts, Resolve in conflicts panel, Reset, Save, Ours/Theirs/Both, Send-to-Claude, Complete-merge) SHALL have a registered keyboard shortcut in `shortcuts.ts`. The shortcut SHALL be surfaced to the user via at least one of: a visible `<kbd>` chip on the action button, a command palette entry, or both. Reliance on `title` attributes alone is NOT acceptable.

#### Scenario: shortcuts.ts coverage includes all git actions
- **WHEN** the registered shortcuts list is inspected at startup
- **THEN** every git action listed above has an entry with `id`, `keyCombo`, and `description` populated

#### Scenario: A new git action without a shortcut is rejected
- **WHEN** a developer adds a new git action button in the UI without registering a shortcut
- **THEN** a lint or test SHALL fail (or, at minimum, the button SHALL log a console warning identifying the missing shortcut at mount)

## MODIFIED Requirements

### Requirement: Contextual dispatch for Ctrl+Shift+R
The shortcut `Ctrl+Shift+R` SHALL dispatch contextually based on the active focus zone and tab type: when focus is in the plan/spec panel and a pending plan review exists, it fires `revise-plan`; when focus is in the GitPanel and conflicts exist or a Conflicts tab is active, it fires `resolve-conflict` (open active conflict tab or send the pre-filled Ask-Claude prompt for auto-merge conflicts as defined in `conflict-resolution`).

#### Scenario: Ctrl+Shift+R in plan panel context
- **WHEN** the focus zone is the plan or spec panel and a pending plan review state exists
- **THEN** the shortcut fires the revise-plan handler (existing behavior preserved)

#### Scenario: Ctrl+Shift+R in conflicts context fires resolve-conflict
- **WHEN** the focus zone is the GitPanel or a Conflicts tab is active with a pre-filled Ask-Claude prompt
- **THEN** the shortcut sends the pre-filled prompt to Claude as defined in `conflict-resolution`'s closed-loop scenario, OR opens the first conflicted file when no prompt is pre-filled
