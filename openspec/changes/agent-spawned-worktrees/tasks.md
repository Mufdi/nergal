# Tasks — agent-spawned-worktrees

> Implemented 2026-06-20 on `main` (default-off, NOT released). Depends on `cluihud-mcp-server` (MCP) + `cross-session-messaging` (`SessionDelivery`). Core module: `src-tauri/src/mcp/worktree_sessions.rs`.

## 1. Request tool + pending queue (request-only, non-blocking)

- [x] 1.1 `create_worktree_session(workspace_id, prompt, branch_name?, agent?, launch_options?)` in `src-tauri/src/mcp/worktree_sessions.rs`: validates `agent_spawned_worktrees.enabled` (default off) → `"disabled"`; resolves `workspace_id → repo_path` via `db.workspace_repo_path` (+ `is_git_repo`) → `"invalid_workspace"`; enqueues `PendingWorktreeRequest`; returns `{ pending_request_id }` **immediately** (sync dispatch, non-blocking). Wired in `mcp/mod.rs` dispatch + `tool_definitions`.
- [x] 1.2 Per-session pending cap (`max_pending_per_session`, default 3) → `too_many_pending_requests`.
- [x] 1.3 **Terminal-status ledger**: `GateInner { pending, ledger }` under one `Mutex`; `resolve` writes the ledger at the purge instant; GC'd after `LEDGER_RETENTION_SECS` (~1 h). `get_worktree_request_status` checks pending then ledger → `pending|approved|denied|timed_out|cancelled|failed|not_found`. `cancel_worktree_request` removes pending + writes `cancelled` (ownership-checked).
- [x] 1.4 Request timeout (default ~1 h, clamped [60s, 24h]); `due_timeouts` **atomically** removes the entry + writes `timed_out` under one lock (test `approve_after_timeout_is_refused`).

## 2. Outcome delivery (via cross-session-messaging)

- [x] 2.1 `notify_outcome` delivers the outcome to `requesting_session` via the `SessionDelivery` channel — PTY wake ONLY when the requester is idle (`session_activity.mode == "idle"`; never paste mid-turn), plus a `worktree:resolved` frontend event. `cancelled` skips the wake (agent-initiated). Poll is the fallback. After restart: queue+ledger empty → no push, poll returns `not_found`. Note (divergence from the redaction): no Stop-`additionalContext` fast-path — that path is DB-queue-driven (cross-session 4.3, deferred); the idle wake + poll are the delivery, which is honest and sufficient.

## 3. Human gate (native, structurally un-bypassable)

- [x] 3.1 Tauri commands `list_worktree_requests` / `approve_worktree_request(id, edited_prompt?, edited_branch?)` / `deny_worktree_request(id)`; `create_worktree_session` emits `worktree:request`.
- [x] 3.2 Confirmation UI `src/components/worktree/WorktreeGate.tsx`: non-blocking floating queue (handles multiple concurrent requests), per-card Approve/Edit/Deny, showing requesting session, workspace, branch, prompt, **worktree count + free disk** (`GateResourceInfo` via `statvfs` + `list_worktrees`), and — **broken out + flagged** — the requested `agent`, `permission_preset` (bypass flagged), `startup_command` (destructive code block), and `allow_skip_in_cycle` (security review H1/H2: every escalation input the PTY consumes is surfaced).
- [x] 3.3 Sole approval entry = `approve_worktree_request` Tauri command (GUI-only; registered only in `lib.rs` invoke_handler, absent from `tool_definitions`). Boundary test `approval_path_is_not_reachable_as_an_mcp_tool`. Headless (`NoopDelivery`, no GUI) → resolves only `denied`/`timed_out`.

## 4. Approval flow

- [x] 4.1 Collision check `.worktrees/cluihud/{slug}.exists()` before `create_worktree` → refuse (require branch edit); never inject into a live worktree.
- [x] 4.2 `create_worktree(repo_path, slug)` + existing `LaunchOptions` (mirrors `clickup_spawn_worktree_with_task`). NO setup-runner.
- [x] 4.3 First prompt via `queue_session_prompt` (`pending_prompts`); frontend activates the returned session (`worktreeGate.ts` sets `activeSessionIdAtom` → PTY spawn consumes the prompt) → control is the user's.
- [x] 4.4 `create_worktree` failure → `failed{reason}`. **Spawn-failure rollback**: `create_session` failure after `create_worktree` rolls back via `remove_worktree` before `failed{reason}` (names the orphan path if rollback fails). Deny → `denied`. Edit → trim+fallback then approve. Kill-switch re-checked at approve (M2).

## 5. Lifecycle + kill-switch

- [x] 5.1 Worktree under `.worktrees/cluihud/{slug}`; removable via cluihud's own `remove_worktree`. No auto-delete; no CC v2.1.157 reliance.
- [x] 5.2 Config `AgentWorktreesConfig { enabled (default off), request_timeout_secs, max_pending_per_session, soft_worktree_cap }`; backend-owned (`agent_worktrees_set_enabled` + `BACKEND_OWNED_CONFIG_KEYS`); Settings toggle in `McpSection.tsx`. In-memory queue+ledger; restart empties both → no push, poll `not_found`. No persistence.

## 6. Verification

- [x] 6.1 `cargo clippy -- -D warnings` clean · `cargo test` 659 passed · `cargo fmt --check` clean · `npx tsc --noEmit` clean · `vite build` OK.
- [x] 6.2 Automated tests (12 in `worktree_sessions`): disabled/empty-prompt/unknown-workspace enqueue nothing; pending→resolved→not_found; `take_pending` mutual exclusion; timeout atomic purge + `timed_out`; approve-after-timeout refused; cancel ownership; per-session count; ledger GC; outcome-note advisory; structural-boundary (approve/deny not MCP tools). Security review: SHIP after H1/H2/M1/M2 fixes applied + re-verified.
- [ ] 6.3 Manual gate walk (USER, needs the app running): agent requests → gate appears with count/disk + flagged escalation inputs, nothing created pre-approval; approve unique slug → session with dedicated prompt + launch options, control is the user's, outcome delivered to requester; deny → `denied` delivered; collision → refused; cancel from agent; timeout purge.
