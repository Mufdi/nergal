# Implementation Plan: agent-spawned-worktrees

> Depends on `cluihud-mcp-server` (MCP) AND `cross-session-messaging` (the `SessionDelivery` outcome channel). Grounded in `src-tauri/src/`; verified 2026-06-08.

## Verified codebase facts (do not re-assume)

- `create_worktree(repo_path, slug)` → `.worktrees/cluihud/{slug}`, branch `cluihud/{slug}`, **reuses the dir if it exists** (`worktree.rs:261`). → collision check is mandatory before approval.
- `remove_worktree` (`worktree.rs:303`) is cluihud's own cleanup path.
- `LaunchOptions = {permission_preset, allow_skip_in_cycle, startup_command}` (`models.rs:118`); `startup_command` is a SHORT prelude, explicitly NOT for long-running setup. → no setup-runner exists.
- `pending_prompts` map (`pty.rs:74`) submits the first prompt at spawn.
- No central permission approver; delegated to `--permission-mode` (`models.rs:104`). → the gate is structurally outside any agent permission path.

## Execution order

1. Request tool + pending queue (non-blocking) + status/cancel/timeout.
2. Human gate (Tauri commands + native UI, multi-request).
3. Approval flow (collision check + create + launch options + first prompt).
4. Outcome delivery via `cross-session-messaging` `SessionDelivery`.
5. Lifecycle + kill-switch + volatile-queue behavior.
6. Tests.

## 1. Request tool + queue — `src-tauri/src/mcp/`

- `create_worktree_session(...)`: gate on `agent_spawned_worktrees_enabled` (default off); resolve `workspace_id → repo_path` via the session/workspace store; enqueue `PendingWorktreeRequest { id, requesting_session, repo_path, branch_name, agent, prompt, launch_options, created_at }`; return `{ pending_request_id }` **immediately**. Per-session pending cap.
- **Two structures**: `pending: HashMap<RequestId, PendingWorktreeRequest>` + a **terminal ledger** `HashMap<RequestId, {state, session_id?, reason?, resolved_at}>` written when a pending entry is removed, GC'd after a TTL (~1 h). `get_worktree_request_status(id)` checks pending then ledger → `... | not_found`. `cancel_worktree_request(id)` removes pending + writes `cancelled`.
- Timeout: per-request deadline (default ~1 h, cap ~24 h); on fire, **atomically** (one lock) remove the pending entry + write `timed_out` to the ledger, so a concurrent approve can't race a timeout into an orphan.

## 2. Human gate — Tauri + UI

- Commands `list_worktree_requests` / `approve_worktree_request(id, edited?)` / `deny_worktree_request(id)`; emit `worktree:request`.
- UI: a queue/list (NOT a single modal — multiple sessions can request concurrently), each row Approve/Edit/Deny, showing requesting session + workspace + branch + prompt + worktree count + free disk + the requested `agent` and `permission_preset` **broken out explicitly** (agent-chosen escalation inputs). Optionally clamp requestable presets. Edit/approve operate per-entry under the queue lock.
- Structural un-bypassability: the sole approval entry is the `approve_worktree_request` Tauri command — no function approves without it, and no MCP/permission handler can invoke it. Document that CC bypass/`--permission-mode` (`models.rs:104`) cannot reach it. Headless → no GUI → only deny/timeout.

## 3. Approval flow

- Resolve slug (reuse the existing diacritics-strip + timestamp slug logic). **Collision check**: if `.worktrees/cluihud/{slug}` exists, refuse (or require edit) — never call `create_worktree` into a live worktree.
- `create_worktree(repo_path, slug)`; start the session via the normal spawn path applying `LaunchOptions`; submit the dedicated prompt via `pending_prompts` (`pty.rs:74`); hand off.
- `create_worktree` failure (disk full etc.) → `failed{reason}`. **Spawn-failure rollback**: if `create_worktree` succeeded but the spawn fails, `remove_worktree` (`worktree.rs:303`) the just-created worktree before `failed{reason}` — no orphan (if rollback fails, name the orphan path in the reason). Resolve the request (ledger) and trigger delivery (§4).

## 4. Outcome delivery

- On terminal resolution, call the `cross-session-messaging` `SessionDelivery` to wake `requesting_session` with the outcome ("worktree request X approved → session Y" / "denied" / "failed: …"). Reuses change-2's idle-PTY / Stop-`additionalContext` paths. Poll (`get_worktree_request_status`) is the fallback if delivery is disabled.

## 5. Lifecycle + kill-switch

- `.worktrees/cluihud/{slug}` + `remove_worktree` (`worktree.rs:303`); no auto-delete; no v2.1.157 reliance.
- `agent_spawned_worktrees_enabled` (default off) + optional total-worktree soft cap (warn in gate). In-memory queue + ledger; on restart both empty → no push, poll returns `not_found` (agent treats as abandoned). Do not persist requests.

## Per-phase risk

- **Phase 1**: a synchronous create would bypass the gate. Test FIRST: nothing on FS until approval.
- **Phase 1/2**: timeout-vs-approve race → orphan. One lock around queue mutation; atomic remove-on-timeout. Adversarial test.
- **Phase 3**: slug collision injects into a live worktree (data loss). Mandatory existence check before `create_worktree`.
- **Phase 3**: create succeeds, spawn fails → orphan worktree. Roll back via `remove_worktree` before `failed`.
- **Phase 4**: outcome never delivered → agent holds a dead handle. The ledger + poll tool is the guaranteed fallback (survives the pending-queue purge); delivery is best-effort on top. Post-restart: poll returns `not_found` (honest abandonment).

## Verification

`cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` · `npx tsc --noEmit` · automated tests (nothing-pre-approval, structural auto-mode, collision refusal, timeout purge, two concurrent requests, create-failure, restart-drop, cancel) · manual gate walk (see proposal Build contract).
