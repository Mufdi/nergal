## ADDED Requirements

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
