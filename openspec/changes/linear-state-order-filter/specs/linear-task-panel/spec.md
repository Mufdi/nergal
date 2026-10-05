## ADDED Requirements

### Requirement: State visibility dropdown

The panel header SHALL offer a multi-select dropdown that shows or hides issues by workflow state name. The dropdown SHALL list the state names present in the current pool (team scope, assigned-to-me and show-completed applied), deduplicated case-insensitively and ordered by the name-first state rank. Hiding a state SHALL remove its issues from the list and from sub-issue trees; the filter SHALL be computed locally over the mirror without a Linear call. The selection SHALL be keyed by state name, so one choice applies to every team that has a state with that name. The selection SHALL persist in the shared store and in local storage, so it survives closing the panel and restarting the app. Each option SHALL render as a full-width row that matches a State-tab group header: the state glyph followed by the state name, with no trailing check mark. A hidden state SHALL render dimmed. The dropdown SHALL follow the keyboard mechanics of the label filter popover: Arrow Down on the trigger opens it and focuses the first option, Arrow Up and Arrow Down move between options, Space and Enter toggle the focused option without closing, and Escape closes it and returns focus to the trigger.

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

#### Scenario: Hidden option is dimmed
- **WHEN** "Done" is hidden
- **THEN** the "Done" row SHALL render dimmed and the other rows SHALL render at full opacity

#### Scenario: Selection survives an app restart
- **WHEN** the user hides "Backlog" and restarts the app
- **THEN** "Backlog" SHALL still be hidden

#### Scenario: Show all restores every state
- **WHEN** the user picks "Show all" in the dropdown
- **THEN** no state SHALL be hidden

### Requirement: State sub-groups in Project, Assignee and Cycle views

In the Project, Assignee and Cycle views, each section SHALL split its issues into collapsible sub-groups by state name. A sub-header SHALL reuse the State-tab group header: chevron, state glyph, state name and issue count in a full-width row. Sub-groups SHALL follow the name-first state order, with closed states last, and issues without a state SHALL form a trailing "No state" sub-group. Clicking a sub-header, or pressing Arrow Left or Arrow Right on it, SHALL fold or unfold it through the same collapsed set as the section headers. A collapsed key SHALL combine the section key and the state name, so folding a state in one section SHALL NOT fold it in another. The hidden-state filter SHALL apply in every view. Sub-issue trees under a parent SHALL stay unchanged.

#### Scenario: Sub-groups follow the state order
- **WHEN** a project section holds issues in Done, Todo and In Review
- **THEN** the section SHALL show sub-groups In Review, Todo, Done in that order, each with its issue count

#### Scenario: Folding is per section
- **WHEN** the user folds "Todo" inside project A
- **THEN** the "Todo" sub-group inside project B SHALL stay expanded

#### Scenario: Hidden states apply to sub-groups
- **WHEN** "Done" is hidden and the Assignee view is active
- **THEN** no section SHALL show a "Done" sub-group
