---
status: archived
implemented: 2026-03-25
archived: 2026-04-04
files:
  - src/components/command/CommandPalette.tsx
  - src/stores/shortcuts.ts
---

## Purpose

Provide a Ctrl+K command palette overlay with fuzzy-filtered, categorized actions and keyboard navigation.

## Implementation Notes

All requirements implemented. Known deviations:
- **Filtering**: Substring matching only (`.includes()`), not fuzzy matching. Sufficient for current action count.
- **Terminal zone**: No explicit check to prevent opening in terminal zone.
## Requirements
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

### Requirement: Dynamic action sources
The command palette SHALL accept dynamic action sources alongside the static registry literal. A "dynamic source" is a Jotai atom whose value is `ShortcutAction[]`. The resolved registry feeding the palette SHALL be the concatenation of (static literal) ⊕ (each dynamic source's current value).

The implementation MUST: (a) keep `shortcutRegistryAtom` as the public surface used by `CommandPalette.tsx` and existing keyboard dispatch, (b) internally split into `staticShortcutRegistryAtom` (the existing static literal) and a list of dynamic sources, (c) expose `registerDynamicShortcutSource(atom)` for feature modules to plug in additional sources at module load time.

#### Scenario: Static plus dynamic merge

- **WHEN** the templates feature registers `obsidianTemplatesShortcutsAtom` as a dynamic source
- **AND** that atom currently emits 3 template entries
- **THEN** `shortcutRegistryAtom` SHALL contain the static base list plus those 3 entries
- **AND** the entries SHALL appear in the palette under their declared category

#### Scenario: Dynamic source updates trigger palette refresh

- **WHEN** a template file is added externally and the templates atom updates
- **THEN** the next render of the command palette SHALL show the new entry
- **AND** previously-displayed entries SHALL remain in place

### Requirement: New `templates` category
The palette SHALL render a new category labeled `templates` in addition to the existing `navigation`, `session`, `panel`, `action`, and `contextual` categories. The new category SHALL appear after `action` in the rendering order.

The category SHALL be hidden entirely (not rendered) when no templates are present, to avoid empty headers.

#### Scenario: Templates section appears when populated

- **WHEN** the palette is opened
- **AND** the templates dynamic source emits 2 entries
- **THEN** a category header "TEMPLATES" SHALL render
- **AND** the 2 entries SHALL appear underneath it

#### Scenario: Templates section hidden when empty

- **WHEN** the palette is opened
- **AND** no templates are registered (e.g. `templates_path` not configured)
- **THEN** no "TEMPLATES" header SHALL render in the palette

### Requirement: Backward compatibility
Existing palette behavior MUST remain unchanged for users who never configure Obsidian. The static `shortcutRegistryAtom` literal SHALL not be reordered, renamed, or have entries removed. The dynamic-source plumbing is additive only.

#### Scenario: User without templates sees no change

- **WHEN** the user has no `templates_path` configured
- **AND** opens the command palette
- **THEN** the palette SHALL render the same categories and entries as before this change
- **AND** the `templates` category SHALL not be rendered

