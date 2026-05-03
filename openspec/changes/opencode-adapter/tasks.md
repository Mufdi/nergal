## 0. Pre-requisite

- [ ] 0.1 `agent-adapter-foundation` change SHALL be merged before any task here is started.

## 1. Spike: empirical SSE schema mapping

- [ ] 1.1 Install OpenCode locally; run `opencode auth login` with a provider
- [ ] 1.2 Start `opencode serve` and run a small session manually; tee the SSE stream (`curl -N http://127.0.0.1:4096/event`) to a file
- [ ] 1.3 Inspect captured events; produce a concrete schema doc at `docs/agents/opencode-sse-schema.md` with one example per event type observed (message.part.updated, tool.execute.before/after, permission.asked/replied, session.idle, etc.)
- [ ] 1.4 Specifically capture the `message.usage` field (cost/tokens) shape; if absent or unparseable, mark `RAW_COST_PER_MESSAGE` as not declarable
- [ ] 1.5 Specifically check if task-list events exist; mark `TASK_LIST` declarability accordingly
- [ ] 1.6 Document the spike findings; if any blocking unknown remains, raise back to user before proceeding

## 2. Backend module scaffold

- [ ] 2.1 Create `src-tauri/src/agents/opencode/mod.rs` with `OpenCodeAdapter` struct implementing the trait
- [ ] 2.2 Add deps to `Cargo.toml`: `reqwest` (with `json` feature), `eventsource-stream`, `dashmap`
- [ ] 2.3 Register `OpenCodeAdapter` in `AgentRegistry::default_registrations()`

## 3. Server supervisor

- [ ] 3.1 Create `agents/opencode/server_supervisor.rs` with `ServerSupervisor` struct holding `Arc<DashMap<SessionId, ServerInstance>>`
- [ ] 3.2 `ServerInstance` holds `Child` (tokio process), `port: u16`, `started_at`
- [ ] 3.3 `start(session_id)` spawns `opencode serve --port 0` (or range fallback), parses stdout for the chosen port (regex match `listening on http://127.0.0.1:(\d+)`), returns the port
- [ ] 3.4 `stop(session_id)` SIGTERMs the child; if alive after 5s, SIGKILLs
- [ ] 3.5 PID file at `~/.local/state/cluihud/opencode-pids/<cluihud_session_id>.pid` for crash-recovery cleanup
- [ ] 3.6 At app startup, `ServerSupervisor::cleanup_orphans()` reads PID files, checks if cluihud parent PID matches — kill orphans

## 4. SSE client

- [ ] 4.1 Create `agents/opencode/sse_client.rs` with `SseClient::connect(url) -> SseClient`
- [ ] 4.2 Background task: parse SSE frames, deserialize JSON, translate to `TranscriptEvent` / `BackendEvent` per the schema doc from task 1.3
- [ ] 4.3 Push translated events into the `EventSink` provided by `start_event_pump`
- [ ] 4.4 On disconnect: log warning, retry with exponential backoff up to 3 times, then surface error toast
- [ ] 4.5 Unit test: feed mock SSE stream from fixture file, assert correct event translation

## 5. Permission client

- [ ] 5.1 Create `agents/opencode/permission_client.rs` with `submit_permission_response(session_id, permission_id, body)` calling `POST /session/:id/permissions/:pid`
- [ ] 5.2 Track `pending_permissions: DashMap<SessionId, PendingPermission { port, opencode_session_id, permission_id }>`
- [ ] 5.3 When SSE delivers `permission.asked`, populate the map; when `submit_ask_answer` is called, look up and POST

## 6. Adapter trait implementation

- [ ] 6.1 `id()`, `display_name()`, `capabilities()`, `transport()` per design.md
- [ ] 6.2 `detect()`: check `~/.config/opencode/`, `~/.local/share/opencode/`, `which opencode`; populate `binary_path`, `config_path`, `version` (via `opencode --version`)
- [ ] 6.3 `spawn()`: returns a noop `SpawnSpec` (or `Err(AdapterError::TransportNotPty)`) — OpenCode sessions don't use the PTY layer
- [ ] 6.4 `start_event_pump()`: orchestrate supervisor + sse_client per design.md
- [ ] 6.5 `submit_ask_answer()`: call permission_client
- [ ] 6.6 `submit_plan_decision()`: returns `Err(NotSupported(PLAN_REVIEW))`
- [ ] 6.7 `parse_transcript_line()`: returns None — OpenCode transcript is read from SSE in real-time, not from a JSONL file

## 7. PTY layer awareness

- [ ] 7.1 In `pty.rs`, when `spawn_session` is called for an OpenCode session, **do not spawn anything** — return a sentinel "no-pty" session handle
- [ ] 7.2 Update session lifecycle to handle no-PTY sessions: no terminal canvas, no PTY input/output streams, only the chat panel renders
- [ ] 7.3 Tests: spawn_session for OpenCode session returns no-pty; spawn_session for CC session returns PTY as before

## 8. Frontend chat panel

- [ ] 8.1 Create `src/components/chat/OpenCodeChat.tsx` consuming `agentEventStreamAtomFamily(sessionId)`
- [ ] 8.2 Components: `MessageBubble` (role + content + model badge), `ToolUseCard` (collapsible, shows tool name + input), `ToolResultCard` (collapsible, shows output), `PermissionPromptInline` (renders question + options + submit handler invoking `submit_ask_answer`)
- [ ] 8.3 Auto-scroll to bottom on new event unless user has scrolled up (mirror terminal scroll behavior)
- [ ] 8.4 Loading state: spinner when SSE not yet connected; error state: retry button when disconnected after retries

## 9. Workspace render routing

- [ ] 9.1 In `components/layout/Workspace.tsx`: route central area render by `agent_id`. CC/Codex/Pi → `<TerminalManager />`; OpenCode → `<OpenCodeChat />`
- [ ] 9.2 Sidebar SessionRow shows OpenCode badge with chat icon (vs terminal icon for PTY-based)

## 10. Settings panel for OpenCode

- [ ] 10.1 Create `src/components/settings/AgentsSettings.tsx` (if not already created in foundation) with a section per detected agent
- [ ] 10.2 OpenCode section shows: install status, version, "Open OpenCode auth login docs" link, instructions to run `opencode auth login` from a terminal
- [ ] 10.3 No API key input field — explicit non-goal documented in the panel

## 11. Auto-detection wiring

- [ ] 11.1 `OpenCodeAdapter::detect()` runs as part of `AgentRegistry::scan()` at startup
- [ ] 11.2 Settings → "Rescan agents" button re-runs detection

## 12. Tests

- [ ] 12.1 Unit test: SseClient parses a fixture SSE stream and produces expected events
- [ ] 12.2 Integration test: ServerSupervisor starts a mock binary that mimics `opencode serve`, returns port, lifecycle SIGTERM works
- [ ] 12.3 Integration test: end-to-end with mock server — submit ask answer → POST received with correct body
- [ ] 12.4 Manual test: real OpenCode session — start, observe chat rendering, accept a permission via UI, verify OpenCode proceeds

## 13. Documentation

- [ ] 13.1 `docs/agents/opencode.md` with: install instructions, BYO credentials flow, what works/doesn't (no plan mode, no annotation injection), troubleshooting
- [ ] 13.2 Update `CLAUDE.md` with note that OpenCode sessions render chat-style, not TUI
- [ ] 13.3 Update README/CHANGELOG entry highlighting agent-agnostic milestone
