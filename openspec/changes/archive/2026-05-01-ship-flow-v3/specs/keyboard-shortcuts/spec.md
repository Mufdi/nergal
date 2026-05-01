## ADDED Requirements

### Requirement: Modal-scoped action shortcuts (Ctrl+1/2/3 in Ship modal)
While the Ship modal is open, the modal's keydown handler SHALL capture `Ctrl+1`, `Ctrl+2`, and `Ctrl+3` in the capture phase and dispatch the corresponding action (Commit, Commit + Push, Commit + Push + PR). The global session-switching shortcuts that bind these key combos SHALL be bypassed while the modal is open.

#### Scenario: Ctrl+1 in modal fires Commit, not session switch
- **WHEN** the Ship modal is open and the user presses Ctrl+1
- **THEN** the Commit action fires; the global "switch to session 1" shortcut does NOT fire

#### Scenario: Ctrl+1 outside modal still switches sessions
- **WHEN** no modal is open and the user presses Ctrl+1
- **THEN** the global session-switching behavior fires as usual

### Requirement: PR Viewer Merge shortcut (Ctrl+Enter when PR Viewer focused)
When a `pr` tab is the active focus zone, `Ctrl+Enter` SHALL fire the "Merge into `<base>`" action defined in `pr-viewer`.

#### Scenario: Ctrl+Enter in focused PR Viewer triggers merge
- **WHEN** a `pr` tab is the active tab and the user presses Ctrl+Enter
- **THEN** the merge handler runs (with the same gates as a button click — disabled if not OPEN, etc.)

## REMOVED Requirements

### Requirement: Every git action SHALL have a discoverable shortcut
**Reason**: Re-scoped in v3. Some actions (the local Merge button, the auto-merge checkbox) are gone. The remaining set still has shortcuts, but the universal mandate caused noise (e.g., Kbd chips on tertiary buttons). v3 keeps `<Kbd>` chips on Commit (`Ctrl+1`), Commit+Push (`Ctrl+2`), Commit+Push+PR (`Ctrl+3`), Merge (`Ctrl+Enter` in PR Viewer), Push-only (`Ctrl+Alt+P` global), Open conflicts (`Ctrl+Alt+Q`), Complete merge (`Ctrl+Alt+Enter`).
**Migration**: Audit existing components: keep Kbd chips on the canonical action surfaces listed above; remove from any tertiary affordances added in v2.

### Requirement: Command palette exposes git actions
**Reason**: Stays effectively the same — the command palette already iterates `shortcutRegistryAtom`, so all registered shortcuts auto-appear. v3 just changes which shortcuts are registered. No spec-level change needed; this requirement collapses into the broader registry behavior.
**Migration**: No code change; this requirement is folded into the existing palette behavior.
