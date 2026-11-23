## 1. Live set in the agent runtime state

- [x] 1.1 Test (Red): in `src-tauri/src/agents/state.rs` tests — `mark_live` makes a session appear in `live_session_ids`; `mark_dead` removes it; `mark_dead_if` with a different pty id keeps it and with the same pty id removes it; `register_session` alone never makes a session live; `mark_live` → `mark_dead_if` (same id) → `register_session` leaves it not live (late hook); `forget_session` removes a live session.
- [x] 1.2 Test (Red): in the MCP tests — a session with a transcript marker inside the window, registered with `register_session` but never `mark_live`d, reports `is_live: false` in `list_sessions` and `get_session`; a killed session with no marker is absent from `list_sessions` and `get_session` returns `None`; a not-live session with recorded activity, touched files and a last message gets the persisted mode and empty live-only fields; `send_to_session` to it returns `inactive_target`, and `request_session_resume` for it returns `pending` (not `already_live`).
- [x] 1.3 Add `live_ptys` plus `mark_live`, `mark_dead`, `mark_dead_if` to `AgentRuntimeState`; `live_session_ids` reads `live_ptys`; `forget_session` clears it (implementation.md §1).
- [x] 1.4 Switch the MCP test helpers that fake liveness (`mcp/directory.rs` `make_live`, `mcp/messaging.rs` seeding helper, and any other test that relied on `register_session`) to `mark_live`.

## 2. PTY lifecycle wiring

- [x] 2.1 Test (Red): in `src-tauri/src/pty.rs` tests — a new `reserve_session_pty` marks the session live under its pty id with mode `running`, and an existing reservation leaves the live set unchanged; `release_session_pty` with the current pty id clears the reservation and liveness; `release_session_pty` with an older pty id leaves a newer reservation and its liveness in place; `kill_session_pty_inner` on a live session clears it; a kill that removed `pty-1` after `pty-2` was marked live keeps `pty-2` live; `on_agent_pty_eof` with the current pty id clears it; `on_agent_pty_eof` with an older pty id keeps the newer one live; `has_session_pty` is true while reserved and false after release. Update the existing reservation tests to the new signatures without changing their assertions.
- [x] 2.2 `reserve_session_pty` (mark live + `running`) and `release_session_pty` (compare-and-remove + `mark_dead_if`) take `&AgentRuntimeState` and change liveness inside their `session_ptys` lock hold; `start_claude_session` passes it and adds `mark_dead_if` on `Gone` (implementation.md §2).
- [x] 2.3 Extract `kill_session_pty_inner` with `mark_dead_if(removed id)` inside the removal lock; the `kill_session_pty` command takes `State<AgentRuntimeState>` and delegates.
- [x] 2.4 Reader thread agent branch: clone the pty id before the spawn and call `on_agent_pty_eof` after the obsidian finalize.
- [x] 2.5 Add `PtyManager::has_session_pty` (poisoned lock → `true`) and `PtyManager::resume_block_state` (both reads in one lock hold). Test (Red) in 2.1: `resume_block_state` returns `(true, true)` after a new reservation, `(false, true)` after `on_agent_pty_eof`, `(false, false)` after release.

## 3. Resume approval guard

- [x] 3.1 Test (Red): `resume_blocked_reason` matrix — `(is_live: true, has_pty: true)` returns the "message it directly" reason; `(false, true)` returns the close-the-tab reason; `(false, false)` returns `None`.
- [x] 3.2 In `approve_worktree_request`, refuse a resume per `resume_blocked_reason(resume_block_state(..))`: resolve `Failed`, `notify_outcome`, queue no prompt (implementation.md §3).
- [x] 3.3 `descriptor_from` drops live-activity fields for a not-live session (implementation.md §4).
- [x] 3.4 Update the warmup comment at `mcp/worktree_sessions.rs:1014-1017` and the liveness docs in `agents/state.rs`.

## 4. Review follow-ups (adversarial, confirmed by the user)

- [x] 4.1 Test (Red): a killed session's nergal id and a CC id mapped to it are unidentified by `resolve_session_hint`; a live one resolves; switch the inline identity tests in `agents/state.rs` to `mark_live`. A session stays live while its PTY is reserved and not killed (soft-close scenario). A busy session that is not yet live gets `inactive_target` from `send_to_session`, and `queued` once `mark_live` runs.
- [x] 4.2 `resolve_session_hint` resolves only sessions in the live set (implementation.md §5).
- [x] 4.3 MCP instructions text for not-live descriptors, and the `inactive_target` hint plus the `send_to_session` description point to `request_session_resume`.

- [x] 4.4 Test (re-lock, walk finding): rewrite `a_not_live_descriptor_uses_the_persisted_mode_and_empty_live_only_fields` in `mcp/liveness_tests.rs` to the scenario "A not-live descriptor keeps past facts and drops present claims", and re-lock the file.
- [x] 4.5 `descriptor_from` keeps `last_activity`, `last_assistant_message` and `recently_touched_files` for a not-live session; MCP instructions text follows (implementation.md §4, §5).

## 5. Verification

- [x] 5.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings`
- [x] 5.2 `cd src-tauri && cargo test`
- [x] 5.3 `rustfmt --edition 2024 --check <changed .rs files>`
- [x] 5.4 `npx tsc --noEmit` and `pnpm test` (no frontend change expected; confirms the IPC shape)
- [x] 5.5 `~/.claude/skills/work/scripts/work-gates.sh main session-liveness-from-pty`
- [x] 5.6 Manual walk (installed `.deb`): create a session with `create_worktree_session`, approve, close its tab, wait 5 s, then `request_session_resume` from another session returns `pending` and approving it revives the session.
- [x] 5.7 Manual walk: with a session that finished at least one turn, after closing the tab, `list_sessions` from another session shows it with `is_live: false`, and `send_to_session` to it returns `inactive_target`.
- [x] 5.8 Manual walk: in a session's tab, exit the agent and then the shell (`exit`); `list_sessions` shows it with `is_live: false`, and approving a `request_session_resume` for it fails with the "close the tab" reason. Then close the tab, wait 5 s, request the resume again and approve it: the session revives.
