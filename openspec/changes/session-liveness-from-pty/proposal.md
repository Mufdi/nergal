Linear: FRI-601

## Why

The MCP tools report a closed session as live. Liveness reads the adapter cache, and only a session delete clears that cache. The user closed a session's tab, and `request_session_resume` still answered `already_live` until Nergal restarted. `send_to_session` accepts messages for the same dead session, and nobody reads them.

## What Changes

- A dedicated live set in the agent runtime state decides liveness. It maps each live session to the id of its agent PTY.
- Spawning the agent PTY marks the session live. Killing the PTY, the PTY exiting on its own, and deleting the session mark it not live.
- The PTY exit clears liveness only when its PTY id still matches. A late exit of a replaced PTY leaves the new one live.
- Every live-set change made with a reservation change (reserve, release, kill) happens under the same lock, so a concurrent kill and start keep a consistent order. The PTY exit and the session delete change only the live set, with a compare-and-remove or a plain remove.
- Hook events and adapter-cache registration never mark a session live.
- Approving a resume fails with a clear reason when the session's agent PTY exited but its tab is still open. Closing the tab first makes the resume work.
- `list_sessions`, `get_session`, `send_to_session`, `request_session_resume`, the resume approval and the MCP caller identity read the live set. A killed session's id no longer identifies a caller.
- The `inactive_target` hint points to `request_session_resume`, the tool that revives a session.
- A not-live descriptor shows the persisted mode and empty live-activity fields, as the `session-directory` spec already requires.
- Visible change: between a resume approval and the PTY reservation, `send_to_session` to that session returns `inactive_target` instead of queuing. The window is short, because the frontend reserves the PTY right after the approval.
- Visible change: a session killed before its first `Stop` has no transcript marker, so it leaves `list_sessions` and `get_session` answers not found. Before, the cache kept listing it as live. `request_session_resume` still accepts its id.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `session-directory`: adds a requirement that defines a live session as one with a running agent PTY, with scenarios for a closed tab, a self-exited PTY, a late hook event and a late exit of a replaced PTY. `cross-session-messaging` already refers to "live" and inherits the definition without a requirement change.
- `agent-spawned-worktrees`: adds a requirement that the resume approval refuses a session whose terminal is still open, and modifies `request_session_resume` so delivery queues from the PTY reservation, not from the approval.
- `nergal-mcp-server`: modifies the cooperative identity requirement so the live registry is the live set, with a scenario for a killed session's id.

## Impact

- `src-tauri/src/agents/state.rs`: new live set and its accessors; `live_session_ids` reads it; `forget_session` clears it.
- `src-tauri/src/pty.rs`: `reserve_session_pty` marks live; `release_session_pty`, `kill_session_pty` and the agent PTY exit mark not live; new `PtyManager::has_session_pty`.
- `src-tauri/src/mcp/worktree_sessions.rs`: the resume approval checks `PtyManager::resume_block_state`; the warmup comment is updated.
- `src-tauri/src/agents/state.rs`: `resolve_session_hint` resolves only live sessions.
- `src-tauri/src/mcp/mod.rs`, `mcp/messaging.rs`: MCP instructions text for not-live descriptors and the `inactive_target` hint.
- `src-tauri/src/mcp/directory.rs`: `descriptor_from` drops live-activity fields for a not-live session; test helpers mark sessions live through the live set.
- `src-tauri/src/mcp/messaging.rs`, `mcp/worktree_sessions.rs`: test helpers mark sessions live through the live set.
- No schema change, no frontend change, no new dependency.
