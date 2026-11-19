## REMOVED Requirements

### Requirement: Faithful Linear status glyphs and group order
**Reason**: The rank used a hard-coded map of state names and a legacy `Pending` rule that went stale when the state was renamed to `Shipped`.
**Migration**: Replaced by "Status glyphs and data-driven group order", which ranks by state type and workflow position only.

### Requirement: Completed, canceled, and duplicate issues visible by default
**Reason**: It classified closed states with the name-first rank and pinned the legacy `Pending` exception.
**Migration**: Replaced by "Closed issues visible by default", which classifies by state type.

## ADDED Requirements

### Requirement: Status glyphs and data-driven group order

The panel SHALL render each Linear workflow state with a glyph faithful to Linear's own: triage (intake ring), backlog (dashed ring), unstarted (hollow ring), started (proportional pie), completed (filled disc with a check), and canceled and duplicate (filled disc with an ✕). `duplicate` SHALL be treated as a distinct state type, not a flavour of canceled. The panel SHALL rank workflow states from their type and workflow `position` only, never from their name. Types SHALL rank started → unstarted → backlog → triage → any other open type → completed → duplicate → canceled, so every closed state ranks below every open state. Inside an open type, a higher `position` SHALL rank first; inside a closed type, a lower `position` SHALL rank first. A missing `position` counts as 0, and equal ranks order by state name. Issues without a state SHALL sort last. When grouping by state, groups SHALL follow this rank. In project, assignee and cycle groupings, issues inside each group SHALL follow the same rank, then the active sort.

#### Scenario: Distinct glyphs per state type
- **WHEN** the panel renders states of type backlog, canceled, and duplicate
- **THEN** backlog SHALL show a dashed ring, and canceled and duplicate SHALL show a filled disc with an ✕ (not a check)

#### Scenario: Group order for team RR's workflow
- **GIVEN** the states Triage (triage, 0), Backlog (backlog, 0), Todo (unstarted, 1), In Progress (started, 2), Done (completed, 3), Canceled (canceled, 4), Duplicate (duplicate, 5), Shipped (completed, 954) and In Review (started, 1002)
- **WHEN** issues in all of them are grouped by state
- **THEN** the groups SHALL appear in the order In Review, In Progress, Todo, Backlog, Triage, Done, Shipped, Duplicate, Canceled

#### Scenario: A renamed state keeps its place
- **GIVEN** a state of type `started` named "Code review" with position 1002
- **WHEN** it is ranked against "In Progress" (started, 2)
- **THEN** "Code review" SHALL rank first, with no name rule involved

#### Scenario: Closed states never interleave with open ones
- **WHEN** an open state has a higher workflow `position` than a closed state
- **THEN** the open state SHALL still appear above the closed state

#### Scenario: Unknown type ranks after the known open types
- **WHEN** a state has a type the panel does not know
- **THEN** it SHALL rank after Triage and above every closed state

### Requirement: Closed issues visible by default

The panel SHALL show closed issues (any state of type `completed`, `duplicate` or `canceled`) by default. Turning the existing "show completed" control (H) off SHALL fold the closed states, not remove them: each closed state group or sub-group keeps its header with glyph, name and issue count, and lists no issues. The control SHALL govern duplicate consistently with completed and canceled.

#### Scenario: Terminal issues shown by default
- **WHEN** the panel first loads with mirrored completed/canceled/duplicate issues
- **THEN** those issues SHALL be visible without toggling any control

#### Scenario: Show-completed off folds every closed type
- **WHEN** the show-completed control is off and the State view holds issues in Done, Shipped, Duplicate, Canceled and In Review
- **THEN** the Done, Shipped, Duplicate and Canceled group headers SHALL stay visible with their issue counts and list no issues
- **AND** the In Review group SHALL stay unfolded and list its issues
