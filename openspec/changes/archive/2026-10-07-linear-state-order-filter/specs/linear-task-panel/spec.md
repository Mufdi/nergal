## ADDED Requirements

### Requirement: State visibility dropdown

The panel header SHALL offer a multi-select dropdown that folds or unfolds issues by workflow state name. The dropdown SHALL list the state names present in the current pool (team scope and assigned-to-me applied), deduplicated case-insensitively and ordered by the name-first state rank. A hidden state SHALL fold, not disappear: its group in the State and My issues views, and its sub-group in the Project, Assignee and Cycle views, SHALL keep the header with glyph, name and issue count and list no issues. The fold SHALL be computed locally over the mirror without a Linear call. Sub-issue trees under a parent SHALL list every child, whatever its state and whatever the show-completed control says; expanding the parent is the fold for its tree. The selection SHALL be keyed by state name, so one choice applies to every team that has a state with that name. The selection SHALL persist in Nergal's config (`linear_hidden_states`), not in browser storage, so it survives closing the panel and restarting the app even when the app quits right after the change. Each option SHALL render as a full-width row that matches a State-tab group header: the state glyph followed by the state name, with no trailing check mark. A hidden state SHALL render dimmed, and the trigger tooltip SHALL count the folded states. The dropdown SHALL follow the keyboard mechanics of the label filter popover: Arrow Down on the trigger opens it and focuses the first option, Arrow Up and Arrow Down move between options, Space and Enter toggle the focused option without closing, and Escape closes it and returns focus to the trigger.

A group folded by the dropdown or by the show-completed control SHALL unfold for the current app session when the user clicks its header or presses Arrow Right on it, and fold again with a second click or Arrow Left. This peek SHALL NOT persist. Hiding or showing that state again, or toggling the show-completed control for a closed state, SHALL clear its peek so the group folds again.

#### Scenario: Hiding a state folds its issues
- **WHEN** the user hides "Done" in the dropdown in the State view
- **THEN** the "Done" group header SHALL stay visible with its issue count
- **AND** the panel SHALL NOT list issues whose state name is "Done"
- **AND** issues in other states SHALL stay listed

#### Scenario: Options follow the state rank
- **WHEN** the pool holds issues in Backlog, In Review and Canceled
- **THEN** the dropdown SHALL list In Review, Backlog, Canceled in that order

#### Scenario: Selection survives reopening the panel
- **WHEN** the user hides "Backlog", closes the panel and opens it again
- **THEN** the "Backlog" group SHALL still be folded

#### Scenario: Hidden option is dimmed
- **WHEN** "Done" is hidden
- **THEN** the "Done" row SHALL render dimmed and the other rows SHALL render at full opacity

#### Scenario: Selection survives an app restart
- **WHEN** the user hides "Backlog" and restarts the app
- **THEN** the "Backlog" group SHALL still be folded

#### Scenario: Unfold all restores every state
- **WHEN** the user picks "Unfold all" in the dropdown
- **THEN** no state SHALL be hidden

#### Scenario: Peek unfolds a hidden state for the session
- **WHEN** "Done" is hidden and the user presses Arrow Right on the "Done" group header
- **THEN** the "Done" group SHALL list its issues
- **AND** after an app restart the "Done" group SHALL be folded again

#### Scenario: Re-hiding clears the peek
- **WHEN** the user peeked the hidden "Done" group, then shows "Done" and hides it again in the dropdown
- **THEN** the "Done" group SHALL be folded

#### Scenario: Sub-issue trees ignore folded states
- **WHEN** "Done" is hidden, show-completed is off and the user expands a parent issue with a child in Done
- **THEN** the child in Done SHALL be listed under the parent

### Requirement: State sub-groups in Project, Assignee and Cycle views

In the Project, Assignee and Cycle views, each section SHALL split its issues into collapsible sub-groups by state name. A sub-header SHALL reuse the State-tab group header: chevron, state glyph, state name and issue count in a full-width row. Sub-groups SHALL follow the name-first state order, with closed states last, and issues without a state SHALL form a trailing "No state" sub-group. Clicking a sub-header, or pressing Arrow Left or Arrow Right on it, SHALL fold or unfold it through the same collapsed set as the section headers, unless the dropdown or the show-completed control folds that state (then it peeks, as in "State visibility dropdown"). A collapsed key SHALL combine the section key and the state name, so folding a state in one section SHALL NOT fold it in another. The hidden-state fold SHALL apply in every view. Sub-issue trees under a parent SHALL stay unchanged.

#### Scenario: Sub-groups follow the state order
- **WHEN** a project section holds issues in Done, Todo and In Review
- **THEN** the section SHALL show sub-groups In Review, Todo, Done in that order, each with its issue count

#### Scenario: Folding is per section
- **WHEN** the user folds "Todo" inside project A
- **THEN** the "Todo" sub-group inside project B SHALL stay expanded

#### Scenario: Hidden states fold sub-groups
- **WHEN** "Done" is hidden and the Assignee view is active
- **THEN** every section SHALL show its "Done" sub-group header with the issue count and list no "Done" issues

### Requirement: Folded groups persist across restarts

The set of folded group and sub-group keys SHALL persist in Nergal's config (`linear_collapsed_groups`), not in browser storage, and SHALL be bounded to the 500 most recently folded keys. A config written before these fields existed SHALL load with both sets empty. Group keys SHALL derive only from server ids and state names, so a stored key keeps matching the same group after a restart.

#### Scenario: Quitting right after folding keeps the fold
- **WHEN** the user folds a group and quits the app immediately
- **THEN** `linear_collapsed_groups` in the config file SHALL contain that group key

#### Scenario: Collapsed set is bounded
- **WHEN** 520 distinct group keys have been folded
- **THEN** only the 500 most recently folded keys SHALL be persisted

#### Scenario: Folded state group survives a restart
- **WHEN** the user folds the "Done" group in the State view and restarts the app
- **THEN** the "Done" group SHALL still be folded

#### Scenario: Folded sub-group survives a restart
- **WHEN** the user folds the "Todo" sub-group inside project A and restarts the app
- **THEN** that sub-group SHALL still be folded and "Todo" inside project B SHALL stay expanded


### Requirement: Show-completed choice persists across restarts

The show-completed control (H) SHALL persist in Nergal's config (`linear_show_completed`), not in memory or browser storage, so folding closed issues survives restarting the app. A config written before the field existed SHALL read as true, so closed issues keep showing unfolded by default.

#### Scenario: Folded closed issues stay folded after restart
- **WHEN** the user turns show-completed off and restarts the app
- **THEN** the panel SHALL open with the closed state groups folded

#### Scenario: Config without the field shows closed issues
- **WHEN** the config on disk has no `linear_show_completed`
- **THEN** the panel SHALL list closed issues unfolded
