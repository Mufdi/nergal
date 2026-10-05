Linear: FRI-491

## Why

The Linear panel orders state groups by Linear's native state type: triage first, started second. Active review work sits below triage intake, and the list fills with closed issues. The user needs the active work at the top and a way to hide states that do not help daily work.

## What Changes

- State order resolves by state **name** first, then by state type for unknown names.
- Open states come first: In Review, In Progress, Todo, Backlog, Triage, then other open states.
- Closed states come last: Done, Duplicate, Canceled. A closed state never sorts between open states.
- The legacy `Pending` state (type `completed`) ranks with In Review and counts as open. The project rules say Pending equals In Review.
- `Duplicate` and `Canceled` share the type `canceled` in Linear. The name tells them apart.
- The same order applies to issues inside project, assignee and cycle groups.
- The show-completed control hides closed states by the same name-first rule, so it no longer hides `Pending`.
- A new multi-select dropdown in the panel header shows or hides individual states by name.
- The hidden-state selection persists in the shared Jotai store, like the other Linear panel filters. It survives closing and reopening the panel, but not an app restart.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `linear-mirror`: the "Faithful Linear status glyphs and group order" requirement changes the group order to the name-first order. The "Completed, canceled, and duplicate issues visible by default" requirement uses the name-first closed classification.
- `linear-task-panel`: adds a requirement for the state-visibility dropdown.

## Impact

- `src/lib/linearStateOrder.ts` (new): pure rank, closed-state and filter helpers with vitest coverage.
- `src/components/linear/LinearPanel.tsx`: group sort, in-group sort, show-completed filter and the new dropdown.
- `src/stores/linear.ts`: new `linearHiddenStatesAtom`.
- No backend, schema, config or Linear API change.
