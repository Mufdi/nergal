## MODIFIED Requirements

### Requirement: Keybinding display
Each action row SHALL display its effective binding as key badges. Chord bindings (`"leader <key>"`) SHALL render as the leader combo followed by the continuation (e.g. `Ctrl+Space` `N`). Entries with `keys: ""` (palette-only actions) SHALL render with no badge and remain selectable/executable.

#### Scenario: Chord badge
- **WHEN** the palette lists "New Session" bound to `leader n`
- **THEN** the row shows `Ctrl+Space` `N` as badges

#### Scenario: Unbound entry
- **WHEN** the palette lists a palette-only action (e.g. "Provider status")
- **THEN** the row shows no key badges and Enter executes it

## REMOVED Requirements

### Requirement: Backward compatibility
**Reason**: this freeze ("the static `shortcutRegistryAtom` literal SHALL not be reordered, renamed, or have entries removed") was scoped to the Obsidian-templates change that introduced dynamic palette sources — it guaranteed *that change* didn't disturb the registry. As a permanent requirement it forbids any keymap evolution, and the shortcuts restructure removes/renames entries by design (`focused-session-*`, remaps). The durable part of the intent (palette and dispatcher consume one override-resolved registry) lives in the keyboard-shortcuts capability's *Shortcut registry* requirement.
