## ADDED Requirements

### Requirement: Default read-only diff mode
The diff panel SHALL render all active diffs in read-only mode by default. Direct text editing of diff content SHALL NOT be available without explicit user action.

#### Scenario: Diff opens read-only
- **WHEN** user opens any diff in the diff panel
- **THEN** the diff SHALL render with syntax highlighting and line gutters but no editable areas
- **AND** no annotation gutters, focus rings, or input fields SHALL be visible

#### Scenario: Direct edit behind explicit toggle
- **WHEN** user clicks an "Edit directly" button in the diff panel toolbar
- **THEN** the view SHALL switch to a direct editing mode
- **AND** an inline warning SHALL be shown explaining that direct edits bypass the review flow

### Requirement: Toggle annotation mode
The diff panel SHALL support toggling into an annotation mode via a toolbar button and a keyboard shortcut. Annotation mode SHALL enable line-level focus, keyboard navigation, and the ability to add comment or suggestion annotations.

#### Scenario: Enter annotation mode via button
- **WHEN** user clicks the "Annotate" button in the diff panel toolbar
- **THEN** annotation mode SHALL activate
- **AND** the first visible diff line SHALL receive focus, indicated by a lateral border

#### Scenario: Enter annotation mode via shortcut
- **WHEN** the diff panel has focus and the user presses the annotation toggle shortcut
- **THEN** annotation mode SHALL activate
- **AND** the shortcut SHALL be scoped so it does not trigger in other panels

#### Scenario: Exit annotation mode
- **WHEN** user presses Escape with no active input or selection, clicks the active "Annotate" button again, or presses the shortcut again
- **THEN** annotation mode SHALL deactivate
- **AND** any focus ring, range selection, or open input SHALL be cleared
- **AND** already-submitted annotations in the current review batch SHALL remain

### Requirement: Keyboard navigation and interaction
While in annotation mode, the diff panel SHALL support full keyboard control with parity to mouse interaction. Mouse interaction SHALL continue to work in parallel.

#### Scenario: Navigate lines with j/k or arrow keys
- **WHEN** annotation mode is active and user presses `j` or `↓`
- **THEN** focus SHALL move to the next diff line
- **WHEN** user presses `k` or `↑`
- **THEN** focus SHALL move to the previous diff line
- **AND** the focused line SHALL be scrolled into view via `scrollIntoView({ block: 'nearest' })`

#### Scenario: Extend selection with Shift
- **WHEN** a line has focus and user presses `Shift+j` / `Shift+↓` or `Shift+k` / `Shift+↑`
- **THEN** the selection SHALL extend to include the next/previous line
- **AND** all lines in the range SHALL be visually highlighted
- **AND** a range indicator SHALL display "lines N-M selected"

#### Scenario: Open comment input with `c`
- **WHEN** a line or range has focus and user presses `c`
- **THEN** a comment input SHALL open anchored below the focused line or range
- **AND** the input SHALL be auto-focused for typing

#### Scenario: Open suggestion input with `s`
- **WHEN** a line or range has focus and user presses `s`
- **THEN** a suggestion input SHALL open with two fields: a description and a proposed code replacement
- **AND** a preview showing original lines vs proposed replacement SHALL be visible

#### Scenario: Submit annotation with Enter
- **WHEN** a comment or suggestion input is open and user presses Enter (without Shift)
- **THEN** the annotation SHALL be committed to `diffAnnotationsAtom` for the current diff
- **AND** the input SHALL close
- **AND** the focused line SHALL retain focus for continued navigation

#### Scenario: Cancel input with Escape
- **WHEN** an input is open and user presses Escape
- **THEN** the input SHALL close without committing
- **AND** focus SHALL return to the previously focused line

#### Scenario: Mouse interaction works in parallel
- **WHEN** annotation mode is active and user clicks on a line number in the gutter
- **THEN** that line SHALL receive focus
- **AND** a comment input SHALL open for that line
- **WHEN** user Shift+clicks another line
- **THEN** the range between focus and the clicked line SHALL be selected

### Requirement: Annotation types and data model
The system SHALL support two annotation types for the MVP: `comment` (note anchored to line range) and `suggestion` (note plus proposed code replacement). Each annotation SHALL carry file path, side (`before` or `after`), start line, optional end line, body text, optional suggestion replacement, and creation timestamp.

#### Scenario: Comment annotation persisted in store
- **WHEN** user submits a comment on `src/foo.ts` line 42 (side `after`)
- **THEN** `diffAnnotationsAtom["diff:src/foo.ts"]` SHALL contain a new entry with `{ filePath: "src/foo.ts", side: "after", startLine: 42, endLine: 42, kind: "comment", body }`

#### Scenario: Multi-line suggestion annotation
- **WHEN** user submits a suggestion spanning lines 58-60 with a replacement body
- **THEN** the store SHALL contain an entry with `startLine: 58, endLine: 60, kind: "suggestion", body, suggestionReplacement`

#### Scenario: Annotation marker rendered on gutter
- **WHEN** one or more annotations exist for a file
- **THEN** each annotated line SHALL display a marker icon in the gutter
- **WHEN** user clicks or focuses the marker
- **THEN** a popover SHALL show the annotation body and type

### Requirement: Review batch submission
The diff panel SHALL accumulate annotations into a review batch and submit them atomically via a "Submit review" action. Submission SHALL serialize the batch into a structured markdown block grouped by file, in line order, and write it to `HookState.pending_annotations` so that the next `UserPromptSubmit` hook injection forwards it to Claude. Individual annotations SHALL NOT be sent one at a time.

#### Scenario: Pending count visible
- **WHEN** one or more annotations exist in the current review batch
- **THEN** the diff panel footer SHALL display a count (e.g., "3 pending comments")
- **AND** a "Submit review" button SHALL be enabled

#### Scenario: Submit review queues batch
- **WHEN** user clicks "Submit review" or presses `Ctrl+Enter` with annotations in the batch
- **THEN** the system SHALL serialize all annotations grouped by file, ordered by line
- **AND** write the result to `HookState.pending_annotations` with source marker `diff-review`
- **AND** clear all annotations from `diffAnnotationsAtom` for the current review
- **AND** show a toast "Review submitted — will be sent in your next message"

#### Scenario: Next prompt injects review
- **WHEN** user sends their next message in the active session after submitting a review
- **THEN** the `inject-edits` hook SHALL append the serialized review to the prompt delivered to Claude
- **AND** `pending_annotations` SHALL be cleared after injection

#### Scenario: Discard review batch
- **WHEN** user clicks a "Discard review" action
- **THEN** all annotations in the current batch SHALL be removed without being sent

### Requirement: Large diff performance
The diff panel SHALL virtualize rendering for files with more than 500 lines so that keyboard navigation and focus movement remain responsive.

#### Scenario: Virtualized rendering on large diff
- **WHEN** a diff file contains more than 500 lines
- **THEN** only visible lines plus an overscan buffer SHALL be rendered in the DOM
- **AND** navigating via `j`/`k` SHALL scroll the focused line into view without frame drops
