# Implementation Plan: agent-spawned-worktrees

> Depends on `cluihud-mcp-server`; ships after `cross-session-messaging`. Grounded in `src-tauri/src/`; re-verify symbols before editing.

## Execution order

1. `create_worktree_session` tool + pending-request queue (request-only).
2. Human gate: Tauri approve/edit/deny commands + confirmation UI.
3. Approval flow: reuse `worktree.rs` creation + `LaunchOptions`, inject initial prompt, hand off.
4. Lifecycle alignment + no auto-delete.

## 1. Tool + pending queue — extend `src-tauri/src/mcp/`

- `create_worktree_session(workspace_id, prompt, branch_name?, agent?, launch_options?)`:
  - Validate `workspace_id` is an active/known workspace (else structured error).
  - Build a `PendingWorktreeRequest { id, requesting_session, workspace_id, branch_name, prompt, launch_options }`, push to an in-memory queue (`HashMap<RequestId, PendingWorktreeRequest>` in the daemon).
  - Return `{ pending_request_id }`. **Create nothing synchronously.**
  - Optional: per-requesting-session rate limit → `{ error: "too_many_pending_requests" }`.

## 2. Human gate — Tauri commands + UI

- Tauri commands in `commands.rs`: `list_worktree_requests()`, `approve_worktree_request(id, edited?)`, `deny_worktree_request(id)`.
- Emit a Tauri event `worktree:request` so the frontend surfaces a confirmation modal/toast showing: requesting session, target workspace, branch, prompt, launch options; actions Approve / Edit / Deny.
- **Auto-mode must NOT auto-approve** (resource/irreversible; CC v2.1.166 non-authority + RULES pause-on-ambiguity). The gate is a hard human step; no code path approves it programmatically. The calling agent's tool result resolves only after a human decision (or a timeout → treated as deny).

## 3. Approval flow — reuse existing machinery

On `approve_worktree_request`:
- Create the worktree via `worktree.rs:261` `create_worktree(repo_path, slug)` (slug from branch_name or generated — reuse existing slug logic with diacritics-strip + timestamp).
- Start the session through the normal spawn path (`pty.rs` `start_claude_session` / the adapter `SpawnSpec`), applying `LaunchOptions` (`models.rs:118`, `startup_command` at `:131`) so project presets/setup run — same path as a user-created worktree session.
- Inject the dedicated `prompt` as the session's first turn (the existing `pending_prompt`-at-spawn mechanism: `pty.rs:74` "session_id -> prompt to submit at spawn").
- Transfer control to the user: cluihud sends nothing further to the new session.
- Resolve the calling agent's tool result as `{ approved: true, new_session_id }`.

On `deny` → resolve `{ approved: false }`. On `edit` → apply user changes to the `PendingWorktreeRequest`, then the approve path.

## 4. Lifecycle alignment + no auto-delete

- Worktree under the existing `<repo>/.worktrees/` convention (already how `create_worktree` + slug work).
- Leave it unlocked when the agent finishes (align with CC v2.1.157 so `git worktree remove`/`prune` works — already the project's `remove_worktree` path at `worktree.rs:303`).
- cluihud SHALL NOT auto-delete an approved worktree. Confirm no idle-reaper or cleanup job removes user-owned worktree sessions.

## Per-phase risk

- **Phase 1 (tool)**: a bug that creates synchronously would bypass the gate — the whole security premise. Test first: assert nothing exists on the filesystem until approval.
- **Phase 2 (gate)**: an auto-mode path that auto-approves is a critical failure. Adversarial test: enable auto-mode, raise a request, assert the gate still blocks. Security reviewer required.
- **Phase 3 (reuse)**: drift from the user-created worktree path (different env, missing presets). Mitigate: route through the exact same spawn/launch-options code, not a parallel copy.

## Verification

`cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` · `npx tsc --noEmit` · manual: request → gate appears, nothing created pre-approval; approve → worktree session with dedicated prompt + presets, control is the user's; deny → nothing created, agent gets denied result; auto-mode does not bypass the gate (see proposal Build contract).
