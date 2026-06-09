# Implementation — clickup-task-integration

Detailed plan mapped against the current codebase. No code — guides Mode B.

## Codebase anchors (validated)

- **Assembler**: `pty.rs:396-410` reads `session.pinned_note_paths`, resolves the vault root, calls `obsidian::pinned_notes::assemble_context(...)`, and sets `injected_context`. Runs on fresh **and** resume (same path) — this is where the ClickUp context block concatenates.
- **SpawnContext / contract**: `agents/mod.rs:296` (`injected_context: Option<&'a str>`), `ContextInjection` enum at `:310` (`AppendSystemPromptFile` / `AppendSystemPrompt` / preamble / unsupported). **No new variant** — ClickUp context is more text in the same string.
- **pending_prompts**: `pty.rs:388-392` — `state.pending_prompts.lock().remove(&session_id)` becomes `initial_prompt`. Set it before spawn for spawn-worktree-with-task (same as deep-link `session/new`).
- **Bracketed-paste write**: `terminal_paste` at `pty.rs:994` wraps text in `\x1b[200~`…`\x1b[201~` so a multi-line body is one paste; it does NOT submit. This is the send-as-prompt primitive (+ an explicit `\r` submit). Do NOT use `reinject_pinned_note` (`pty.rs:880`) / `write_session_data` (`pty.rs:823`): raw write, no bracketed paste, no submit — would fragment a multi-line task into partial turns.
- **Readiness signal**: `SessionStatus` (`models.rs:8`: `Idle`/`Running`/`NeedsAttention`/`Completed`) on the session row — the defer guard queues a send while `Running` and delivers on `Idle`. No dependency on the unimplemented cross-session-messaging mode-map.
- **Worktree**: `create_worktree(repo_path, slug)` at `worktree.rs:261` → `.worktrees/cluihud/{slug}`, branch `cluihud/{slug}`.
- **sessions JSON columns**: precedent `pinned_note_paths` (010), `launch_options` (011), `env_shells` (013) — `ALTER TABLE sessions ADD COLUMN … TEXT` holding JSON. Follow it for `pinned_clickup_task_ids`; `active_clickup_task_id` is a plain TEXT id.
- **Mirror**: from `clickup-sync` — `clickup/mirror.rs` read helpers feed `compose_task_markdown`.

## Migrations

This change's migration is **N+1 relative to `clickup-sync`'s** mirror migration — reconciled at merge, NOT two independent "next free" grabs (parallel worktrees could otherwise both grab the same integer → a registration-order collision that only surfaces on a fresh DB). One migration, two `ALTER TABLE sessions` columns, registered in `db.rs:132` in order after clickup-sync's.

## Execution order

1. **Migration + Session model**: add the two columns; extend `Session` struct + `find_session` row mapping to load `active_clickup_task_id` and parse `pinned_clickup_task_ids` (JSON). Without this the assembler can't see the binding.
2. **`clickup/integration.rs` composition**: `compose_task_markdown` + `assemble_clickup_context` reading `clickup/mirror.rs`. Pure functions, unit-testable over fixtures. Get the byte-budget + truncation right here (reuse the discipline from `obsidian::pinned_notes::assemble_context`).
3. **Assembler extension** (`pty.rs:396`): after the vault block, append the ClickUp block. Keep `None` when neither source has content (byte-identical spawn preserved). Lock pattern: `compose_task_markdown` reads the **mirror tables**, which need the db guard — follow the existing vault precedent at `pty.rs:396-410`, which composes **inside** the `db.lock()` closure. Do NOT "release the lock then compose" (the mirror read needs the guard; that guidance was incoherent — round-1 #4). If hold time is a concern, snapshot **all** needed mirror rows under the guard first, then drop it and format — never re-acquire the same `db` mutex (double-lock risk).
4. **Commands**: bind/unbind, pin/unpin, send-as-prompt, spawn-worktree-with-task, reinject. Register in `lib.rs`. Reuse `create_worktree` + `pending_prompts` (spawn-worktree). Send-as-prompt uses `terminal_paste` (`pty.rs:994`, bracketed) + `\r` submit — NOT the `reinject_pinned_note`/`write_session_data` raw path. `clickup_reinject_task` (context refresh, no submit) may mirror the reinject-style labeled write, since it seeds context rather than submitting a turn. **Deferred-send flush wiring**: the per-session queued send is drained by the same code path that writes `SessionStatus` (the mode-map writer / status-transition site) on the `Running → Idle` edge — identify or add that drain hook; log a marker on enqueue AND on drain (no silent failure).
5. **Frontend**: actions on panel rows + floating detail; session-tab active-task chip; rebind confirmation; binding/pin atoms.

## Reuse, don't reinvent

- Pin/unpin semantics: copy `pin_vault_note`/`unpin_vault_note` (ordered, idempotent JSON-array edits) — the ClickUp pin array is the same shape as `pinned_note_paths`.
- Re-inject: `clickup_reinject_task` mirrors `reinject_pinned_note` (explicit, never auto — same hot-reload rule).
- Worktree spawn: the user-initiated path, NOT the `agent-spawned-worktrees` context-bridge change (that is agent-initiated with a human gate; this is the user clicking "spawn worktree with task").

## Edge cases

- **Send-as-prompt mid-stream**: writing to a live PTY while the agent generates corrupts input. Guard = `SessionStatus` (`models.rs:8`): if `Running`, queue and deliver on `→ Idle`; never interrupt a running turn. Delivery is `terminal_paste` (bracketed) + `\r`.
- **Untrusted content**: ClickUp comments are multi-writer. The composed block is framed as untrusted data, and send-as-prompt (auto-submit) is gated behind an explicit confirm showing the block.
- **`initial_prompt` for `Unsupported`-injection adapters**: `initial_prompt` and `injected_context` are distinct `SpawnContext` fields; spawn-worktree relies on the adapter folding `initial_prompt` regardless of its injection tier. Verify per adapter (Claude/Codex/OpenCode/Pi) at build that `initial_prompt` folds even when `context_injection()` is `Unsupported`.
- **Column residue on revert**: additive `ADD COLUMN` is not cleanly reversible pre-SQLite-3.35 (no `DROP COLUMN`); reverting leaves harmless dangling columns (forward-only convention, accepted).
- **Queued send not persisted** (accepted): a send queued while `Running` lives in memory; a restart before the agent goes `Idle` drops it. Acceptable for a one-shot imperative prompt (the user re-triggers) — not guaranteed delivery; stated so it isn't mistaken for one.
- **Attach residual injection risk** (accepted): the untrusted-data fence is *framing*, not enforcement — for Claude, attach lands via `AppendSystemPromptFile` (authoritative), and attach has no per-injection confirm (only send-as-prompt does). The confirm gate protects the auto-submit path; attach's exposure is an explicitly accepted MVP residual, not neutralized by the fence.
- **Slug derivation** for spawn-worktree: derive from task name with the existing slug rules (diacritics stripped + timestamp, per the worktree-slug convention); collision-check before `create_worktree`.
- **Active task deleted/archived in ClickUp**: the binding id may dangle; the assembler skips a task id absent from the mirror (like a missing vault note is skipped) and the UI flags the stale binding.
- **Rebind**: replacing the active task must not touch the pinned array; the replaced task simply loses active status.
- **`automatic_progress` and other computed fields**: render read-only in the composed markdown; never present as writable (matters for the writeback change).

## Out of scope (later change)

- Any write to ClickUp (status/comment/checklist/field) and the write-back-on-done closure → `clickup-writeback`. This change keeps Nergal read-only toward ClickUp; it only feeds tasks INTO the agent and remembers the binding.
