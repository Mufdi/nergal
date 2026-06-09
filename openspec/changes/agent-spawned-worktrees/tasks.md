# Tasks — agent-spawned-worktrees

> Redaction only. Depends on `cluihud-mcp-server` (and ships after `cross-session-messaging`). Do NOT start implementation until the three-change set is approved.

## 1. create_worktree_session tool (backend)

- [ ] 1.1 Add `create_worktree_session(workspace_id, prompt, branch_name?, agent?, launch_options?)` to `src-tauri/src/mcp/`: validate workspace is active/known (else structured error), enqueue a pending creation request, return a pending handle. Never create synchronously.
- [ ] 1.2 Pending-request queue keyed by request id, holding requesting session, workspace, branch, prompt, launch options. Optional per-session rate limit (structured "too many pending requests").

## 2. Human confirmation gate

- [ ] 2.1 Tauri command(s) to list/approve/edit/deny pending requests.
- [ ] 2.2 Confirmation UI (modal or actionable toast): requesting session, target workspace, branch, prompt, launch options; Approve / Edit / Deny.
- [ ] 2.3 Ensure auto-mode cannot auto-approve (resource/irreversible; mirror CC v2.1.166 + RULES pause-on-ambiguity). Verify against the auto-mode path.

## 3. Approval flow

- [ ] 3.1 On approve: create the worktree session via the existing worktree machinery + `session-launch-options` (presets/setup).
- [ ] 3.2 Inject the dedicated prompt as the new session's first turn; then transfer control to the user (cluihud sends nothing further).
- [ ] 3.3 On deny: return a structured denied result to the calling agent. On edit: apply user changes before the approve path.

## 4. Lifecycle alignment

- [ ] 4.1 Created worktree uses the existing `<repo>/.worktrees/` convention + Claude-managed unlock-on-finish (CC v2.1.157); standard `git worktree remove`/`prune` works.
- [ ] 4.2 No cluihud auto-delete of approved worktrees.

## 5. Verification

- [ ] 5.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 5.2 `npx tsc --noEmit`
- [ ] 5.3 Manual: agent calls the tool → gate appears, nothing created pre-approval; approve → new worktree session with dedicated prompt + presets, control is the user's; deny → nothing created, agent gets denied result; confirm auto-mode does not bypass the gate.
