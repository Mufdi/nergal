# conflict-resolution

The resolution UI is decomposed into a hook + pane composition, behavior-preserving.

## ADDED Requirements

### Requirement: ConflictView is a composition over extracted logic

The conflict resolution view SHALL separate its stateful resolution logic (region navigation, accept/reject application, keyboard handling) into a `useConflictResolution` hook, with the component composing the existing `CodePane` and `ConnectorStrip` pieces. All externally observable behavior — shortcuts, region choice semantics, scroll-sync connectors, auto-resolve flow — SHALL be unchanged by the decomposition.

#### Scenario: identical resolution walk

- **WHEN** a user resolves a multi-region conflicted file (navigate regions, accept ours/theirs/both, save)
- **THEN** every interaction behaves exactly as before the refactor

#### Scenario: logic is testable without CodeMirror

- **WHEN** unit tests exercise region navigation and choice application through the hook's returned API (views absent/null)
- **THEN** the logic is testable headlessly (no EditorView mount required) for its non-editor state transitions
