## 1. Session-Scoped HookState Migration

- [ ] 1.1 Refactor `HookState::state_path()` in `src-tauri/src/hooks/state.rs` to accept a `session_id: &str` parameter and return `~/.claude/cluihud-state-{session_id}.json`. Add fallback: if session-scoped file doesn't exist, try global `cluihud-state.json`.
- [ ] 1.2 Update `inject_edits()` in `src-tauri/src/hooks/cli.rs` to read `CLUIHUD_SESSION_ID` from env and pass it to `HookState::state_path()`.
- [ ] 1.3 Update all other `HookState` callers (plan edit writer, annotation writer in Tauri commands) to pass the session ID when writing state.
- [ ] 1.4 Add `pending_context` field to `HookState`: `Option<ContextInjection>` with struct `{ from_session: String, summary: String, timestamp: u64 }`.
- [ ] 1.5 Extend `inject_edits()` to check `pending_context` after plan edits and annotations. Format with source attribution: `[Shared context from session "{name}" ({time} ago)]:\n{text}`. Consume via take pattern.

## 2. Quick Share Backend

- [ ] 2.1 Add Tauri command `quick_share(from_session_name: String, to_session_id: String, text: String)` that writes a `ContextInjection` to the target session's HookState `pending_context` field.
- [ ] 2.2 Add Tauri command `get_pending_context(session_id: String) -> Option<ContextInjection>` for the frontend to check if a session has pending context (used for badge display).

## 3. Quick Share Frontend

- [ ] 3.1 Create `src/stores/quickShare.ts` with atoms: `quickShareOpenAtom` (boolean), `pendingContextMapAtom` (Record<string, ContextInjection>). Listen for a Tauri event `context:injected` to clear the pending badge.
- [ ] 3.2 Create Quick Share composer component: text area + session picker (active sessions, excluding current). Triggered by `Ctrl+Shift+S`. Calls `quick_share` Tauri command on confirm.
- [ ] 3.3 Register `Ctrl+Shift+S` shortcut in `src/lib/shortcuts.ts` (verify no collision).
- [ ] 3.4 Add pending-context badge to `SessionRow.tsx`: small indicator when `pendingContextMapAtom[session.id]` is set. Badge disappears when context is consumed.
- [ ] 3.5 Emit `context:injected` Tauri event from backend when `inject_edits()` consumes a `pending_context`, so frontend can clear the badge.

## 4. Channel Backend — File Management

- [ ] 4.1 Create `src-tauri/src/channels/mod.rs` module with `Channel` struct: `{ id: String, topic: String, participants: [SessionId; 2], status: ChannelStatus, file_path: PathBuf, created_at: u64 }`. `ChannelStatus` enum: `Active`, `Closed`.
- [ ] 4.2 Add Tauri command `create_channel(session_a_id, session_b_id, topic) -> Channel` that generates a channel ID, creates the `.claude/crossmsg-{id}.md` file with header (topic, participants, timestamp), and returns the channel struct.
- [ ] 4.3 Add Tauri command `close_channel(channel_id)` that marks channel as `Closed`. Does not delete the file.
- [ ] 4.4 Add Tauri command `list_channels() -> Vec<Channel>` to return all active channels.

## 5. Channel Backend — File Watcher & Message Detection

- [ ] 5.1 Extend the existing `notify` file watcher setup to also watch `.claude/crossmsg-*.md` files. On modify event, read the file and parse the last message block.
- [ ] 5.2 Implement message parser: extract last `## {name} — {timestamp}` header, determine sender session, detect `**CAMBIO**` vs `**CONSENSO**` terminator.
- [ ] 5.3 On CAMBIO detection: identify target session (the other participant), emit a Tauri event `channel:message` with `{ channel_id, sender_session_id, target_session_id }`.
- [ ] 5.4 On CONSENSO detection: mark channel as `Closed`, emit Tauri event `channel:closed` with `{ channel_id }`.

## 6. Channel Backend — PTY Injection & Queue

- [ ] 6.1 Add PTY stdin writer function: given a session ID, write bytes to that session's PTY master fd. The function MUST verify the session exists and is owned by cluihud.
- [ ] 6.2 Implement state-aware injection: on `channel:message` event, check `modeMapAtom` equivalent in backend (or query frontend via Tauri event). If idle, inject prompt to PTY stdin. If not idle, queue.
- [ ] 6.3 Implement injection queue: `HashMap<SessionId, PendingInjection>` in app state. On `Stop` hook event for a session, check queue and inject if pending. Replace strategy: new message overwrites queued one.
- [ ] 6.4 Implement channel timeout: spawn a tokio task that checks for stale channels (no new message within 5min after CAMBIO). Emit Tauri event `channel:timeout` with `{ channel_id, stale_session_name }`.

## 7. Channel Frontend

- [ ] 7.1 Create `src/stores/channels.ts` with atoms: `channelsAtom` (list of active channels), `channelQueueAtom` (pending injections per session). Listen to `channel:message`, `channel:closed`, `channel:timeout` Tauri events.
- [ ] 7.2 Add "New Channel" entry to command palette (`Ctrl+K`): shows session picker for two participants + topic input. Calls `create_channel` Tauri command.
- [ ] 7.3 Add "Start Channel" to SessionRow context menu (right-click): creates channel between active session and right-clicked session.
- [ ] 7.4 Add channel indicator to `SessionRow.tsx`: icon/badge when session is participating in an active channel. Tooltip shows other participant name.
- [ ] 7.5 Create channel viewer component: read-only markdown render of the channel file, opened as a tab in the right panel. Auto-refreshes on `channel:message` events.
- [ ] 7.6 Add toast notifications: "Channel '{topic}' created", "Channel reached consensus", "No response from {name} in 5 minutes".

## 8. Integration & Verification

- [ ] 8.1 End-to-end test: create two sessions, open a channel, verify autonomous routing (A writes CAMBIO → B receives prompt → B writes CAMBIO → A receives prompt → one writes CONSENSO → channel closes).
- [ ] 8.2 Test Quick Share: send context from A to B, verify it appears in B's next prompt via inject_edits.
- [ ] 8.3 Test state-aware queuing: send channel message to a working session, verify it queues and delivers on Stop.
- [ ] 8.4 Test HookState migration: verify existing plan edit and annotation flows still work with session-scoped state files.
- [ ] 8.5 Rebuild CLI binary: `cargo install --path src-tauri --force` after all backend changes.
