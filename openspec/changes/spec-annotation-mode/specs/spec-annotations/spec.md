## ADDED Requirements

### Requirement: Default read-only mode
The `SpecViewer` SHALL open every active spec file in read-only mode by default. No inline editing, text selection popovers, or pinpoint hover targeting SHALL be active until the user explicitly enables annotation mode.

#### Scenario: Spec opens in read-only
- **WHEN** user opens any spec file in the spec panel
- **THEN** the markdown content SHALL render without editable affordances
- **AND** no annotation toolbar SHALL be visible
- **AND** hover, click, and text selection SHALL behave as normal browser interactions (scroll, copy)

#### Scenario: Toggle into annotation mode via button
- **WHEN** user clicks the "Annotate" button in the `SpecViewer` toolbar
- **THEN** the view SHALL switch to annotation mode with pinpoint targeting and selection popovers enabled
- **AND** the button SHALL show an active/pressed state

#### Scenario: Toggle into annotation mode via shortcut
- **WHEN** the `SpecViewer` has focus and the user presses the annotation mode shortcut
- **THEN** the view SHALL switch to annotation mode
- **AND** the shortcut SHALL NOT trigger any other action while the `SpecViewer` is focused

#### Scenario: Exit annotation mode
- **WHEN** user is in annotation mode and clicks the active "Annotate" button again, presses the shortcut again, or presses Escape with no active selection
- **THEN** the view SHALL return to read-only mode
- **AND** any in-progress pinpoint selection or text highlight SHALL be cleared (but stored annotations SHALL remain)

### Requirement: Annotation targeting and types reuse plan-annotations behavior
While in annotation mode, the `SpecViewer` SHALL use the same `AnnotatableMarkdownView` component and annotation types (Comment, Replace, Delete, Insert) defined by the `plan-annotations` capability, with identical hover targeting, selection popovers, and gutter markers.

#### Scenario: Pinpoint hover highlights elements
- **WHEN** annotation mode is active and user hovers over a paragraph, heading, list item, table cell, or code block
- **THEN** the element SHALL display the same dashed outline and tooltip used in plan annotations

#### Scenario: All four annotation types available
- **WHEN** user clicks a pinpointed element
- **THEN** the floating toolbar SHALL offer Comment, Replace, Delete, and Insert actions with identical semantics to plan annotations

#### Scenario: Text selection creates annotation
- **WHEN** user selects text within the spec content and releases the mouse
- **THEN** the selected text SHALL persist as a `<mark>` and a popover with Comment and Replace SHALL appear

### Requirement: Per-spec annotation namespace
The annotation store SHALL scope annotations by target identifier using the format `spec:<relative-spec-path>` so that annotations on different specs, and annotations on plans vs specs, remain isolated from one another.

#### Scenario: Annotations isolated per spec file
- **WHEN** user adds annotations on spec A and then opens spec B
- **THEN** spec B SHALL display zero annotations
- **WHEN** user reopens spec A
- **THEN** spec A SHALL display its original annotations intact

#### Scenario: Plan and spec annotations do not collide
- **WHEN** the active session has plan annotations pending AND the user adds spec annotations
- **THEN** both sets SHALL remain independently visible in their respective panels
- **AND** neither SHALL contaminate the other's count or serialization

### Requirement: Send annotations to Claude via chat injection
The system SHALL provide a "Send to Claude" action in the `SpecViewer` annotation toolbar that serializes all annotations for the current spec and queues them into `HookState.pending_annotations` so they are injected into the user's next prompt via the `inject-edits` (`UserPromptSubmit`) hook. This path SHALL NOT use the plan-review FIFO.

#### Scenario: Send to Claude queues feedback
- **WHEN** user has one or more annotations on a spec and clicks "Send to Claude"
- **THEN** the system SHALL call `serializeAnnotations()` scoped to the current spec target
- **AND** the serialized result (including the spec's relative path as context) SHALL be written to `HookState.pending_annotations`
- **AND** the annotations for that spec SHALL be cleared from the local store
- **AND** a toast SHALL confirm "Feedback queued for next message"

#### Scenario: Pending feedback indicator
- **WHEN** `HookState.pending_annotations` contains queued spec feedback that has not yet been injected
- **THEN** the `SpecViewer` SHALL display a persistent indicator showing the count of pending annotations
- **AND** the indicator SHALL disappear once the feedback is injected by the next `UserPromptSubmit` hook

#### Scenario: Next user prompt injects feedback
- **WHEN** the user sends their next message in the active session after queuing spec feedback
- **THEN** the `inject-edits` hook SHALL read `pending_annotations` and append the serialized feedback to the prompt submitted to Claude
- **AND** `pending_annotations` SHALL be cleared after injection

#### Scenario: Discard queued feedback
- **WHEN** user clicks a "Discard" action on the pending feedback indicator
- **THEN** `pending_annotations` SHALL be cleared without being sent
- **AND** the indicator SHALL disappear

### Requirement: Direct edit mode as explicit escape hatch
The `SpecViewer` SHALL continue to support direct markdown editing, but only behind an explicit "Edit directly" action in the toolbar. Entering edit mode SHALL display a warning that bypasses the OpenSpec change flow.

#### Scenario: Edit directly requires explicit action
- **WHEN** user clicks the "Edit directly" button
- **THEN** the view SHALL switch to an editable markdown editor
- **AND** an inline warning SHALL be displayed: "Editing spec directly bypasses OpenSpec change flow"

#### Scenario: Edit mode is not the default
- **WHEN** a user opens any spec file without prior interaction
- **THEN** the view SHALL NOT start in edit mode under any circumstance
