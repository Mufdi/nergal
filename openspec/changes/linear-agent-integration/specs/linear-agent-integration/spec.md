# linear-agent-integration

## ADDED Requirements

### Requirement: Compose an issue into an agent-ready context block

The system SHALL compose a Linear issue into a single labeled markdown block read from the local mirror (never a live API call): identifier, title, workflow state, url, markdown description, priority (rendered as a label, not the raw integer), estimate and assignee when present, labels, sub-issues (identifier + title + state), and comments (author + body) when the mirror holds them. Because Linear's model has no checklists and no custom fields, those sections SHALL NOT appear. Because attachments and issue relations are not mirrored (they are fetched live for the detail view), they SHALL be excluded from the composed block (the mirror-only contract forbids a live call at compose time). The composed context SHALL be capped at a byte budget; on overflow the system SHALL drop oldest comments first, then collapse the sub-issue list to a count, then head/tail-truncate the description, appending a visible marker at each step and never dropping the identifier/title/state/url heading.

#### Scenario: Issue composed from the mirror

- **WHEN** the system composes an issue that has a description, priority, assignee, labels, and sub-issues
- **THEN** the block SHALL include each of those sections read from the mirror
- **AND** SHALL render priority as a label using the panel's exact vocabulary (No priority / urgent / high / normal / low — matching `linearPriorityStr`), not the raw integer
- **AND** SHALL render a comment author by parsing `linear_comments.user_json` (display name, falling back to "Unknown" on null/unparseable JSON)
- **AND** SHALL NOT issue a live Linear API call to compose
- **AND** SHALL NOT include attachments or issue relations (not mirrored)

> Note: change #1's poller does not populate `linear_comments`, so the comment
> section is exercised by a seeded unit-test fixture (a directly-inserted row
> with a realistic `user_json`), not by live-mirror data — the rendering and
> attrition contract is verified, while the live data path activates for free
> when a later change syncs comments.

#### Scenario: Oversize composition truncates visibly

- **WHEN** a composed issue exceeds the byte budget
- **THEN** the system SHALL drop oldest comments first, then collapse sub-issues to a count, then head/tail-truncate the description
- **AND** append a visible marker at each applied step
- **AND** never drop the identifier/title/state/url heading

### Requirement: Three issue-to-agent verbs

The system SHALL provide three distinct actions on a selected issue: (1) send-as-prompt — after explicit user confirmation, submit the composed block as a turn to the active session via a bracketed-paste write plus a submit (so a multi-line body lands as one turn, not fragmented); (2) spawn-worktree-with-issue — create a new worktree session via the existing user-initiated worktree machinery with the composed block as the initial prompt, binding the new session to the issue; (3) attach-as-context — fold the composed block into the session's injected context. Send-as-prompt SHALL NOT establish a persistent binding on the active session. Send-as-prompt SHALL deliver immediately regardless of the agent's run state — a send into a mid-turn agent rides the agent's own prompt queueing (Claude Code queues natively); no send-gate is built (matching the ClickUp Revision-3 decision).

#### Scenario: Send as prompt to the active session

- **WHEN** the user sends an issue as a prompt to the active session
- **THEN** the system SHALL show a confirmation with the composed block before submitting
- **AND** on confirm SHALL deliver it via a bracketed-paste write so the multi-line body is one paste, followed by a submit
- **AND** no persistent binding SHALL be created by this action
- **AND** delivery SHALL NOT be gated on the agent's run state

#### Scenario: Spawn a worktree session with the issue

- **WHEN** the user spawns a worktree session with an issue
- **THEN** the system SHALL create the worktree via the existing user-initiated machinery
- **AND** derive the worktree slug from the issue title
- **AND** set the composed block as the new session's initial prompt
- **AND** bind the new session to that issue as its active issue
- **AND** the initial prompt SHALL submit on spawn without a REPL timing race

### Requirement: Attach an issue as injected context, rejoining the existing injection contract

The system SHALL fold attached-issue context into the session's `injected_context` through the existing assembler and adapter contract — never by typing into the PTY. The assembler SHALL run on both fresh spawn and resume so attached-issue context is re-injected automatically on resume, reflecting current mirror content. The Linear block SHALL be concatenated as an additional labeled source alongside any vault-note and ClickUp blocks, preserving the `None`-when-empty behavior so a session with no pins/bindings of any source spawns byte-identically. When an adapter's injection tier is `Unsupported`, the system SHALL record the attachment but SHALL NOT inject and SHALL indicate this in the session UI, with no PTY-typed fallback.

#### Scenario: Attached issue injected via the adapter contract

- **WHEN** a session with an attached issue spawns
- **THEN** the composed block SHALL be folded into `injected_context` alongside any vault-note and ClickUp context
- **AND** SHALL reach the agent through the adapter's injection tier, not via PTY typing

#### Scenario: Resume re-injects current attached-issue context

- **WHEN** a session with an attached issue is resumed after the issue changed in the mirror
- **THEN** the system SHALL re-compose from the mirror and inject the current content

#### Scenario: Unsupported agent records but does not inject

- **WHEN** a session whose adapter is `Unsupported` has an attached issue
- **THEN** spawning SHALL NOT inject the issue context
- **AND** the session UI SHALL indicate injection is unsupported
- **AND** no context SHALL be typed into the PTY

### Requirement: One active issue per session plus pinned context issues

The system SHALL persist at most one active Linear issue per session (`active_linear_issue_id`) and zero or more pinned context issues (`pinned_linear_issue_ids`, an ordered idempotent JSON array). Both active and pinned issues SHALL be injected (deduped by id); only the active issue SHALL be the write-back subject. The session tab SHALL show the active issue. Live delivery on top of the spawn/resume seeding (best-effort, no-op without a live PTY): binding SHALL deliver the composed block to the running agent SUBMITTED as a turn (the deliberate "work on this" act); pinning SHALL paste it WITHOUT submitting (passive context). Binding an issue while one is already active SHALL replace it, with UI confirmation; the replaced issue SHALL remain in the mirror. Unbind and unpin SHALL affect only future spawns and resumes — they SHALL NOT retract context already present in a running agent's window.

#### Scenario: Active issue is unique and surfaced

- **WHEN** the user binds an issue to a session that has no active issue
- **THEN** `active_linear_issue_id` SHALL be set
- **AND** the session tab SHALL indicate the bound issue
- **AND** the composed block SHALL be delivered to the live agent immediately, submitted as a turn

#### Scenario: Rebinding replaces with confirmation

- **WHEN** the user binds an issue to a session that already has an active issue
- **THEN** the UI SHALL confirm the replacement
- **AND** on confirmation the previous active issue SHALL be replaced
- **AND** SHALL remain present in the mirror

#### Scenario: Pinned issues are context-only

- **WHEN** a session has pinned issues that are not the active issue
- **THEN** they SHALL be injected as context
- **AND** SHALL NOT be the write-back subject

#### Scenario: Active issue in the pinned set is deduped

- **WHEN** the active issue id also appears in the pinned array
- **THEN** the injected context SHALL include the issue once

#### Scenario: Unbind affects only future spawns

- **WHEN** the user unbinds (or unpins) an issue from a running session
- **THEN** the change SHALL apply to future spawns and resumes
- **AND** SHALL NOT retract context already present in the running agent's window

### Requirement: Composed Linear context is labeled and reviewed before submission

The system SHALL wrap the composed block in a labeled fence identifying it as the session's team-authored Linear issue brief, for both attach and send-as-prompt. Terminal control sequences SHALL be neutralized on every PTY delivery path (`sanitize_for_pty`), and a fence-sentinel appearing in the issue content SHALL be neutralized so it cannot close the labeled block early. Because send-as-prompt auto-submits the content as a turn, the system SHALL require explicit user confirmation showing the composed block before submitting it — a review step, not a distrust warning; attach (passive context) SHALL NOT require a per-injection confirmation.

#### Scenario: Composed block is labeled as the issue brief

- **WHEN** an issue is composed for injection or sending
- **THEN** the block SHALL be wrapped in a labeled fence identifying it as the team-authored Linear issue brief
- **AND** terminal control sequences and fence-sentinel collisions in the content SHALL be neutralized

#### Scenario: Send-as-prompt requires confirmation

- **WHEN** the user triggers send-as-prompt
- **THEN** the system SHALL require explicit confirmation showing the composed block before submitting
- **AND** SHALL NOT auto-submit content the user has not seen
