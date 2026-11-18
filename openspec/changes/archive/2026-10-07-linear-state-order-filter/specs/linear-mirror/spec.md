## MODIFIED Requirements

### Requirement: Faithful Linear status glyphs and group order

The panel SHALL render each Linear workflow state with a glyph faithful to Linear's own: triage (intake ring), backlog (dashed ring), unstarted (hollow ring), started (proportional pie), completed (filled disc with a check), and canceled and duplicate (filled disc with an ✕). `duplicate` SHALL be treated as a distinct state type, not a flavour of canceled. The panel SHALL rank workflow states by state **name** first (case-insensitive) and fall back to the state type only for unknown names. Open states SHALL rank In Review → In Progress → Todo → Backlog → Triage → other open states; closed states SHALL rank Done → Duplicate → Canceled, and every closed state SHALL rank below every open state. The legacy `Pending` state SHALL rank with In Review and SHALL count as open, regardless of its `completed` type. Unknown names SHALL fall back by type: `started` with In Progress, `unstarted` with Todo, `backlog` with Backlog, `triage` with Triage, `completed` with Done, `duplicate` with Duplicate, `canceled` with Canceled; any other type SHALL rank after the named open states. When grouping by state, groups SHALL follow this rank, ties broken by the workflow `position` ascending. Issues without a state SHALL sort last. In project, assignee and cycle groupings, issues inside each group SHALL follow the same rank, then workflow `position`, then the active sort.

#### Scenario: Distinct glyphs per state type
- **WHEN** the panel renders states of type backlog, canceled, and duplicate
- **THEN** backlog SHALL show a dashed ring, and canceled and duplicate SHALL show a filled disc with an ✕ (not a check)

#### Scenario: Group order puts active work first
- **WHEN** issues in states Triage, Backlog, Todo, In Progress, In Review, Done, Duplicate and Canceled are grouped by state
- **THEN** the groups SHALL appear in the order In Review, In Progress, Todo, Backlog, Triage, Done, Duplicate, Canceled

#### Scenario: Duplicate and Canceled told apart by name
- **WHEN** a state named "Duplicate" and a state named "Canceled" both have type `canceled`
- **THEN** the Duplicate group SHALL appear before the Canceled group

#### Scenario: Pending ranks with In Review
- **WHEN** a state named "Pending" has type `completed`
- **THEN** it SHALL rank with In Review, above In Progress
- **AND** it SHALL NOT be classified as closed

#### Scenario: Unknown names fall back to type
- **WHEN** a state named "QA" has type `started` and a state named "Shipped" has type `completed`
- **THEN** QA SHALL rank with In Progress and Shipped SHALL rank with Done

#### Scenario: Closed states never interleave with open ones
- **WHEN** an open state has a higher workflow `position` than a closed state
- **THEN** the open state SHALL still appear above the closed state

#### Scenario: Ties ordered by workflow position
- **WHEN** two states share a rank
- **THEN** they SHALL order by workflow `position` ascending

### Requirement: Completed, canceled, and duplicate issues visible by default

The panel SHALL show closed issues (Done, Duplicate, Canceled, and any state that the name-first rank classifies as closed) by default. Turning the existing "show completed" control (H) off SHALL fold the closed states, not remove them: each closed state group or sub-group keeps its header with glyph, name and issue count, and lists no issues. The control SHALL govern duplicate consistently with completed and canceled. The control SHALL use the name-first closed classification, so it SHALL NOT fold the legacy `Pending` state.

#### Scenario: Terminal issues shown by default
- **WHEN** the panel first loads with mirrored completed/canceled/duplicate issues
- **THEN** those issues SHALL be visible without toggling any control

#### Scenario: Show-completed off folds closed states and keeps Pending open
- **WHEN** the show-completed control is off and the State view holds issues in Done, Duplicate, Canceled and Pending
- **THEN** the Done, Duplicate and Canceled group headers SHALL stay visible with their issue counts and list no issues
- **AND** the `Pending` group SHALL stay unfolded and list its issues
