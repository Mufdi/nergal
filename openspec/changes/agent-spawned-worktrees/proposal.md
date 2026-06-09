## Why

The cross-session workflow has a natural endpoint: A and B converse and reach consensus that some implementation work must happen, e.g. "the front needs change X". To avoid polluting the current session, the agent decides the right move is a fresh worktree session dedicated to that task. Today only the user can create a worktree session.

This change lets an agent *request* the creation of a worktree session under an active workspace, with a dedicated initial prompt — but always behind a mandatory human confirmation gate. After the user approves and the initial prompt is sent, control passes to the user, who owns that session thereafter. It is the most resource-sensitive capability of the set, so it ships last, on top of `cluihud-mcp-server` and after `cross-session-messaging` (whose `SessionDelivery` channel carries the outcome back to the requesting agent).

## What Changes

- New MCP tool `create_worktree_session(workspace_id, prompt, branch_name?, agent?, launch_options?)` that does NOT create immediately and **does not block**: it enqueues a request and returns `{ pending_request_id }` at once (an MCP call must not hang for human latency, which exceeds the agent tool timeout).
- **Async outcome**: the result reaches the agent via the `cross-session-messaging` `SessionDelivery` channel (push) and a `get_worktree_request_status(request_id)` tool (pull); `cancel_worktree_request(request_id)` withdraws a pending one.
- **Mandatory human gate, structurally un-bypassable**: a native cluihud confirmation UI (requesting session, workspace, branch, prompt, launch options, current worktree count + free disk; Approve / Edit / Deny). It is outside every agent permission system — CC's `--permission-mode`/bypass (`models.rs:104`) cannot reach a cluihud modal, and there is no programmatic approve path. Headless runs resolve only `denied`/`timed_out`.
- On **approve**: cluihud verifies the slug doesn't collide with an existing `.worktrees/cluihud/{slug}` (avoiding injection into a live worktree), creates via the existing machinery, applies the existing `LaunchOptions` (permission preset + short `startup_command` prelude — **not** a setup-runner; that "Workspace presets" item is unimplemented), submits the dedicated prompt as the first turn, then **hands control to the user**.
- **Timeout** atomically purges the request (no later-approval orphan), writing `timed_out` to a terminal-status ledger separate from the pending queue (so the agent can still poll the outcome). **Kill-switch** `agent_spawned_worktrees_enabled` (default off). In-memory queue + ledger: a daemon restart empties both, so in-flight requests are abandoned — no push is attempted and a poll returns `not_found`.
- **Lifecycle**: `.worktrees/cluihud/{slug}` removable via cluihud's own `remove_worktree`; no auto-delete; no reliance on CC v2.1.157 (which governs CC's own `/worktree`).

## Capabilities

### New Capabilities
- `agent-spawned-worktrees`: The `create_worktree_session` / `get_worktree_request_status` / `cancel_worktree_request` tools, the non-blocking request + async outcome delivery, the structurally un-bypassable human gate, the collision-checked approval flow reusing existing `LaunchOptions` (no setup-runner), hand-off to the user, timeout/kill-switch/volatile-queue semantics, and lifecycle via cluihud's own worktree tooling.

### Modified Capabilities
<!-- Builds on cluihud-mcp-server (MCP) + cross-session-messaging (SessionDelivery). Reuses existing worktree creation + LaunchOptions. No existing spec-level behavior changes. -->

## Impact

- **Backend**: extend `src-tauri/src/mcp/` with the request/status/cancel tools + pending queue + timeout sweep; Tauri commands for list/approve/edit/deny; approval wires into `create_worktree` (`worktree.rs:261`) + `LaunchOptions` + `pending_prompts` (`pty.rs:74`); outcome via `cross-session-messaging` `SessionDelivery`.
- **Frontend**: native confirmation UI handling multiple concurrent requests, showing worktree count + free disk.
- **File system**: worktrees under the existing `.worktrees/cluihud/` convention.
- **Existing flows**: reuses worktree creation + launch options + pending-prompt; adds a gated agent-initiated entry point.

## Build contract

### Qué construyo
- MCP tools `create_worktree_session` (non-blocking, returns `pending_request_id`), `get_worktree_request_status`, `cancel_worktree_request`.
- Native human gate UI (Approve/Edit/Deny), structurally outside any agent permission path, multi-request, showing count + free disk.
- Approval flow: slug-collision check → `create_worktree` → existing `LaunchOptions` → dedicated first prompt via `pending_prompts` → hand off. NO setup-runner.
- Async outcome delivery via `cross-session-messaging` `SessionDelivery` + poll fallback.
- Timeout (atomic purge), kill-switch (default off), volatile-queue restart behavior, create-failure → `failed{reason}`.
- Lifecycle via cluihud's own `remove_worktree`; no auto-delete.

### Cómo verifico
- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Automated: nothing on FS pre-approval; auto-mode cannot reach the gate (structural); slug-collision refusal; timeout atomically purges + ledger returns `timed_out` then `not_found` after TTL; two concurrent requests; create-failure → `failed`; spawn-failure rolls back the worktree; daemon-restart → poll `not_found` (no push); cancel.
- Manual: agent requests → gate shows count/disk, nothing created; approve unique slug → session with dedicated prompt + launch options, control is the user's, outcome delivered; deny → `denied` delivered; collision refused.

### Criterio de done
- The tool returns immediately; no worktree is created without explicit human approval, including under any bypass/auto permission mode (structurally).
- The agent learns the outcome via delivery or poll; a timeout atomically purges so no orphan session is created by a late approval.
- On approval (unique slug), a session starts with the dedicated prompt + existing `LaunchOptions`, then control passes to the user; a slug collision is refused, never injected into a live worktree.
- The worktree follows `.worktrees/cluihud/` and is removable via cluihud tooling; cluihud never auto-deletes it.

### Estimated scope
- files_estimate: 12
- risk_tier: critical
- tags: [feature, security]
- visibility: public
- spec_target: agent-spawned-worktrees
