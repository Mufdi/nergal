## Context

cluihud creates worktree sessions on user action today: `create_worktree(repo_path, slug)` makes `.worktrees/cluihud/{slug}` with branch `cluihud/{slug}` and **reuses the directory if it already exists** (`worktree.rs:261`); `remove_worktree` (`worktree.rs:303`) is the cleanup path; `LaunchOptions = {permission_preset, allow_skip_in_cycle, startup_command}` (`models.rs:118`) where `startup_command` is a **short PTY prelude** that must not be long-running (it would block the agent from starting); `pending_prompts` (`pty.rs:74`) submits a first prompt at spawn. `cluihud-mcp-server` gives agents a tool surface + identity; `cross-session-messaging` provides the `SessionDelivery` channel (PTY wake / Stop-hook `additionalContext`) and the consensus that motivates a spawn. This change adds the agent-initiated entry point — gated, because creating a session is resource-consuming and hard to reverse.

## Goals / Non-Goals

**Goals:**
- Let an agent **request** (never directly create) a dedicated worktree session under an active workspace.
- A mandatory human gate that is **structurally** un-bypassable by any agent permission mode.
- Deliver the outcome back to the requesting agent **without blocking** its tool call.
- Reuse cluihud's existing worktree creation + the existing `LaunchOptions`.
- Hand control to the user after the first prompt; cluihud does not autopilot.

**Non-Goals:**
- Autonomous, ungated session creation (rejected — resource/irreversible).
- Building the "Workspace presets" setup-runner (`pnpm install`, `docker compose up`) — that is a **separate, unimplemented backlog item**; this change only passes the existing `LaunchOptions` (short `startup_command` prelude + permission preset). It does NOT "fold" or "reuse" a presets capability that does not exist.
- cluihud driving the spawned session beyond its first prompt.
- Auto-deletion of approved worktrees (the user owns lifecycle).

## Decisions

### 1. The tool requests; outcome is delivered asynchronously (never blocks)

**Decision**: `create_worktree_session(...)` enqueues a `PendingWorktreeRequest` and **immediately returns `{ pending_request_id }`** (non-blocking — an MCP tool call must not hang for human latency, which routinely exceeds CC's ~60s tool timeout). The outcome reaches the agent two ways:
- **Push**: when the human decides, cluihud delivers the result to the requesting session via the `cross-session-messaging` `SessionDelivery` channel (a wake / `additionalContext`: "worktree request X approved, session Y created" or "denied"). This is why this change ships **after** `cross-session-messaging`.
- **Pull**: a `get_worktree_request_status(request_id)` tool returns `pending | approved{session_id} | denied | timed_out | cancelled | failed{reason}` for agents that prefer to poll.

The agent may also `cancel_worktree_request(request_id)` while it is pending.

**Two-structure data model (round-2 finding 1)**: a purged pending entry cannot also answer a poll. So the daemon holds **two** structures: a `pending` map (active requests) and a **terminal-status ledger** (`request_id → {state, session_id?, reason?, resolved_at}`) written at the same instant a pending entry is removed (on approve/deny/timeout/cancel/failure). `get_worktree_request_status` checks pending first, then the ledger. The ledger is GC'd after a retention TTL (e.g. 1 h after resolution). Unknown id → `not_found`.

**Why**: Round-1 found the original "return a handle" vs "resolve after human decision" wording was two mutually exclusive blocking/non-blocking models with no delivery mechanism. Non-blocking handle + the already-built delivery channel + a poll/cancel pair resolves it; the terminal-status ledger reconciles "atomically purge the pending entry" with "poll can still report `timed_out`".

### 2. Mandatory human gate, structurally outside any agent permission path

**Decision**: cluihud surfaces a **native modal/queue UI** (not an agent-facing decision) showing requesting session, target workspace, branch, prompt, the current worktree count + free disk, and — **broken out explicitly, not buried in a generic blob** — the requested **`agent` CLI** and **`permission_preset`** (round-2 finding 3: the requesting agent chooses both, and a permissive preset is exactly the escalation this gate exists to stop; the human must see them prominently). Optionally, the requestable presets are **clamped** so an agent cannot request a bypass preset at all. Actions: Approve / Edit / Deny. The gate is un-bypassable **by construction**: it lives entirely outside every agent's permission system — CC's `--permission-mode`/`bypassPermissions` (`models.rs:104`) has no path that can reach a cluihud GUI modal, and cluihud has no central permission auto-approver to subvert. There is no programmatic approve path at all; the sole approval entry point is the `approve_worktree_request` Tauri command, invokable only by the GUI (round-2 finding 6 — the structural-separation test asserts no MCP/permission-mode handler can reach it).

**Headless/cron**: if no GUI is present (headless run), the request can only ever resolve as `denied`/`timed_out` — there is no human to approve. Documented, not a silent hang.

**Why**: Round-1 noted the strength is structural, not a discipline claim. Stating the architectural separation is the real guarantee; RULES also requires pausing on resource/irreversible actions even in auto-mode.

### 3. Approval reuses existing creation + the existing LaunchOptions, with a uniqueness check

**Decision**: On approve, cluihud resolves `workspace_id → repo_path` (via the session/workspace store), **validates the target slug does not collide with an existing `.worktrees/cluihud/{slug}`** (`create_worktree` reuses on collision — `worktree.rs:261` — which could inject into a *live* worktree with uncommitted work), creates the worktree, applies the existing `LaunchOptions` (permission preset + short `startup_command` prelude), submits the dedicated prompt via `pending_prompts` (`pty.rs:74`), then hands control to the user. It does **not** run a project setup-runner (none exists).

**Collision handling**: if the slug collides, the gate surfaces it and the create is refused (or the user edits the branch); cluihud never injects a prompt into or spawns a second PTY on an existing worktree path.

**Partial-failure rollback (round-2 finding 4)**: the approve sequence is collision-check → `create_worktree` → spawn session → `pending_prompts`. If `create_worktree` succeeds but the **PTY/session spawn** then fails, cluihud rolls back the just-created worktree via `remove_worktree` (`worktree.rs:303`) before resolving `failed{reason}` — so a spawn failure does not leave an orphan worktree on disk (the "no auto-delete" rule applies to *successfully handed-off* worktrees, not to a half-created one cluihud itself just made). If rollback itself fails, the orphan path is surfaced explicitly in the `failed` reason.

**Why**: Reuse keeps parity with user-created worktrees. The uniqueness check closes the data-loss/session-collision hole; the rollback closes the create-then-spawn-fail orphan. The presets claim is dropped to match the verified codebase.

### 4. Lifecycle via cluihud's own worktree tooling; no auto-delete

**Decision**: Created worktrees use `.worktrees/cluihud/{slug}` and are removable via cluihud's own `remove_worktree` (`worktree.rs:303`) — the same path user-created worktrees use. cluihud does NOT auto-delete an approved worktree. No reliance on CC v2.1.157 unlock-on-finish (that governs CC's *own* `/worktree` feature and may not apply to externally-created worktrees).

**Why**: Round-1 flagged the v2.1.157 attribution as likely wrong for cluihud-created worktrees. Relying on our own `remove_worktree` is correct and verified.

### 5. Resource bounds + failure semantics

**Decision**: Cap concurrent **pending** requests per session (structured `too_many_pending_requests`) and surface the current worktree **count + free disk** in the gate UI so the human approves with context. If `create_worktree` fails mid-approval (e.g. disk full), the request resolves `failed{reason}` and that is delivered/pollable. A total-worktree soft cap (configurable) warns in the gate.

**Why**: Round-1 noted growth being "human-paced" still has no guardrail and no create-failure result. Surfacing cost at the gate + a defined failure result closes it.

### 6. Kill-switch + volatile-queue behavior

**Decision**: A config flag `agent_spawned_worktrees_enabled` (default **off**) gates the tool. The pending queue AND the terminal-status ledger live in daemon memory and are NOT persisted. On daemon restart both are empty, so the daemon no longer knows which sessions had pending requests — therefore it **cannot push-notify**, and a post-restart poll returns `not_found` (round-2 finding 2: the earlier "notified via SessionDelivery on restart" claim was impossible — there is nothing left to deliver from). The honest contract: after a restart, in-flight requests are abandoned; an agent that polls gets `not_found` and must treat it as such. Non-persistence is deliberate — a persisted pending request could be approved after restart into an orphan.

**Why**: Round-1 flagged no disable path and a volatile queue that silently strands agents; round-2 corrected the impossible restart-notify. Abandoned-on-restart with `not_found` on poll is the only behavior an in-memory model can actually deliver.

## Risks / Trade-offs

**[Risk] Agent spam-requests creation** → Every request hits the human gate; nothing is created without approval. Per-session pending cap keeps the gate UI usable.

**[Risk] Coerced/malicious prompt via a relayed message** → The human sees the requesting session + full prompt before approving (and `cross-session-messaging` labels relayed context advisory). The gate is the boundary; a relayed message cannot auto-satisfy it (it is human-decided by construction).

**[Risk] Outcome delivery depends on `cross-session-messaging`** → Accepted; it is the prior change in the chain and provides the `SessionDelivery` channel. The poll tool is the fallback if delivery is disabled.

**[Trade-off] Non-blocking vs synchronous** → The agent must handle "result arrives later" (push or poll) rather than a synchronous return. Accepted — the alternative (blocking on human latency) breaks the MCP tool timeout.
