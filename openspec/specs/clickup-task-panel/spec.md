# clickup-task-panel Specification

## Purpose
TBD - created by archiving change clickup-sync. Update Purpose after archive.
## Requirements
### Requirement: Read-only ClickUp right panel

The system SHALL provide a right-panel view registered as a singleton `clickup` view (category `tool`), openable from a TopBar icon and a keyboard shortcut, whose entry point is shown only when a ClickUp token is configured. The panel SHALL render its task list exclusively from the mirror and SHALL refresh on the `clickup:changed` event. The panel SHALL be read-only in this capability; write affordances are introduced by `clickup-writeback`.

#### Scenario: Panel entry hidden without a token

- **WHEN** no ClickUp token is configured
- **THEN** the panel's TopBar entry point SHALL NOT be shown

#### Scenario: Panel lists tasks from the mirror

- **WHEN** the user opens the panel with a configured token
- **THEN** the panel SHALL list tasks read from the mirror
- **AND** SHALL update when a `clickup:changed` event fires

#### Scenario: Panel works without an active session

- **WHEN** the user opens the panel while no session is active
- **THEN** the panel SHALL render the task list normally under a session-independent panel key
- **AND** SHALL NOT render an empty surface or depend on per-session panel state

### Requirement: Space scoping and organization

The panel SHALL present a persistent Space selector in its header offering "Todos" (all Spaces) plus each Space, defaulting to "Todos". The panel SHALL let the user group tasks by status, list, or assignee, and filter to tasks assigned to the token's user. Grouping and filtering SHALL be computed locally over the mirror without re-querying ClickUp.

#### Scenario: Space scope filters the list

- **WHEN** the user selects a specific Space in the header selector
- **THEN** the panel SHALL show only that Space's tasks
- **AND** selecting "Todos" SHALL show tasks from all Spaces

#### Scenario: Group-by recomputed locally

- **WHEN** the user changes the group-by mode
- **THEN** the panel SHALL regroup from the mirror
- **AND** SHALL NOT issue a ClickUp API call to do so

#### Scenario: Assigned-to-me filter

- **WHEN** the user enables the assigned-to-me filter
- **THEN** the panel SHALL show only tasks whose assignees include the token's user
- **AND** the filter SHALL resolve the user from an id cached at token validation, not from a possibly-stale status event

### Requirement: Closed tasks shown via ephemeral fetch

The panel SHALL offer a show-closed toggle that fetches closed tasks on demand. The fetched closed tasks SHALL be ephemeral: never written to the mirror (an upsert would un-tombstone them and fight the next reconcile), merged client-side with the mirror's open tasks, and subject to the same local filters (Space scope, assigned-to-me, group-by).

#### Scenario: Closed tasks never touch the mirror

- **WHEN** the user enables show-closed and the panel fetches closed tasks
- **THEN** the fetched tasks SHALL be merged into the view client-side
- **AND** SHALL NOT be upserted into the mirror

#### Scenario: Closed tasks respect assigned-to-me

- **WHEN** show-closed and assigned-to-me are both active
- **THEN** the panel SHALL show only the user's own closed tasks

### Requirement: Keyboard-first navigation

The panel SHALL be a focus zone (`data-focus-zone="clickup"`). List rows SHALL participate in arrow navigation via `data-nav-item`, navigable with the within-list tier (`Alt+↑/↓`). Collapsible groups SHALL support `data-nav-expanded` with arrow-left/right collapse. The group-by mode SHALL cycle with `Shift+←/→`. `Enter` on a selected task SHALL open its detail. All bindings SHALL match on `event.code` and SHALL be checked against `src/stores/shortcuts.ts` for collisions before being added.

#### Scenario: Arrow navigation through the list

- **WHEN** the panel zone is focused and the user presses the within-list nav keys
- **THEN** selection SHALL move between task rows
- **AND** `Enter` SHALL open the selected task's detail

#### Scenario: Group collapse

- **WHEN** a group header is selected and the user presses arrow-left
- **THEN** the group SHALL collapse via its `data-nav-expanded` state

### Requirement: Floating task-detail module

The system SHALL render a selected task's full detail in a floating module (not in the narrow panel), reusing the existing floating-module pattern. The module SHALL show the markdown description, subtasks, checklists, the comment thread, and attachments as chips — image attachments SHALL show a thumbnail and SHALL open the original in the browser on click; the system SHALL NOT store attachment binaries. The module SHALL be read-only in this capability.

#### Scenario: Open full detail in the floating module

- **WHEN** the user opens a task from the list
- **THEN** a floating module SHALL render the task's description, subtasks, checklists, comments, and attachment chips
- **AND** the narrow panel SHALL NOT be used as the full-detail surface

#### Scenario: Attachment opens in browser without local storage

- **WHEN** the user clicks an attachment chip
- **THEN** the system SHALL open the attachment's original URL in the browser
- **AND** SHALL NOT have stored the attachment binary locally

### Requirement: Untrusted ClickUp content rendered safely

ClickUp task descriptions and comments are multi-writer untrusted input rendered in a WebKitGTK webview. The system SHALL render markdown descriptions and comments through a sanitizing pipeline that strips raw HTML and script (no raw-HTML passthrough). Attachment image thumbnails SHALL load lazily only on detail open (never in list rows) and SHALL be gated so a non-image `thumbnail_url` cannot auto-load; opening the original SHALL be an explicit user action, not an automatic fetch.

#### Scenario: Description and comments are sanitized

- **WHEN** a task description or comment contains raw HTML or script
- **THEN** the renderer SHALL strip it
- **AND** SHALL NOT execute or pass through raw HTML

#### Scenario: Thumbnails load lazily and gated

- **WHEN** the task list renders
- **THEN** no attachment thumbnail SHALL auto-load in a list row
- **AND** thumbnails SHALL load only on detail open, gated against non-image URLs

