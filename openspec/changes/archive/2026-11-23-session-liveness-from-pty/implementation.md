## Verified codebase facts (do not re-assume)

Agent runtime state (`src-tauri/src/agents/state.rs`):
- `AgentRuntimeState` is `#[derive(Clone)]` with `Arc<DashMap<..>>` fields (`:50-103`). `bootstrap()` builds every field (`:109-127`).
- `register_session` inserts into `agent_id_cache` only (`:137-140`).
- `forget_session` removes `agent_id_cache` plus the five side maps (`:145-153`).
- `live_session_ids()` returns the `agent_id_cache` keys as an owned `HashSet<String>` (`:157-162`). This is the only liveness source.
- The test module starts at `:302`.

PTY layer (`src-tauri/src/pty.rs`):
- `PtyManager.session_ptys: Mutex<HashMap<String, String>>` maps session id → pty id (`:121`).
- `spawn_pty(app, state, pty_id, …, session_id, agent_session, shell_ready_tx)` (`:457-593`). The reader thread ends at `:546`. Its agent branch only calls `finalize_session_obsidian` through `eof_app.try_state::<crate::db::SharedDb>()` (`:546-552`). `pty_id` is moved into `instances` at `:589`, so the thread needs its own clone captured before the spawn.
- `reserve_session_pty` returns `Ok(None)` on a new reservation and `Ok(Some(existing))` when one exists (`:640-651`). `release_session_pty` removes the reservation (`:654-658`).
- `start_claude_session` (`:708`) takes `agents: State<'_, AgentRuntimeState>` (`:711`). It builds `pty_id` (`:734-741`), reserves (`:743-747`), calls `spawn_pty` with `agent_session = true` (`:751-766`, rollback via `release_session_pty` at `:764`), then `confirm_reservation_or_extract` (`:768-778`, `Gone` returns `Err` at `:776`).
- The only other `spawn_pty` caller is the aux-shell path with `agent_session = false` (`:1164`).
- `kill_session_pty(state: State<'_, PtyManager>, session_id: String)` (`:1520-1542`) removes the `session_ptys` entry, then the instance, then drops it off the main thread. It has no `AgentRuntimeState` access today. Registered in `src-tauri/src/lib.rs:516`. `AgentRuntimeState` is managed state (`lib.rs:500`).
- `reserve_session_pty` / `release_session_pty` have one production caller each, `start_claude_session` (`:743`, `:764`). Existing reservation tests call them directly with `PtyManager::new()` (`pty.rs:2300-2445`: `:2305`, `:2308`, `:2324`, `:2326`, `:2334`, `:2389`, `:2401`, `:2419`). A signature change updates those call sites mechanically; their assertions stay.
- `PtyManager::agent_shell_pid` (`:192-195`) is the pattern for a session-keyed read of `session_ptys`.
- `shutdown_all` (`:205-222`) clears `session_ptys` on exit; no live-set change needed (design D3).
- Errors after `Intact` in `start_claude_session` (`:815-818`, `:889`, `:921-925`) leave the reservation and PTY in place. Out of scope (design Risks).

Resume approval (`src-tauri/src/mcp/worktree_sessions.rs`): `approve_worktree_request` (`:709`) already takes `pty: State<PtyManager>` (`:720`). It calls `resume_session` for `RequestKind::Resume` (`:767`), then `queue_session_prompt` with the relayed text. Failure handling resolves the request through `gate.resolve(.., WorktreeRequestState::Failed { reason })` plus `notify_outcome` (pattern at `:736-753`). The warmup comment to update is `:1014-1017`.

Agent runtime docs that describe the old semantics: module doc `src-tauri/src/agents/state.rs:1-9` and the rustdoc of `live_session_ids` (`:155-156`).

Session delete (`src-tauri/src/commands/workspaces_sessions.rs:749`): calls `agents.forget_session(&session_id)` before `db.delete_session`.

Hook server (`src-tauri/src/hooks/server.rs:958-967`): registers any uncached `nergal_session_id` as claude-code via `register_session`. No change needed once liveness leaves `agent_id_cache`.

Liveness consumers (no call-site change, they all call `live_session_ids()`):
- `src-tauri/src/mcp/directory.rs:122` (`list_sessions`), `:174` (`get_session`).
- `src-tauri/src/mcp/messaging.rs:64` (`send_to_session` → `inactive_target`).
- `src-tauri/src/mcp/worktree_sessions.rs:398-408` (`request_session_resume` → `already_live`), `:1000-1008` (`resume_session` approval re-check).

Test helpers that fake liveness with `register_session`:
- `src-tauri/src/mcp/directory.rs:336-338` (`make_live`).
- `src-tauri/src/mcp/messaging.rs:480` (inside the session seeding helper).
- Any other test that expects `is_live: true` or a delivered send after only `register_session` (grep `register_session` under `src-tauri/src/mcp/` and `src-tauri/src/hooks/` test code).

## Execution order

1. `agents/state.rs`: add the live set and its methods; switch `live_session_ids`; clear in `forget_session`.
2. MCP test helpers: switch fake liveness to `mark_live`. Run the MCP tests to confirm nothing else relied on `register_session` meaning live.
3. `pty.rs`: reserve/release with liveness, kill helper, reader thread, `has_session_pty`.
4. `mcp/worktree_sessions.rs`: resume approval guard and the warmup comment.
5. `mcp/directory.rs`: not-live descriptor.
6. Doc comments in `agents/state.rs`.
7. Full Rust gates.

## Per-section plan

### 1. `AgentRuntimeState` live set

- Field `live_ptys: Arc<DashMap<String, String>>`, built in `bootstrap()`.
- `mark_live(&self, session_id: &str, pty_id: &str)`: insert or overwrite.
- `mark_dead(&self, session_id: &str)`: remove.
- `mark_dead_if(&self, session_id: &str, pty_id: &str)`: remove only when the stored value equals `pty_id`. Use `DashMap::remove_if` so the compare and the remove are one step.
- `live_session_ids()` collects `live_ptys` keys.
- `forget_session` also calls `live_ptys.remove`.
- `register_session` stays as is.

### 2. PTY wiring (`pty.rs`)

- `reserve_session_pty(state, agents, session_id, pty_id)`: on a new reservation, call `agents.mark_live(session_id, pty_id)` and `agents.record_activity(session_id, "running", None)` before the lock guard drops.
- `release_session_pty(state, agents, session_id, pty_id)`: remove the reservation only if `session_ptys.get(session_id) == Some(pty_id)`, and call `agents.mark_dead_if(session_id, pty_id)` in the same lock hold.
- `start_claude_session`: pass `&agents` to both. On `ReservationOutcome::Gone`, also call `agents.mark_dead_if(&session_id, &pty_id)`.
- Helper `fn kill_session_pty_inner(state: &PtyManager, agents: &AgentRuntimeState, session_id: &str) -> Result<(), String>` holding today's body. Inside the `session_ptys` lock block that removes the entry, call `agents.mark_dead_if(session_id, &removed)` when an id was removed. The command `kill_session_pty` gains `agents: State<'_, AgentRuntimeState>` and delegates. The JS call shape (`{ sessionId }`) does not change, because Tauri injects `State` params.
- Reader thread (agent branch): capture `let eof_pty_id = pty_id.clone();` before `pty_id` moves. After the obsidian finalize, call `on_agent_pty_eof(agents, &eof_session, &eof_pty_id)` via `eof_app.try_state::<crate::agents::state::AgentRuntimeState>()`. `fn on_agent_pty_eof(agents: &AgentRuntimeState, session_id: &str, pty_id: &str)` calls `mark_dead_if`; it exists so the test runs the same code the thread runs.
- `PtyManager::has_session_pty(&self, session_id: &str) -> bool`: true when `session_ptys` holds an entry. A poisoned lock returns `true`, so the resume approval refuses rather than queue into a PTY it cannot check.
- `PtyManager::resume_block_state(&self, agents: &AgentRuntimeState, session_id: &str) -> (bool, bool)`: reads `(is_live, has_pty)` inside one `session_ptys` lock hold. A poisoned lock returns `(false, true)`.

### 3. Resume approval guard (`mcp/worktree_sessions.rs`)

- Pure `fn resume_blocked_reason(is_live: bool, has_pty: bool) -> Option<String>`: `is_live` → `Some("session became live before approval — message it directly")` (same text as `resume_session`, `:1007`); else `has_pty` → `Some("the session's tab is still open; close it, then request the resume again")`; else `None`.
- In `approve_worktree_request`, for `RequestKind::Resume { target_session_id }`, call it with the pair from `pty.resume_block_state(&agents, target_session_id)` before `resume_session`. On `Some(reason)`: resolve `Failed { reason }`, call `notify_outcome`, return `Err(reason)`. Reuse the kill-switch failure branch shape (`:736-753`).
- Update the comment at `:1014-1017` to say the busy mark takes effect once the frontend reservation makes the session live.

### 4. Not-live descriptor (`mcp/directory.rs`)

- `descriptor_from` (`mcp/directory.rs:231-290`): when `!is_live`, `mode` comes from `s.status`, `waiting_for` is `None`, and `background_tasks` / `session_crons` are empty. `last_activity` comes from `agents.session_activity(&s.id)` when present (else `s.updated_at`), and `recently_touched_files` / `last_assistant_message` come from the runtime maps as for a live session. Live sessions keep today's reads.

### 5. Identity and MCP text (adversarial review)

- `AgentRuntimeState::resolve_session_hint` (`agents/state.rs:308`): return `Some(hint)` only when `live_ptys` contains it; for the CC side map, return the mapped nergal id only when `live_ptys` contains it. Its only caller is `mcp/mod.rs:795` (`handle_connection`, per tool call). The inline tests `hint_resolves_registered_nergal_id`, `cc_side_map_resolves_to_nergal_id` and `forget_tears_down_cc_side_map` (`agents/state.rs:361-393`) identify callers with `register_session` and must switch to `mark_live` (test-writer).
- MCP server instructions (`mcp/mod.rs:220-223`): say that for a not-live session `mode` is the last persisted value, `background_tasks` / `session_crons` are empty, and `last_activity`, `last_assistant_message` and `recently_touched_files` keep their last recorded value (empty when the daemon never saw the session).
- `inactive_target` hint (`mcp/messaging.rs:67`) and the `send_to_session` tool description (`mcp/mod.rs:262`): point to `request_session_resume` to revive the session, and to `create_worktree_session` for a new one.

### 6. Docs

- Update the module doc (`agents/state.rs:1-9`) and the `live_session_ids` rustdoc to describe the live set.

## Per-phase risk

- **Phase 1:** a test elsewhere relies on `register_session` meaning live. Mitigation: run `cargo test` after step 2 and switch each failing helper to `mark_live`. Never weaken an assertion.
- **Phase 2:** `pty_id` is moved into `instances` before the thread could read it. Mitigation: clone it before `std::thread::spawn`.
- **Phase 2:** the `reserve_session_pty` / `release_session_pty` signature change breaks the existing reservation tests at compile time. Mitigation: the test-writer updates those call sites at Red (pass a `AgentRuntimeState::bootstrap()`), with no assertion change.
- **Phase 2:** `kill_session_pty` is an IPC command. Adding a `State` param keeps the call shape, but the command must stay registered (`lib.rs:516`). Mitigation: `cargo check` plus the full test run.
- **Formatting:** locked tests exist in the tree. Use `rustfmt --edition 2024 <changed files>`, never bare `cargo fmt`.
