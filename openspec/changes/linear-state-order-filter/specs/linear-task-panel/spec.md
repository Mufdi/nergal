## ADDED Requirements

### Requirement: State visibility dropdown

The panel header SHALL offer a multi-select dropdown that shows or hides issues by workflow state name. The dropdown SHALL list the state names present in the current pool (team scope, assigned-to-me and show-completed applied), deduplicated case-insensitively and ordered by the name-first state rank. Hiding a state SHALL remove its issues from the list and from sub-issue trees; the filter SHALL be computed locally over the mirror without a Linear call. The selection SHALL be keyed by state name, so one choice applies to every team that has a state with that name. The selection SHALL persist in the shared in-memory store and SHALL survive closing and reopening the panel within one app run. The dropdown SHALL follow the keyboard mechanics of the label filter popover: Arrow Down on the trigger opens it and focuses the first option, Arrow Up and Arrow Down move between options, Space and Enter toggle the focused option without closing, and Escape closes it and returns focus to the trigger.

#### Scenario: Hiding a state removes its issues
- **WHEN** the user hides "Done" in the dropdown
- **THEN** the panel SHALL NOT list issues whose state name is "Done"
- **AND** issues in other states SHALL stay listed

#### Scenario: Options follow the state rank
- **WHEN** the pool holds issues in Backlog, In Review and Canceled
- **THEN** the dropdown SHALL list In Review, Backlog, Canceled in that order

#### Scenario: Selection survives reopening the panel
- **WHEN** the user hides "Backlog", closes the panel and opens it again
- **THEN** "Backlog" SHALL still be hidden

#### Scenario: Show all restores every state
- **WHEN** the user picks "Show all" in the dropdown
- **THEN** no state SHALL be hidden
