## MODIFIED Requirements

### Requirement: Send annotations to Claude via chat injection

The `SpecPanel` SHALL provide a "Send to Agent" action in the annotation toolbar that serializes all annotations for the currently viewed spec (or other artifact) and queues them into `HookState.pending_annotations` via the `set_pending_annotations` Tauri command. The serialized payload SHALL include the artifact's repo-relative path so Claude can read the file directly. Delivery SHALL happen via the existing `inject-edits` (`UserPromptSubmit`) hook — NOT via the plan-review FIFO.

#### Scenario: Send to Claude queues feedback

- **WHEN** user has one or more annotations on a spec and clicks "Send to Agent" (or presses `Ctrl+Shift+R`)
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
