# linear-task-panel Specification

## Purpose
TBD - created by archiving change linear-mirror. Update Purpose after archive.
## Requirements
### Requirement: Read-only Linear right panel

The system SHALL provide a right-panel view registered as a singleton `linear` view (category `tool`), openable from a TopBar icon and a keyboard shortcut, whose entry point is shown only when a Linear API key is configured. The panel SHALL render its issue list exclusively from the mirror and SHALL refresh on the `linear:changed` event. The panel SHALL be read-only in this capability; write affordances are introduced by `linear-writeback`.

#### Scenario: Panel entry hidden without a key

- **WHEN** no Linear API key is configured
- **THEN** the panel's TopBar entry point SHALL NOT be shown

#### Scenario: Panel lists issues from the mirror

- **WHEN** the user opens the panel with a configured key
- **THEN** the panel SHALL list issues read from the mirror
- **AND** SHALL update when a `linear:changed` event fires

#### Scenario: Panel works without an active session

- **WHEN** the user opens the panel while no session is active
- **THEN** the panel SHALL render the issue list normally under a session-independent panel key
- **AND** SHALL NOT render an empty surface or depend on per-session panel state

### Requirement: Team scoping and organization

The panel SHALL present a persistent Team selector in its header offering "Todos" (all selected teams) plus each team, defaulting to "Todos". The panel SHALL let the user group issues by workflow state, project, or assignee, and filter to issues assigned to the viewer. Grouping and filtering SHALL be computed locally over the mirror without re-querying Linear.

#### Scenario: Team scope filters the list

- **WHEN** the user selects a specific team in the header selector
- **THEN** the panel SHALL show only that team's issues
- **AND** selecting "Todos" SHALL show issues from all selected teams

#### Scenario: Group-by recomputed locally

- **WHEN** the user changes the group-by mode
- **THEN** the panel SHALL regroup from the mirror
- **AND** SHALL NOT issue a Linear GraphQL call to do so

#### Scenario: Assigned-to-me filter

- **WHEN** the user enables the assigned-to-me filter
- **THEN** the panel SHALL show only issues whose assignee is the viewer
- **AND** the filter SHALL resolve the viewer from an id cached at key validation, not from a possibly-stale status event

### Requirement: Completed issues shown via the mirror window

The panel SHALL offer a show-completed toggle. Because completed issues within the rolling poll window are already in the mirror (tombstoned only when they age out of scope), the toggle SHALL reveal mirror issues whose workflow-state `type` is `completed` or `canceled` rather than issuing a separate fetch, and SHALL be subject to the same local filters (team scope, assigned-to-me, group-by). Issues that have aged out of the window SHALL NOT be resurrected by the toggle.

#### Scenario: Show-completed reveals in-window done issues

- **WHEN** the user enables show-completed
- **THEN** the panel SHALL include mirror issues whose state `type` is `completed`/`canceled`
- **AND** SHALL NOT issue a separate Linear fetch to do so

#### Scenario: Show-completed respects assigned-to-me

- **WHEN** show-completed and assigned-to-me are both active
- **THEN** the panel SHALL show only the viewer's own completed issues

### Requirement: Keyboard-first navigation

The panel SHALL be a focus zone (`data-focus-zone="linear"`). List rows SHALL participate in arrow navigation via `data-nav-item`, navigable with the within-list tier (`Alt+↑/↓`). Collapsible groups SHALL support `data-nav-expanded` with arrow-left/right collapse. The group-by mode SHALL cycle with `Shift+←/→`. `Enter` on a selected issue SHALL open its detail. All bindings SHALL match on `event.code` and SHALL be checked against `src/stores/shortcuts.ts` for collisions before being added.

#### Scenario: Arrow navigation through the list

- **WHEN** the panel zone is focused and the user presses the within-list nav keys
- **THEN** selection SHALL move between issue rows
- **AND** `Enter` SHALL open the selected issue's detail

#### Scenario: Group collapse

- **WHEN** a group header is selected and the user presses arrow-left
- **THEN** the group SHALL collapse via its `data-nav-expanded` state

### Requirement: Floating issue-detail module

The system SHALL render a selected issue's full detail in a floating module (not in the narrow panel), reusing the existing floating-module pattern. The module SHALL show the markdown description, the sub-issue tree, the comment thread, and labels/priority/project/cycle as chips; attachments SHALL render as chips — image attachments SHALL show a thumbnail and SHALL open the original in the browser on click; the system SHALL NOT store attachment binaries. The module SHALL be read-only in this capability. The issue's StatusIcon SHALL derive its open/in-progress/done class from the workflow state's native `type`, and the priority glyph SHALL derive from the issue's integer priority.

#### Scenario: Open full detail in the floating module

- **WHEN** the user opens an issue from the list
- **THEN** a floating module SHALL render the issue's description, sub-issues, comments, and label/priority chips
- **AND** the narrow panel SHALL NOT be used as the full-detail surface

#### Scenario: Attachment opens in browser without local storage

- **WHEN** the user clicks an attachment chip
- **THEN** the system SHALL open the attachment's original URL in the browser
- **AND** SHALL NOT have stored the attachment binary locally

### Requirement: Untrusted Linear content rendered safely

Linear issue descriptions and comments are multi-writer untrusted input rendered in a WebKitGTK webview. The system SHALL render markdown descriptions and comments through a sanitizing pipeline that strips raw HTML and script (no raw-HTML passthrough) and allow-lists link schemes to `http`/`https` (no `javascript:`/`data:`). Because markdown-native inline images (`![](url)`) otherwise auto-fetch on render — a tracking-pixel / presence-leak / SSRF surface — the renderer SHALL **gate remote inline images** (suppress auto-load behind a click-to-load affordance or an image-host allow-list) rather than emit a bare auto-loading `<img>`. Attachment image thumbnails SHALL load lazily only on detail open (never in list rows) and SHALL be gated so a non-image attachment URL cannot auto-load. Opening any attachment or link original SHALL be an explicit user action **and SHALL pass the untrusted URL through the backend `validate_url` http/https allow-list before opening**, not a raw hand-off to the OS opener.

#### Scenario: Description and comments are sanitized

- **WHEN** an issue description or comment contains raw HTML, a script, or a `javascript:`/`data:` link
- **THEN** the renderer SHALL strip the HTML/script and SHALL NOT resolve the dangerous link scheme

#### Scenario: Inline remote images do not auto-load

- **WHEN** a description or comment contains an inline markdown image with a remote URL
- **THEN** the renderer SHALL NOT auto-fetch it on detail open
- **AND** loading SHALL require an explicit user action (or pass an image-host allow-list)

#### Scenario: Opening an attachment validates the scheme

- **WHEN** the user clicks an attachment chip or a link
- **THEN** the system SHALL pass the URL through `validate_url` (http/https allow-list) before opening
- **AND** a non-http(s) scheme SHALL be refused

#### Scenario: Thumbnails load lazily and gated

- **WHEN** the issue list renders
- **THEN** no attachment thumbnail SHALL auto-load in a list row
- **AND** thumbnails SHALL load only on detail open, gated against non-image URLs

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

