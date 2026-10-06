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

