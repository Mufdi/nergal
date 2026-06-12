# clickup-agent-integration Specification

## Purpose
TBD - created by archiving change clickup-task-integration. Update Purpose after archive.
## Requirements
### Requirement: Compose a task into an agent-ready context block

The system SHALL compose a ClickUp task into a single labeled markdown block read from the local mirror (never a live API call): name, status, url, markdown description, subtasks (name + status), checklists (items + resolved), custom fields rendered by type (computed fields such as `automatic_progress` shown read-only), recent comments (author + text), and attachments as title+url links (never inlined binaries). The composed context SHALL be capped at a byte budget; on overflow the system SHALL drop oldest comments first, append a visible truncation marker, and log the omission.

#### Scenario: Task composed from the mirror

- **WHEN** the system composes a task that has subtasks, checklists, comments, custom fields, and attachments
- **THEN** the block SHALL include each of those sections read from the mirror
- **AND** SHALL NOT issue a live ClickUp API call to compose
- **AND** attachments SHALL appear as title+url links, not inlined binaries

#### Scenario: Oversize composition truncates visibly

- **WHEN** a composed task exceeds the byte budget
- **THEN** the system SHALL drop oldest comments first
- **AND** append a visible truncation marker
- **AND** log what was omitted

### Requirement: Three task-to-agent verbs

The system SHALL provide three distinct actions on a selected task: (1) send-as-prompt — after explicit user confirmation, submit the composed block as a turn to the active session via a bracketed-paste write plus a submit (so a multi-line body lands as one turn, not fragmented), or as the initial prompt to a new session; (2) spawn-worktree-with-task — create a new worktree session via the existing user-initiated worktree machinery with the composed block as the initial prompt, binding the new session to the task; (3) attach-as-context — fold the composed block into the session's injected context. Send-as-prompt SHALL NOT establish a persistent binding on the active session. Send-as-prompt SHALL deliver immediately regardless of the agent's run state — a send into a mid-turn agent rides the agent's own prompt queueing (Claude Code queues natively). (Design Revision 3, user decision 2026-06-11: the hook-based send-gate of Revision 1 was removed — it duplicated the agent's native queueing, required a hook in the user's global settings plus a session restart, and only worked for Claude Code.)

#### Scenario: Send as prompt to the active session

- **WHEN** the user sends a task as a prompt to the active session
- **THEN** the system SHALL show a confirmation with the composed block before submitting
- **AND** on confirm SHALL deliver it via a bracketed-paste write so the multi-line body is one paste, followed by a submit
- **AND** no persistent binding SHALL be created by this action
- **AND** delivery SHALL NOT be gated on the agent's run state

#### Scenario: Spawn a worktree session with the task

- **WHEN** the user spawns a worktree session with a task
- **THEN** the system SHALL create the worktree via the existing user-initiated machinery
- **AND** set the composed block as the new session's initial prompt
- **AND** bind the new session to that task as its active task
- **AND** the initial prompt SHALL submit on spawn without a REPL timing race

### Requirement: Attach a task as injected context, rejoining the existing injection contract

The system SHALL fold attached-task context into the session's `injected_context` through the existing assembler and adapter contract — never by typing into the PTY. The assembler SHALL run on both fresh spawn and resume so attached-task context is re-injected automatically on resume, reflecting current mirror content. When an adapter's injection tier is `Unsupported`, the system SHALL record the attachment but SHALL NOT inject and SHALL indicate this in the session UI, with no PTY-typed fallback.

#### Scenario: Attached task injected via the adapter contract

- **WHEN** a session with an attached task spawns
- **THEN** the composed block SHALL be folded into `injected_context` alongside any vault-note context
- **AND** SHALL reach the agent through the adapter's injection tier, not via PTY typing

#### Scenario: Resume re-injects current attached-task context

- **WHEN** a session with an attached task is resumed after the task changed in the mirror
- **THEN** the system SHALL re-compose from the mirror and inject the current content

#### Scenario: Unsupported agent records but does not inject

- **WHEN** a session whose adapter is `Unsupported` has an attached task
- **THEN** spawning SHALL NOT inject the task context
- **AND** the session UI SHALL indicate injection is unsupported
- **AND** no context SHALL be typed into the PTY

### Requirement: One active task per session plus pinned context tasks

The system SHALL persist at most one active ClickUp task per session (`active_clickup_task_id`) and zero or more pinned context tasks (`pinned_clickup_task_ids`, an ordered idempotent JSON array). Both active and pinned tasks SHALL be injected (deduped by id); only the active task SHALL be the write-back subject. The session tab SHALL show the active task by name and the pinned tasks as a count indicator. Live delivery on top of the spawn/resume seeding (best-effort, no-op without a live PTY): binding SHALL deliver the composed block to the running agent SUBMITTED as a turn (the deliberate "work on this" act); pinning SHALL paste it WITHOUT submitting (passive context). Binding a task while one is already active SHALL replace it, with UI confirmation; the replaced task SHALL remain in the mirror.

#### Scenario: Active task is unique and surfaced

- **WHEN** the user binds a task to a session that has no active task
- **THEN** `active_clickup_task_id` SHALL be set
- **AND** the session tab SHALL indicate the bound task
- **AND** the composed block SHALL be delivered to the live agent immediately, submitted as a turn

#### Scenario: Rebinding replaces with confirmation

- **WHEN** the user binds a task to a session that already has an active task
- **THEN** the UI SHALL confirm the replacement
- **AND** on confirmation the previous active task SHALL be replaced
- **AND** SHALL remain present in the mirror

#### Scenario: Pinned tasks are context-only

- **WHEN** a session has pinned tasks that are not the active task
- **THEN** they SHALL be injected as context
- **AND** SHALL NOT be the write-back subject

#### Scenario: Active task in the pinned set is deduped

- **WHEN** the active task id also appears in the pinned array
- **THEN** the injected context SHALL include the task once

#### Scenario: Unbind affects only future spawns

- **WHEN** the user unbinds (or unpins) a task from a running session
- **THEN** the change SHALL apply to future spawns and resumes
- **AND** SHALL NOT retract context already present in the running agent's window

### Requirement: Composed ClickUp context is labeled and reviewed before submission

The system SHALL wrap the composed block in a labeled fence identifying it as the session's team-authored ClickUp task brief, for both attach and send-as-prompt. (Per the user decision of 2026-06-11, the workspace is a trusted team and tasks may carry direct instructions — the original "untrusted data, not instructions" framing was dropped; terminal-level sanitization of control sequences and fence-sentinel neutralization are retained as technical protections.) Because send-as-prompt auto-submits the content as a turn, the system SHALL require explicit user confirmation showing the composed block before submitting it — a review step, not a distrust warning; attach (passive context) SHALL NOT require a per-injection confirmation.

#### Scenario: Composed block is labeled as the task brief

- **WHEN** a task is composed for injection or sending
- **THEN** the block SHALL be wrapped in a labeled fence identifying it as the team-authored ClickUp task brief
- **AND** terminal control sequences and fence-sentinel collisions in the content SHALL still be neutralized

#### Scenario: Send-as-prompt requires confirmation

- **WHEN** the user triggers send-as-prompt
- **THEN** the system SHALL require explicit confirmation showing the composed block before submitting
- **AND** SHALL NOT auto-submit content the user has not seen

