## MODIFIED Requirements

### Requirement: Pinpoint mode hover targeting
The system SHALL highlight plan elements on hover with a dashed outline and a label tooltip describing the target type (paragraph, heading, list item, code block, table cell). Targeting SHALL be context-aware. Hover outlines SHALL NOT appear while a pinpoint selection or text selection is active.

#### Scenario: Hover highlights paragraph
- **WHEN** user hovers over a paragraph in the plan viewer
- **THEN** the paragraph SHALL display a dashed outline with a tooltip label "paragraph"

#### Scenario: Context-aware table targeting
- **WHEN** user hovers near the outer edge of a table
- **THEN** the entire table SHALL be highlighted
- **WHEN** user hovers inside a specific cell
- **THEN** only that cell SHALL be highlighted

#### Scenario: Context-aware list targeting
- **WHEN** user hovers over a specific list item
- **THEN** that individual item SHALL be highlighted
- **WHEN** user hovers in the gap between items or the left gutter
- **THEN** the entire list SHALL be highlighted

#### Scenario: Pinpoint click opens toolbar
- **WHEN** user clicks a highlighted element in pinpoint mode
- **THEN** a floating toolbar SHALL appear with four actions: Comment, Replace, Delete, Insert
- **AND** the element SHALL display a yellow dashed outline replacing the hover orange

#### Scenario: Hover suppressed during active selection
- **WHEN** a pinpoint element is active (yellow outline) OR text is selected (yellow highlight)
- **THEN** hover outlines on other elements SHALL NOT appear

### Requirement: Selection mode text annotation
The system SHALL allow free text selection within the plan. Selected text SHALL persist visually as a yellow highlight (`<mark>`) after the mouse is released. A popover with Comment and Replace options SHALL appear. Selection SHALL work across element boundaries.

#### Scenario: Text selection persists after mouseup
- **WHEN** user selects text within the plan content and releases the mouse
- **THEN** the selected text SHALL remain highlighted with a yellow background via DOM `<mark>` elements
- **AND** the native browser selection SHALL be cleared (replaced by the mark)

#### Scenario: Cross-element selection works
- **WHEN** user selects text that spans across multiple DOM elements (e.g., bold + normal text)
- **THEN** each text node within the range SHALL be individually wrapped in a `<mark>` element
- **AND** no `InvalidStateError` SHALL be thrown

#### Scenario: Selection replaces pinpoint
- **WHEN** a pinpoint element is active (yellow outline) and user then selects text
- **THEN** the pinpoint outline SHALL be removed and only the text selection highlight SHALL remain

#### Scenario: Selection shows popover
- **WHEN** text is selected and highlighted
- **THEN** a popover SHALL appear near the selection with Comment and Replace buttons

### Requirement: Structured feedback export on Revise
The system SHALL serialize annotations into structured instructions and inject them via the `UserPromptSubmit` hook (`inject-edits`) when the user clicks "Revise". Annotations SHALL NOT be embedded as HTML comments in the plan file.

#### Scenario: Revise injects feedback via hook
- **WHEN** user clicks "Revise" with annotations
- **THEN** the system SHALL serialize annotations via `serializeAnnotations()`, store the result in backend `HookState` as `pending_annotations`, and invoke `reject_plan`
- **AND** the `inject-edits` hook SHALL read `pending_annotations` and append the feedback to Claude's next prompt

#### Scenario: Revise clears annotations after send
- **WHEN** revise completes successfully
- **THEN** all annotations SHALL be cleared from both Jotai state and SQLite

#### Scenario: Approve clears annotations
- **WHEN** user clicks "Approve" on a plan with annotations
- **THEN** all annotations SHALL be cleared and the plan SHALL be accepted as-is

### Requirement: Escape clears all interaction state
The system SHALL clear all active interaction state (pinpoint selection, text selection marks, toolbar) when the user presses Escape.

#### Scenario: Escape clears pinpoint
- **WHEN** a pinpoint element is active and user presses Escape
- **THEN** the yellow outline SHALL be removed and the toolbar SHALL close

#### Scenario: Escape clears text selection
- **WHEN** text selection marks exist and user presses Escape
- **THEN** all `<mark class="pending-selection">` elements SHALL be unwrapped and the toolbar SHALL close
