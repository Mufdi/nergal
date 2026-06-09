# Tasks — agent-spawned-worktrees

> Redaction only. Depends on `cluihud-mcp-server` (MCP) AND `cross-session-messaging` (the `SessionDelivery` outcome channel). Do NOT start implementation until the set is approved. Verify cited symbols before editing.

## 1. Request tool + pending queue (request-only, non-blocking)

- [ ] 1.1 `create_worktree_session(workspace_id, prompt, branch_name?, agent?, launch_options?)` in `src-tauri/src/mcp/`: validate `agent_spawned_worktrees_enabled` (default off) → else structured "disabled"; resolve `workspace_id → repo_path` (session/workspace store) → else structured error; enqueue `PendingWorktreeRequest { id, requesting_session, repo_path, branch_name, prompt, launch_options, created_at }`; return `{ pending_request_id }` **immediately** (non-blocking — must not wait for the human, exceeds the agent tool timeout).
- [ ] 1.2 Per-session pending cap → `too_many_pending_requests`.
- [ ] 1.3 **Terminal-status ledger** (NEW-1): a `request_id → {state, session_id?, reason?, resolved_at}` map separate from the pending queue, written at the instant a pending entry is removed, GC'd after a retention TTL (~1 h post-resolution). `get_worktree_request_status(request_id)` checks pending then ledger → `pending | approved{session_id} | denied | timed_out | cancelled | failed{reason} | not_found`. `cancel_worktree_request(request_id)` → remove pending + write `cancelled` to ledger.
- [ ] 1.4 Request timeout (default ~1 h, upper bound ~24 h); on timeout **atomically** remove the queue entry + write `timed_out` to the ledger under one lock (no approve-vs-timeout race → no later-approval orphan).

## 2. Outcome delivery (via cross-session-messaging)

- [ ] 2.1 On a terminal resolution the requester didn't initiate (approved/denied/timed_out/failed), deliver the outcome to `requesting_session` via the `SessionDelivery` channel from `cross-session-messaging` (wake / `additionalContext`: "worktree request X <outcome>"). Skip push for `cancelled` (agent-initiated — it already knows; still recorded in the ledger). Poll via `get_worktree_request_status` is the fallback. **After a daemon restart** (NEW-2): pending + ledger are empty, so NO push is attempted and poll returns `not_found` — do not claim restart-time SessionDelivery (nothing to deliver from).

## 3. Human gate (native, structurally un-bypassable)

- [ ] 3.1 Tauri commands: `list_worktree_requests()`, `approve_worktree_request(id, edited?)`, `deny_worktree_request(id)`. Emit `worktree:request` event.
- [ ] 3.2 Confirmation UI (modal/queue) showing requesting session, workspace, branch, prompt, **current worktree count + free disk**, and — **broken out explicitly** (NEW-3) — the requested `agent` CLI and `permission_preset` (both agent-chosen escalation inputs; not buried in a generic blob). Optionally clamp requestable presets. Approve / Edit / Deny. Handle MULTIPLE concurrent pending requests (list/sequential, not a single modal) — edit/approve serialized per entry under the queue lock.
- [ ] 3.3 The sole approval entry point is the `approve_worktree_request` Tauri command (GUI-only); no MCP/permission-mode handler can invoke it. Document this structural separation (CC `--permission-mode`/bypass `models.rs:104` cannot reach a cluihud modal; no central approver). Headless (no GUI) → request can only resolve `denied`/`timed_out`.

## 4. Approval flow

- [ ] 4.1 On approve: verify the target slug does NOT collide with an existing `.worktrees/cluihud/{slug}` (`create_worktree` reuses on collision, `worktree.rs:261` — could hit a live worktree). Collision → refuse / require branch edit; never inject into an existing worktree.
- [ ] 4.2 Create via `create_worktree(repo_path, slug)`; apply existing `LaunchOptions` (`models.rs:118` — permission preset + short `startup_command` prelude). NO setup-runner (none exists; Workspace presets is a separate backlog item — do not claim to run it).
- [ ] 4.3 Submit the dedicated prompt as the first turn via `pending_prompts` (`pty.rs:74`); transfer control to the user (cluihud sends nothing further).
- [ ] 4.4 `create_worktree` failure (e.g. disk full) → resolve `failed{reason}`. **Spawn-failure rollback** (NEW-4): if `create_worktree` succeeded but the PTY/session spawn then fails, roll back the just-created worktree via `remove_worktree` (`worktree.rs:303`) before `failed{reason}` — no orphan dir (if rollback fails, name the orphan path in the reason). Deny → `denied`. Edit → apply changes then approve path.

## 5. Lifecycle + kill-switch

- [ ] 5.1 Worktree under `.worktrees/cluihud/{slug}`; removable via cluihud's own `remove_worktree` (`worktree.rs:303`). No auto-delete. Do NOT rely on CC v2.1.157 unlock-on-finish (governs CC's own `/worktree`, may not apply).
- [ ] 5.2 Config `agent_spawned_worktrees_enabled` (default off) + optional total-worktree soft cap (warn in gate). In-memory queue + ledger: on daemon restart, both are empty → no push, poll returns `not_found` (agent treats as abandoned). No persistence — a persisted pending request could be approved post-restart into an orphan.

## 6. Verification

- [ ] 6.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` + `npx tsc --noEmit`.
- [ ] 6.2 Automated tests: nothing on FS until approval; **structural auto-mode** (NEW-6 — assert the sole approval path is the `approve_worktree_request` Tauri command and no MCP/permission handler can invoke it, a boundary test, not a flag check); slug-collision refusal; timeout atomically purges + ledger returns `timed_out` then `not_found` after TTL; two concurrent requests; `create_worktree` failure → `failed`; **spawn-failure rolls back the worktree** (no orphan); daemon-restart → poll `not_found` (no push); cancel removes pending + ledger `cancelled`.
- [ ] 6.3 Manual: agent requests → gate appears with count/disk, nothing created pre-approval; approve unique slug → session with dedicated prompt + launch options, control is the user's, outcome delivered to requester; deny → `denied` delivered; collision → refused.
