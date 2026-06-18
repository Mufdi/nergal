# linear-mirror

## ADDED Requirements

### Requirement: Faithful Linear status glyphs and group order

The panel SHALL render each Linear workflow state with a glyph faithful to Linear's own: triage (intake ring), backlog (dashed ring), unstarted (hollow ring), started (proportional pie), completed (filled disc with a check), and canceled and duplicate (filled disc with an ✕). `duplicate` SHALL be treated as a distinct state type, not a flavour of canceled. When grouping by state, groups SHALL be ordered triage → started → unstarted → backlog → completed → canceled → duplicate, ties broken by the workflow `position` ascending (so a completed state with a lower position precedes a later one).

#### Scenario: Distinct glyphs per state type
- **WHEN** the panel renders states of type backlog, canceled, and duplicate
- **THEN** backlog SHALL show a dashed ring, and canceled and duplicate SHALL show a filled disc with an ✕ (not a check)

#### Scenario: Group order follows Linear
- **WHEN** issues are grouped by state
- **THEN** the groups SHALL appear in the order triage, started, unstarted, backlog, completed, canceled, duplicate
- **AND** two completed states SHALL order by workflow position ascending

### Requirement: Completed, canceled, and duplicate issues visible by default

The panel SHALL show completed, canceled, and duplicate issues by default; the existing "show completed" control SHALL still hide them on demand and SHALL govern duplicate consistently with completed and canceled.

#### Scenario: Terminal issues shown by default
- **WHEN** the panel first loads with mirrored completed/canceled/duplicate issues
- **THEN** those issues SHALL be visible without toggling any control

### Requirement: Configurable default view per tracker

The system SHALL persist a default view per tracker chosen from that tracker's existing chip views (Linear: my issues, state, project, assignee, cycle; ClickUp: my tasks, status, list, assignee) and SHALL apply it when the panel first opens, unless the user has already changed the view in the session. An unset or unrecognized default SHALL fall back to the "my issues" view (today's behavior).

#### Scenario: Default view applied on open
- **WHEN** the default view is set to "project" and the panel opens
- **THEN** the panel SHALL group by project with the assigned-to-me filter off

#### Scenario: Unset default preserves current behavior
- **WHEN** no default view is configured
- **THEN** the panel SHALL open on the "my issues" view

### Requirement: Multiple Linear workspaces with one active mirror

The system SHALL store one Personal API key per Linear workspace (secrets in the keyring, non-secret workspace metadata — org id, name, url key — in the database) and SHALL mirror exactly one active workspace at a time. Adding a workspace SHALL validate its key and resolve its organization; the first added workspace SHALL become active. Setting a different workspace active SHALL bump the key-generation epoch, wipe the mirrored issue/team/state/label/cycle/project rows, clear the selected teams, and re-sync — so data from one workspace can never appear under another. Removing the active workspace SHALL clear the active selection and wipe the mirror. An existing single stored key SHALL be migrated to a namespaced per-workspace entry on first run, idempotently.

#### Scenario: Switching workspaces wipes and re-syncs
- **WHEN** the user sets a different workspace active
- **THEN** the system SHALL wipe the current mirror, clear selected teams, bump the epoch, and re-sync the new workspace
- **AND** an in-flight poll from the previous workspace SHALL NOT commit into the new one

#### Scenario: First workspace becomes active
- **WHEN** the user adds the first workspace key
- **THEN** that workspace SHALL become the active mirror

#### Scenario: Legacy single key migrates once
- **WHEN** a legacy single Linear key exists and no workspace rows do
- **THEN** the system SHALL validate it, resolve its workspace, store it namespaced, and mark it active
- **AND** a subsequent run SHALL NOT duplicate the migration

### Requirement: Issue activity feed in the detail

The issue detail SHALL fetch the issue's history live on open (not mirrored, like attachments and relations) and render an Activity section under the description showing, most-recent first and capped, the issue's creation and its state, assignee, label, cycle, and priority changes, each as actor + change + relative time. When the history exceeds the cap the system SHALL note the omission.

#### Scenario: Activity rendered under the description
- **WHEN** an issue with history is opened
- **THEN** the detail SHALL show an Activity section under the description listing creation and state/assignee/label/cycle/priority changes
- **AND** the history SHALL be fetched live, not read from the mirror
