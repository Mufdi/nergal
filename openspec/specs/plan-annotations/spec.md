## Purpose

Enable inline plan annotations (comment, replace, delete, insert) with pinpoint targeting, selection popovers, and structured feedback export on revise.

## Requirements

### Requirement: Pinpoint mode hover targeting
The system SHALL highlight plan elements on hover with a dashed outline and a label tooltip describing the target type (paragraph, heading, list item, code block, table cell). Targeting SHALL be context-aware.

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

### Requirement: Selection mode text annotation
The system SHALL allow free text selection within the plan, showing an annotation popover with Comment and Replace options.

#### Scenario: Text selection shows popover
- **WHEN** user selects text within the plan content
- **THEN** a popover SHALL appear near the selection with Comment and Replace buttons

#### Scenario: Comment annotation on selection
- **WHEN** user clicks Comment in the selection popover
- **THEN** a text input SHALL appear for entering the comment, attached to the selected text range

#### Scenario: Replace annotation on selection
- **WHEN** user clicks Replace in the selection popover
- **THEN** a text input SHALL appear pre-filled with the selected text, allowing the user to write the replacement

### Requirement: Annotation types
The system SHALL support four annotation types: Comment (note without modification), Replace (propose text substitution), Delete (mark for removal with reason), and Insert (add content at position).

#### Scenario: Delete annotation with reason
- **WHEN** user selects Delete from the pinpoint toolbar
- **THEN** the system SHALL prompt for a reason and visually mark the element with strikethrough styling and a red gutter indicator

#### Scenario: Insert annotation
- **WHEN** user selects Insert from the pinpoint toolbar
- **THEN** an inline editor SHALL appear below the targeted element for entering new content

#### Scenario: Annotations rendered as gutter markers
- **WHEN** one or more annotations exist on the plan
- **THEN** each annotated element SHALL display a colored gutter indicator (blue for comment, yellow for replace, red for delete, green for insert)

### Requirement: Annotation store and count
The system SHALL maintain an annotation store (array of typed annotations) and display the total annotation count in the plan panel footer.

#### Scenario: Annotation count displayed
- **WHEN** annotations exist on the active plan
- **THEN** the plan panel footer SHALL show the count (e.g., "3 annotations")

#### Scenario: Clear all annotations
- **WHEN** user clicks a clear button in the annotation footer
- **THEN** all annotations SHALL be removed from the store and visual markers cleared

### Requirement: Structured feedback export on Revise
The system SHALL serialize annotations into structured instructions and deliver them via the `PermissionRequest[ExitPlanMode]` hook's deny message when the user clicks "Revise". The hook blocks Claude until the user approves or denies.

#### Scenario: Revise exports structured feedback
- **WHEN** user clicks "Revise" with 3 annotations
- **THEN** the system SHALL deny the PermissionRequest with message: "YOUR PLAN WAS NOT APPROVED. You MUST revise the plan to address ALL of the feedback below... [1] DELETE section 'X' — reason: Y. [2] REPLACE 'A' with 'B' in section Z. [3] COMMENT on step N: feedback."
- **AND** Claude SHALL automatically re-plan based on the deny message

#### Scenario: Approve allows Claude to proceed
- **WHEN** user clicks "Approve" on a plan
- **THEN** all annotations SHALL be cleared and the PermissionRequest SHALL be allowed, letting Claude proceed

#### Scenario: Annotations disabled outside plan review
- **WHEN** Claude is not awaiting plan approval (no pending PermissionRequest)
- **THEN** the annotation toolbar, pinpoint mode, and selection highlighting SHALL be disabled
- **AND** the plan SHALL be displayed in read-only mode
