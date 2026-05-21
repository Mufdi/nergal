# spec-annotations Specification

## Purpose
TBD - created by archiving change spec-annotation-mode. Update Purpose after archive.
## Requirements
### Requirement: Default read-only mode

The `SpecPanel` SHALL open every active spec file in read-only mode by default. No inline editing, text selection popovers, or pinpoint hover targeting SHALL be active until the user explicitly enables annotation mode.

#### Scenario: Spec opens in read-only

- **WHEN** user opens any spec file in the spec panel
- **THEN** the markdown content SHALL render without editable affordances
- **AND** no annotation toolbar SHALL be visible
- **AND** hover, click, and text selection SHALL behave as normal browser interactions (scroll, copy)

#### Scenario: Toggle into annotation mode via button

- **WHEN** user clicks the "Annotate" button in the `SpecPanel` toolbar
- **THEN** the view SHALL switch to annotation mode with pinpoint targeting and selection popovers enabled
- **AND** the button SHALL show an active/pressed state

#### Scenario: Toggle into annotation mode via shortcut

- **WHEN** the `SpecPanel` has focus and the user presses `Ctrl+Shift+H`
- **THEN** the view SHALL switch to annotation mode
- **AND** the shortcut SHALL NOT trigger any other action while the `SpecPanel` is focused

#### Scenario: Exit annotation mode

- **WHEN** user is in annotation mode and clicks the active "Annotate" button again, presses the shortcut again, or presses Escape with no active selection
- **THEN** the view SHALL return to read-only mode
- **AND** any in-progress pinpoint selection or text highlight SHALL be cleared (but stored annotations SHALL remain)

### Requirement: Annotation targeting and types reuse plan-annotations behavior

While in annotation mode, the `SpecPanel` SHALL use the same `AnnotatableMarkdownView` component and annotation types (Comment, Replace, Delete, Insert) defined by the `plan-annotations` capability, with identical hover targeting, selection popovers, and gutter markers.

#### Scenario: Pinpoint hover highlights elements

- **WHEN** annotation mode is active and user hovers over a paragraph, heading, list item, table cell, or code block
- **THEN** the element SHALL display the same dashed outline and tooltip used in plan annotations

#### Scenario: All four annotation types available

- **WHEN** user clicks a pinpointed element
- **THEN** the floating toolbar SHALL offer Comment, Replace, Delete, and Insert actions with identical semantics to plan annotations

#### Scenario: Text selection creates annotation

- **WHEN** user selects text within the spec content and releases the mouse
- **THEN** the selected text SHALL persist as a `<mark>` and a popover with Comment and Replace SHALL appear

### Requirement: Annotation scope is discriminated, not string-keyed

The annotation store SHALL discriminate plan vs spec scope via an `AnnotationScope` tagged union (`{ kind: "plan"; sessionId } | { kind: "spec"; specPath }`) and SHALL hold per-scope buffers in separate atoms (`annotationMapAtom` for plans keyed by `sessionId`, `specAnnotationMapAtom` for specs keyed by `specPath`). This guarantees compile-time isolation between plan and spec annotations without a composite string identifier.

#### Scenario: Annotations isolated per spec file

- **WHEN** user adds annotations on spec A and then opens spec B
- **THEN** spec B SHALL display zero annotations
- **WHEN** user reopens spec A
- **THEN** spec A SHALL display its original annotations intact

#### Scenario: Plan and spec annotations do not collide

- **WHEN** the active session has plan annotations pending AND the user adds spec annotations
- **THEN** both sets SHALL remain independently visible in their respective panels
- **AND** neither SHALL contaminate the other's count or serialization
- **AND** the plan delivery path (plan-review FIFO) and the spec delivery path (`UserPromptSubmit` hook) SHALL operate independently

### Requirement: Spec annotations persist to SQLite for the duration of the change

Spec annotations SHALL be persisted to SQLite via the Tauri commands `save_spec_annotation`, `get_spec_annotations(spec_key)`, `delete_spec_annotation(id)`, `clear_spec_annotations(spec_key)`, and `count_spec_annotations_by_prefix(like_pattern)`. Persistence SHALL be keyed by `spec_key` (the spec's repo-relative artifact path) so annotations survive panel closes, tab switches, and app restarts while the change is still active.

This requirement supersedes the MVP "in-memory only" intent in the original proposal — persistence proved necessary because users edit specs across sessions and lose context when annotations evaporate.

#### Scenario: Annotations survive panel close

- **WHEN** user creates annotations on a spec and closes the spec panel
- **THEN** the next reopen of that spec SHALL repopulate the annotations from SQLite
- **AND** annotation counts in artifact tabs SHALL reflect the persisted state

#### Scenario: Counts aggregated across a change

- **WHEN** a change contains multiple spec artifacts and several have annotations
- **THEN** `count_spec_annotations_by_prefix(<change-prefix>)` SHALL return the aggregate count
- **AND** the artifact tab bar SHALL display per-artifact counts using the same source of truth

### Requirement: Send annotations to Claude via chat injection

The `SpecPanel` SHALL provide a "Send to Claude" action in the annotation toolbar that serializes all annotations for the currently viewed spec (or other artifact) and queues them into `HookState.pending_annotations` via the `set_pending_annotations` Tauri command. The serialized payload SHALL include the artifact's repo-relative path so Claude can read the file directly. Delivery SHALL happen via the existing `inject-edits` (`UserPromptSubmit`) hook — NOT via the plan-review FIFO.

#### Scenario: Send to Claude queues feedback

- **WHEN** user has one or more annotations on a spec and clicks "Send to Claude" (or presses `Ctrl+Shift+R`)
- **THEN** the system SHALL call `serializeSpecAnnotations(annotations, { changeName, artifactPath, isMaster })`
- **AND** the serialized result SHALL be passed to `set_pending_annotations`
- **AND** the local annotations for that scope SHALL be cleared (in-memory atom + SQLite for that `spec_key`)
- **AND** annotation mode SHALL exit
- **AND** a toast SHALL confirm "Feedback queued — Will be sent in your next prompt"

#### Scenario: Master spec vs change-scoped spec context

- **WHEN** the spec being annotated is under `openspec/specs/<capability>/spec.md` (master spec)
- **THEN** the serialized payload header SHALL read `Review the OpenSpec capability spec and address my annotations.`
- **AND** the `Change:` field SHALL read `(master specs)`
- **AND** the `File:` field SHALL contain the master-spec path
- **WHEN** the spec being annotated is under `openspec/changes/<change>/...`
- **THEN** the header SHALL read `Review the OpenSpec change artifact and address my annotations.`
- **AND** the `Change:` field SHALL contain the change slug
- **AND** the `File:` field SHALL contain the change-scoped path

#### Scenario: Next user prompt injects feedback

- **WHEN** the user sends their next message in the active session after queuing spec feedback
- **THEN** the `inject-edits` hook SHALL read `pending_annotations` and append the serialized feedback to the prompt submitted to Claude
- **AND** `pending_annotations` SHALL be cleared after injection

### Requirement: Global comment and clear-all affordances

While in annotation mode, the `SpecPanel` toolbar SHALL expose:

1. A "Global comment" button (and `Ctrl+Shift+O` shortcut) that opens an input for free-form feedback NOT anchored to a specific element. Submitted entries SHALL be stored with `target: "[global]"`.
2. A "Clear all" button (and `Ctrl+Shift+X` shortcut) that drops all annotations for the current scope (in-memory + SQLite for that `spec_key`) and shows a toast confirmation.

These are post-MVP additions discovered during implementation; they replace the original "explicit Edit directly escape hatch" requirement (which was deferred indefinitely — direct file editing happens in the user's external editor, not in cluihud's spec panel).

#### Scenario: Global comment captures non-anchored feedback

- **WHEN** user opens the global comment input and submits non-empty text
- **THEN** an annotation of type `comment` SHALL be added with `target: "[global]"` and empty DOM metas
- **AND** the global input SHALL collapse

#### Scenario: Clear all wipes the current scope

- **WHEN** user clicks "Clear all" (or presses `Ctrl+Shift+X`)
- **THEN** the local atom for the active scope SHALL be emptied
- **AND** the persisted SQLite rows for that `spec_key` SHALL be deleted
- **AND** a toast SHALL confirm "Cleared — All annotations removed"

### Requirement: Multi-artifact navigation with per-artifact counts

When a change exposes multiple artifacts (proposal, design, tasks, specs), the `SpecPanel` SHALL render an artifact tab bar at the top of the panel. Each tab SHALL display:

- The artifact's icon + label.
- For `specs` tabs: the count of spec files in the change.
- For any artifact with annotations: a count badge.

The active artifact SHALL be visually distinguished and switching artifacts SHALL preserve the scroll position-aware annotation state of the previously active artifact (annotations remain on each artifact independently).

#### Scenario: Annotation counts per artifact tab

- **WHEN** a change has annotations on proposal.md and on a spec file
- **THEN** the proposal tab SHALL render its count badge
- **AND** the specs tab SHALL render its aggregate count badge (summed across all spec files in the change)
- **AND** counts SHALL update reactively when annotations are added or removed

