## ADDED Requirements

### Requirement: create_worktree_session tool requests, never creates, and never blocks

The system SHALL expose a `create_worktree_session(workspace_id, prompt, branch_name?, agent?, launch_options?)` MCP tool that enqueues a pending creation request and **immediately** returns `{ pending_request_id }` without blocking. It SHALL NOT create a worktree or session synchronously, and SHALL NOT block the tool call waiting for the human decision (which can exceed the agent's tool-call timeout). The tool SHALL be gated by `agent_spawned_worktrees_enabled` (default off).

#### Scenario: Tool raises a request and returns immediately

- **WHEN** an agent calls `create_worktree_session` with a valid active workspace and a prompt
- **THEN** the system SHALL enqueue a pending request and return `{ pending_request_id }` immediately, creating nothing yet

#### Scenario: Disabled by setting

- **WHEN** `agent_spawned_worktrees_enabled` is off
- **THEN** the tool SHALL return a structured "disabled" error and enqueue nothing

#### Scenario: Invalid target workspace

- **WHEN** the workspace is not active/known (cannot resolve to a repo path)
- **THEN** the tool SHALL return a structured error and enqueue nothing

#### Scenario: Too many pending requests

- **WHEN** a session already has the maximum pending requests
- **THEN** the tool SHALL return `too_many_pending_requests` and enqueue nothing

### Requirement: Outcome delivered asynchronously; status pollable via a terminal ledger; request cancellable

The system SHALL deliver the request outcome to the requesting session via the `cross-session-messaging` `SessionDelivery` channel when the human decides. It SHALL maintain a **terminal-status ledger** separate from the pending queue: when a pending entry is removed (approve/deny/timeout/cancel/failure) the terminal state SHALL be written to the ledger at the same instant, with a retention TTL. `get_worktree_request_status(request_id)` SHALL check the pending queue then the ledger and return `pending | approved{session_id} | denied | timed_out | cancelled | failed{reason}`, or `not_found` for an unknown/expired id. `cancel_worktree_request(request_id)` SHALL withdraw a pending request.

#### Scenario: Approval is delivered and pollable after purge

- **WHEN** the user approves and a session is created (the pending entry is removed)
- **THEN** the requesting session SHALL receive `approved{session_id}` via `SessionDelivery`, and `get_worktree_request_status` SHALL still return `approved{session_id}` from the ledger

#### Scenario: Terminal status survives the queue purge

- **WHEN** a request is resolved (e.g. `timed_out`) and its pending entry removed
- **THEN** `get_worktree_request_status` SHALL return that terminal state from the ledger until the TTL expires, after which it returns `not_found`

#### Scenario: Agent cancels a pending request

- **WHEN** an agent calls `cancel_worktree_request` for a pending request
- **THEN** the request SHALL be removed from the queue, written `cancelled` to the ledger, and SHALL NOT be approvable afterward

### Requirement: Mandatory human gate, structurally un-bypassable

The system SHALL surface a native confirmation UI for each pending request, showing the requesting session, target workspace, branch, prompt, the current worktree count + free disk, and — **broken out explicitly** — the requested `agent` CLI and `permission_preset` (not buried in a generic launch-options blob, since both are agent-chosen escalation inputs), with Approve / Edit / Deny. The system MAY clamp which presets an agent can request. The sole approval entry point SHALL be the `approve_worktree_request` Tauri command (GUI-only); no MCP/permission-mode handler SHALL be able to invoke it. The gate SHALL be un-bypassable by construction: it lives outside every agent's permission system. In a headless run with no GUI, a request SHALL only resolve as `denied`/`timed_out`.

#### Scenario: Nothing created before approval

- **WHEN** a creation request is pending
- **THEN** no worktree and no session SHALL exist until the user approves

#### Scenario: Auto-mode cannot bypass the gate

- **WHEN** an agent runs under any bypass/auto permission mode and raises a request
- **THEN** the gate SHALL still require explicit human action — no agent permission mode can reach the cluihud gate, by construction

#### Scenario: User edits before approving

- **WHEN** the user chooses Edit
- **THEN** the user SHALL be able to adjust the prompt and/or branch before approving

#### Scenario: User denies

- **WHEN** the user chooses Deny
- **THEN** nothing SHALL be created and the requester SHALL receive a `denied` outcome

#### Scenario: Headless run

- **WHEN** there is no GUI to present the gate
- **THEN** the request SHALL resolve as `denied` or `timed_out`, never silently hang

### Requirement: Timeout atomically purges the request

A pending request SHALL have a configurable timeout (default ~1 hour, with an upper bound, e.g. 24 h). On timeout the system SHALL **atomically** remove the queue entry and write `timed_out` to the terminal ledger, so a later human action cannot create an orphaned session for a request the agent already abandoned.

#### Scenario: Timeout removes the entry and records terminal status

- **WHEN** a pending request reaches its timeout
- **THEN** the system SHALL atomically remove it from the queue, write `timed_out` to the ledger, and it SHALL NOT be approvable afterward

### Requirement: Approval reuses existing creation with a uniqueness check; no setup-runner

On approval, the system SHALL resolve `workspace_id → repo_path`, verify the target slug does NOT collide with an existing `.worktrees/cluihud/{slug}`, create the worktree via the existing machinery, apply the existing `LaunchOptions` (permission preset + short `startup_command` prelude), submit the dedicated prompt as the first turn via `pending_prompts`, then transfer control to the user. The system SHALL NOT run a project setup-runner (none exists; "Workspace presets" is a separate unimplemented backlog item) and SHALL NOT claim to.

#### Scenario: Approved spawn runs the dedicated prompt

- **WHEN** the user approves a request whose slug is unique
- **THEN** a new worktree session SHALL start under the target workspace with the existing `LaunchOptions` applied, and the dedicated prompt SHALL be delivered as its first turn

#### Scenario: Slug collision is refused, never injected into a live worktree

- **WHEN** the target slug collides with an existing `.worktrees/cluihud/{slug}`
- **THEN** the system SHALL refuse the create (surfacing it in the gate / requiring a branch edit) and SHALL NOT inject the prompt into or spawn a second PTY on the existing worktree

#### Scenario: Control passes to the user

- **WHEN** the dedicated initial prompt has been delivered
- **THEN** control of the new session SHALL belong to the user and cluihud SHALL NOT drive it further

#### Scenario: Create failure resolves as failed

- **WHEN** `create_worktree` fails during approval (e.g. disk full)
- **THEN** the request SHALL resolve `failed{reason}` and that outcome SHALL be deliverable/pollable

#### Scenario: Spawn failure after create rolls back the worktree

- **WHEN** `create_worktree` succeeds but the subsequent PTY/session spawn fails
- **THEN** the system SHALL roll back the just-created worktree via `remove_worktree` before resolving `failed{reason}`, leaving no orphan worktree on disk (if rollback itself fails, the orphan path SHALL be named in the reason)

### Requirement: Worktree lifecycle via cluihud tooling, no auto-delete

The created worktree SHALL use the existing `.worktrees/cluihud/{slug}` convention and be removable via cluihud's own `remove_worktree`. The system SHALL NOT auto-delete an approved worktree; the user owns its removal. The system SHALL NOT rely on CC's own `/worktree` unlock-on-finish behavior for externally-created worktrees.

#### Scenario: User owns cleanup

- **WHEN** an approved worktree session is later idle or finished
- **THEN** the system SHALL NOT auto-delete it, and it SHALL remain removable by the user via cluihud's worktree tooling

### Requirement: Kill-switch and volatile-queue behavior

The tool SHALL be gated by `agent_spawned_worktrees_enabled` (default off). The pending queue and terminal ledger are in-memory and SHALL NOT be persisted (a stale approval after restart could create an orphan). On daemon restart both are empty: the daemon cannot push-notify (it no longer knows which sessions had requests), and a poll returns `not_found` — the agent treats the request as abandoned.

#### Scenario: Disabled tool

- **WHEN** the feature is disabled
- **THEN** `create_worktree_session` SHALL return a structured "disabled" error

#### Scenario: Daemon restart abandons in-flight requests

- **WHEN** the daemon restarts with pending requests
- **THEN** those requests SHALL be dropped, no push notification SHALL be attempted, and a subsequent `get_worktree_request_status` SHALL return `not_found` (the agent treats it as abandoned)

### Requirement: request_session_resume revives an existing inactive session through the same gate

The system SHALL expose a `request_session_resume(session_id, message?)` MCP tool that requests reviving an existing, currently-inactive session (the natural complement to `send_to_session` refusing an inactive target: that session exists and was worked before but is not live now). It SHALL go through the SAME human gate as `create_worktree_session`, be gated by `agent_spawned_worktrees_enabled`, and be non-blocking (return `{ pending_request_id }`). It SHALL validate that the session exists (`unknown_session` otherwise) and is not already live (`already_live`, pointing to `send_to_session`). A resume request SHALL carry NO agent-chosen escalation inputs — it reuses the target session's own launch options. On approval the system SHALL resume the session in its existing worktree ("continue" mode, no new worktree created) and, if a `message` was given, deliver it to the revived session as a labeled, advisory relayed first turn. To avoid pasting into a not-yet-ready prompt, the target SHALL be marked busy the instant it is approved so delivery to it queues until its first idle.

#### Scenario: Resume requests through the gate, never creates directly

- **WHEN** an agent calls `request_session_resume` for an existing inactive session
- **THEN** the system SHALL enqueue a pending resume request behind the human gate and return `{ pending_request_id }`, reviving nothing until the user approves

#### Scenario: Unknown or already-live target

- **WHEN** the target session does not exist, OR is already live
- **THEN** the tool SHALL return `unknown_session` (does not exist) or `already_live` (use `send_to_session` instead), enqueuing nothing

#### Scenario: Approved resume revives in place and delivers the message

- **WHEN** the user approves a resume whose target still exists and is still not live
- **THEN** the session SHALL be resumed in its existing worktree (no new worktree), and any provided message SHALL be delivered to it as a labeled advisory relayed turn

#### Scenario: No new escalation surface

- **WHEN** the gate shows a resume request
- **THEN** it SHALL NOT present create-only escalation inputs (branch, agent, permission preset, startup_command) — a resume reuses the target's own launch options
