## ADDED Requirements

### Requirement: Command palette opens with Ctrl+K
The system SHALL display a command palette overlay when the user presses Ctrl+K from any zone except terminal. The overlay SHALL contain a text input for searching and a list of available actions.

#### Scenario: Open palette
- **WHEN** user presses Ctrl+K (outside terminal)
- **THEN** a centered overlay appears with a search input focused and all actions listed

#### Scenario: Palette not captured in terminal
- **WHEN** focus is in terminal and user presses Ctrl+K
- **THEN** the keystroke passes to the PTY (Ctrl+K = kill line in readline)

### Requirement: Action list with fuzzy filtering
The palette SHALL list all actions from the shortcut registry. Typing in the search input SHALL filter actions by substring matching on action label and keywords. Matching SHALL be case-insensitive.

#### Scenario: Filter by substring
- **WHEN** palette is open and user types "git"
- **THEN** only actions containing "git" in their label or keywords are shown (e.g., "Open Git Panel", "Commit", "Merge")

#### Scenario: Empty filter shows all
- **WHEN** palette is open and search input is empty
- **THEN** all available actions are listed

### Requirement: Actions grouped by category
Actions in the palette SHALL be grouped under category headers: Navigation, Session, Panel, Action. Groups with no matching actions after filtering SHALL be hidden.

#### Scenario: Grouped display
- **WHEN** palette is open with no filter
- **THEN** actions appear under "Navigation", "Session", "Panel", "Action" category headers

### Requirement: Keybinding display
Each action in the palette SHALL display its keyboard shortcut on the right side of the row, rendered as key badge elements (e.g., `Ctrl` `Shift` `P`).

#### Scenario: Shortcut visible
- **WHEN** palette shows "Open Plan Panel"
- **THEN** the row displays `Ctrl` `Shift` `P` as key badges on the right

### Requirement: Keyboard navigation in palette
The user SHALL navigate the action list with Up/Down arrow keys. The highlighted action SHALL execute on Enter. Esc or clicking outside SHALL dismiss the palette.

#### Scenario: Arrow key navigation
- **WHEN** palette is open and user presses Down arrow twice then Enter
- **THEN** the third action in the filtered list executes and palette closes

#### Scenario: Escape dismisses
- **WHEN** palette is open and user presses Escape
- **THEN** the palette closes without executing any action

#### Scenario: Click outside dismisses
- **WHEN** palette is open and user clicks on the backdrop
- **THEN** the palette closes without executing any action

### Requirement: Palette executes action and closes
When an action is selected (Enter or click), the palette SHALL close immediately and execute the action's handler function.

#### Scenario: Execute action
- **WHEN** user selects "Open Plan Panel" from the palette
- **THEN** palette closes and the Plan tab opens/focuses in the right panel
