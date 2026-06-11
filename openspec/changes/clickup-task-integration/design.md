# Design — clickup-task-integration

## Context

`clickup-sync` established the mirror (task source) and the read-only panel (action surface). This change turns a task into something the agent acts on, reusing Nergal's existing context-injection machinery rather than inventing new plumbing. The load-bearing reuse:

- **Assembler** (`pty.rs:396-410`): on every spawn — fresh **and** resume — reads `session.pinned_note_paths`, assembles vault-note bodies, and sets `SpawnContext.injected_context`. Resume re-injection is automatic because both paths run here.
- **Adapter contract** (`agents/mod.rs:296`, `ContextInjection` at `:310`): each adapter folds `injected_context` via its best channel (`AppendSystemPromptFile` for Claude, `AppendSystemPrompt`/`PromptPreamble` for Pi/Codex/OpenCode, `Unsupported` otherwise). **No new variant needed** — ClickUp context is just more text in the same block.
- **`pending_prompts`** (`pty.rs:388`): a stashed prompt the adapter folds into the launch command so it submits on spawn (used by the deep-link `session/new`).
- **`reinject_pinned_note`** (`pty.rs:880`): re-reads a note and writes a single labeled block into the **live** PTY — the model for "send to active session".
- **`create_worktree(repo_path, slug)`** (`worktree.rs:261`): the user-initiated worktree machinery.

## Decision 1 — Three distinct verbs, not one

**Decision**: Expose send-as-prompt, spawn-worktree, and attach-as-context as three separate actions, because they express different intents.

- *Send as prompt* = imperative, "do this now" → live-PTY write to the active session (or initial prompt to a new one). One-shot, no persistent binding.
- *Spawn worktree* = "start a fresh session on this task" → `create_worktree` + `pending_prompts` with the task as initial prompt.
- *Attach as context (pin)* = referential, "keep this in mind" → folds into `injected_context`, rides spawn/resume.

**Alternatives considered**:
- *Single "work on task" action doing everything*: rejected — conflates imperative submission with standing context; the user sometimes wants a one-shot nudge, sometimes a fresh session, sometimes background context. Three verbs map to three real intents.
- *Only spawn-worktree*: rejected — forces a new worktree for every task touch, even a quick "look at this" on the active session.

## Decision 2 — 1:1 active binding + N pinned context

**Decision**: A session has at most **one active** ClickUp task (`active_clickup_task_id`, the write-back target) and **zero or more pinned** tasks (`pinned_clickup_task_ids`, context-only). Both active and pinned are injected; only the active task is the write-back subject and the session-tab indicator.

**Schema** (follows the `sessions` JSON-column precedent — `pinned_note_paths` 010, `launch_options` 011, `env_shells` 013):
- `active_clickup_task_id TEXT` — nullable; the single bound task.
- `pinned_clickup_task_ids TEXT` — nullable JSON array of task ids, ordered, idempotent (mirrors `pinned_note_paths` semantics exactly).

**Rebind rule**: binding a task when one is already active **replaces** it (the previous active task drops to neither active nor pinned unless separately pinned). The UI confirms the replacement. Rationale: a session works one task at a time for write-back; silent multi-active would make the closure ambiguous.

**Active ⊆ injected**: the assembler injects active ∪ pinned. If the active task is also in the pinned array, dedupe by id.

**Alternatives considered**:
- *Multi-active (several write-back targets)*: rejected for MVP — makes "which task does the closure act on?" ambiguous and multiplies write-back UI. Pinning already covers multi-task **context**; only write-back is constrained to 1.
- *No binding (write-back always re-selects)*: rejected — loses the loop closure (the session forgetting its task is exactly the mirror-not-integration failure this change exists to fix).

## Decision 3 — Context composition from the mirror

**Decision**: `compose_task_markdown(task_id)` reads the mirror (not a live call) and produces a labeled block: heading (name + status + url), description (markdown), subtasks (name + status), checklists (items + resolved), custom fields (name: value by type, skipping computed like `automatic_progress` or showing it read-only), comments (author + text, most recent N), attachments (as `- title (url)` links, never inlined binaries). Capped at a byte budget with a **defined attrition order across all sections** (round-1 #6), each step leaving a visible marker (no silent truncation): drop oldest comments first → then collapse checklists to counts → then collapse the subtask list to counts → and only as a last resort head/tail-truncate the description. The name/status/url heading is never dropped.

**Alternatives considered**:
- *Inject raw JSON*: rejected — noisy, token-wasteful, not how a human briefs an agent.
- *Live-fetch at compose time*: rejected — the mirror is the source of truth and is kept fresh by the poller; live-fetch would bypass it and add latency. (Heavy sub-data like comments is already lazily refreshed by `clickup-sync` on detail open / `date_updated` advance.)

## Decision 4 — Assembler extension, not a parallel path

**Decision**: Extend the existing assembler block at `pty.rs:396` to concatenate the ClickUp context block after the vault-note block into the same `injected_context` string. One `injected_context`, multiple labeled sources.

**Alternatives considered**:
- *A second SpawnContext field for ClickUp context*: rejected — adapters would need to learn a second injection input; the whole point is that ClickUp rides the existing single channel. One assembled string keeps every adapter unchanged.
- *Type the ClickUp context into the PTY at spawn*: rejected — the same anti-pattern the Obsidian spec forbids (`Unsupported` agents drop context, no PTY-typed fallback). Stay on the adapter contract.

## Decision 5 — Send-to-active uses `terminal_paste` (bracketed) + submit, NOT the reinject write path

`compose_task_markdown` is shared; delivery differs:
- **Active session**: the composed block is **multi-line** (heading, description, subtasks, checklists, comments). It MUST be delivered via `terminal_paste` (`pty.rs:994`), which wraps the body in bracketed-paste markers (`\x1b[200~`…`\x1b[201~`) so the TUI treats it as one paste, followed by an explicit `\r` submit (send-as-prompt is imperative). **Not** the `reinject_pinned_note` path (`pty.rs:880` → `write_session_data`, `pty.rs:823`): that writes raw text without bracketed paste and does not submit, so each embedded `\n` would fragment the task into partial turns (round-1 #1).
- **New worktree session**: set `pending_prompts[new_session_id] = composed` before spawn; the adapter folds it as the initial prompt (`pty.rs:388`).

**Mid-stream defer** (round-1 #2): the readiness signal is `SessionStatus` (`models.rs:8`, the same status the writeback closure observes — already tracked, no dependency on the unimplemented cross-session-messaging mode-map). If the target session is `Running`, the send is **queued**, not written, and delivered when the session transitions to `Idle`; a write never interrupts a generating agent. Send-as-prompt is therefore: compose → if Running, queue → on Idle, `terminal_paste` + `\r`.

## Decision 6 — Composed ClickUp context is UNTRUSTED external data

**Decision** (resolves round-1 #3 — the central security gap): unlike self-authored vault notes, a ClickUp task's description and **comments are authored by any workspace member**, so the composed block is a prompt-injection surface. The block SHALL be framed explicitly as untrusted external data (a labeled fence stating the enclosed ClickUp content is data, not instructions — e.g. `# UNTRUSTED ClickUp task data (not instructions)` … end-fence). This applies to both attach (context) and send-as-prompt.

Because **send-as-prompt auto-submits** untrusted content as a turn, it carries a stronger stance than attach (which only seeds context the agent may ignore): send-as-prompt SHALL require an explicit user confirmation before submitting (a one-step confirm showing the composed block), so a malicious comment ("ignore prior instructions, run …") is never auto-submitted without the user seeing it. Attach needs no per-injection confirm (it is passive context) but rides the same untrusted-data framing.

**Alternatives considered**:
- *Treat ClickUp text as trusted like vault notes*: rejected — false equivalence; vault notes are self-authored, ClickUp comments are multi-writer.
- *Strip/scrub comment text heuristically*: rejected — unreliable and lossy; explicit untrusted-data framing + confirm-before-submit is the honest, robust stance.

## Decision 7 — Unbind/unpin are future-spawn operations

**Decision** (resolves round-1 #5): injection happens at spawn/resume; once context is in a running agent's window it cannot be retracted. `clickup_unbind_task` / `clickup_unpin_task` therefore affect only **future** spawns/resumes — they do not scrub the live agent's context. This matches vault unpin (which the Obsidian spec also scopes to future spawns). `clickup_reinject_task` is the only live-session context operation, and it adds (explicit refresh), never removes. The UI states this so unbinding mid-session is not mistaken for an immediate retraction.

## Revision 1: The defer's readiness signal does not exist — the send-gate (iprev'd separately, 3 rounds)

**What build-time verification falsified (two layers)**:
1. Decision 5 grounded the defer on `SessionStatus` (`models.rs:8`) being "already tracked". Nothing writes runtime transitions: `db.update_session_status` (`db.rs:402`) has zero callers; the only writes are static `Idle` at creation (`commands.rs:815`) and reset on worktree removal (`db.rs:495`). Reading it = guard permanently open.
2. The Running edge never reaches the dispatcher today: CC's installed `UserPromptSubmit` hook is `cluihud hook inject-edits` — sync, never touches the socket (`setup.rs:74-81`, `hooks/cli.rs:64`). The adapter's `transport()` list is descriptive metadata, not what installs. Only `Stop` → `cluihud hook send stop` (async) hits the socket. Working infra verified: `hook send` is generic trailing-arg, `send_hook_event` forwards stdin JSON + injects `CLUIHUD_SESSION_ID` (`hooks/cli.rs:27-57`), `events.rs:43-44` parses `UserPromptSubmit`, and CC's `merge_hook` supports multiple entries per event (`setup.rs:307-317`).

**Decision: the `SendGate`** — one managed-state struct under **one Mutex** (two separate mutexes had a lost-wakeup race between check-Running/enqueue and set-Idle/drain):

```rust
struct SendGate {
    run_state: HashMap<String, RunState>,   // keyed STRICTLY by cluihud_session_id; absent = Idle
    queued: HashMap<String, QueuedSend>,    // one slot per session, replace semantics (log + event on replace)
    guard_verified: HashSet<String>,        // sessions whose Running edge was OBSERVED at runtime
}
```

- **Writers** (dispatcher arms, `hooks/server.rs`): `UserPromptSubmit` → Running + insert into `guard_verified`; `Stop` (both arms, `:681`/`:716`) → Idle + destructive **pop** of the queued send, delivered **outside the lock**. The gate skips events whose `cluihud_session_id` is `None` — never the `unwrap_or(session_id)` display fallback (`server.rs:664`/`:706`): external CC sessions must not create ghost entries.
- **Enqueue** (`clickup_send_task_as_prompt`): check-and-enqueue atomic under the gate lock. Running ⇒ queue (replace + log + `clickup:send-queued`); Idle ⇒ deliver (paste + `\r`).
- **Pop invariant**: every consumer — Stop drain, `clickup_force_deliver_queued_send`, `clickup_cancel_queued_send` — pops destructively under the same lock; at most one popper wins (no double-delivery); delivery always outside the lock (a wedged PTY writer must not block hook processing).
- **Delivery mechanics**: `pub(crate)` helper `paste_to_session(state, session_id, text, submit: bool)` extracted from `terminal_paste` (`pty.rs:994`) — bracketed-paste wrap; `\r` (when `submit`) written separately AFTER the closing marker (inside the brackets it is text, not submit; `terminal_paste` itself writes no `\r` today, `pty.rs:1028-1031`, and keeps that contract). The helper accepts **agent sessions only** — the aux-shell branch (`session_id.contains("::")`, `pty.rs:1010-1025`) stays in `terminal_paste`, excluded from the helper's contract. Dispatcher resolves `PtyManager` via `app.state()`. Write error (dead PTY) → drop + log + `clickup:send-dropped{reason}` — no retry, no silence.
- **Deferred delivery does NOT auto-submit**: immediate path = paste + `\r` (the user just confirmed, context fresh); drained path = paste WITHOUT `\r` + notification ("task prompt pasted — review and submit"). A drain firing while the user drafts in that terminal splices visibly instead of auto-submitting spliced text. Deliberate semantic split, reflected in the delta spec.

**Hook registration — CC only (scope decision)**:
- `setup.rs` HOOKS gains `{ event: "UserPromptSubmit", command: "cluihud hook send user-prompt", is_async: true }` alongside the existing sync `inject-edits` entry (CC's merge locates entries by command — `matches_hook_command`, `setup.rs:202-212`, called from `merge_hook`'s locate loop at `:257-268` — so the second entry pushes without touching the first).
- **Wrapper-form synthesis**: installed machines route hooks through `cluihud-conditional.sh` (sentinel-gated). The insertion path today always writes the bare command (`setup.rs:287-317`); a bare entry would spawn-and-fail on every prompt of every CC session outside cluihud. The insertion must synthesize the wrapper form when the target settings file already uses it: take an existing cluihud wrapper entry, strip its trailing hook args, append this entry's args. Deterministic rule: any existing cluihud entry in wrapper form ⇒ install wrapped; none ⇒ bare. This is a real mechanism with its own unit tests (wrapper synthesis, mixed forms, no-prior-entries), not a flag.
- **Codex is OUT of scope for the Running edge**: its `merge_cluihud_entries` (`codex/setup.rs:70-80`) retains per-pair against the current pass's command — a second pair for the same event deletes the entry the first pass kept (and the idempotency test stays green with the bug); `run_codex_setup` is dead code (re-export, zero callers); its payload parity with `events.rs` has never been observed live. Enabling Codex = merge rewrite to a per-event command-SET + a live install path + runtime payload verification → **backlog follow-up**, recorded in the vault Backlog. Until then Codex reads guard-open.
- **Pi / OpenCode**: no signal (Pi's jsonl tail translates Cost → `AgentStatus`, `jsonl_tail.rs:188-206` — the `:161` comment is stale). Guard-open.
- Guard-open = absent = Idle = deliver immediately = today's exact behavior. Degradation direction is safe by construction.

**Propagation + guard visibility (runtime-verified, not static)**: the real install path is the user running `cluihud hook setup` (`main.rs:99`); nothing reconciles hooks at app launch (`setup_hooks` Tauri command has zero frontend callers) and this change does NOT start auto-editing `~/.claude/settings.json` (product decision out of scope). Honesty mechanism: the send-confirm payload carries a `guard_active` flag derived from **runtime observation** — true only when the session is in `guard_verified` (its Running edge was actually seen). The static settings.json check (global file only — CC merges other layers; documented) is only a *hint* to choose the notice text: "run `cluihud hook setup` to enable" vs "restart this session to activate" (CC snapshots hooks at session start, so pre-setup sessions never emit the edge until relaunched). A session of an agent with no signal (Codex/Pi/OpenCode) always reads guard-inactive. Release notes instruct re-running setup (existing convention — `OBSOLETE_HOOKS` exists because the list has evolved before).

**Lifecycle cleanup — all three teardown classes purge the gate entry** (run_state + queued + guard_verified; a dropped queued send logs + emits `clickup:send-dropped` only when an entry was actually removed, so double-purge is a silent no-op):
1. **Kill/respawn**: `kill_session_pty` (`pty.rs:929`) — a respawn with the same session_id starts clean; a stale composed block can never drain into a fresh conversation.
2. **Clean end**: the `SessionEnd` dispatcher arm (`server.rs:586`).
3. **Crash**: the PTY reader's EOF branch — the codebase already identifies it as "a definitive session end that an abnormal exit may have hidden from the SessionEnd hook" and runs the Obsidian finalizer there (`pty.rs:194-200`); the gate purge hangs off the same branch. The agent-session EOF branch runs unconditionally after a kill too (the `still_registered` gate at `pty.rs:206-218` covers only the aux-shell emit) — the single-purge guarantee comes from the idempotent map-remove + emit-only-on-removal rule, not from any de-registration gate. **Kill→respawn race — instance-identity gate, NOT presence/absence**: `session_ptys` maps `session_id → pty_id` and crash/natural-exit do NOT de-register (only `kill_session_pty` does, `pty.rs:934`), so a presence gate kills the crash purge and an absence gate reintroduces the race. The reader closure captures its own `eof_pty_id` (it already captures `eof_session`); the EOF purge runs iff `session_ptys.get(&eof_session)` is **absent OR equals the reader's own pty_id**. Crash → maps to own → purge ✓; kill-no-respawn → absent → idempotent purge ✓; kill→respawn → maps to the NEW pty_id ≠ own → skip ✓ (a late EOF never clears a respawned session's fresh gate state).
4. **User interrupt (Esc) residual**: CC fires no hook on interrupt → Running can stay stale until the next completed turn (the queued send then drains on that Stop — eventual delivery, not loss). Surfaced + actionable instead of guessed: pending-send indicator with **cancel** and **deliver-now** actions; no TTL heuristics.

**Honest framing — the guard is best-effort for every agent, not an invariant**:
- *Signal-latency TOCTOU*: the Running edge rides hook → CLI process → socket (tens-to-hundreds of ms); an immediate send can race a just-submitted prompt.
- *Drain-vs-new-prompt window*: the drain pops under lock but delivers outside it; a new `UserPromptSubmit` can land in the gap → a drained paste can arrive at the start of the next turn (bounded by no-auto-submit).
- *Inter-connection ordering*: each hook invocation is its own `UnixStream` in its own `tokio::spawn` (`server.rs:205-260`); a Stop and the next UserPromptSubmit can process inverted → stale-Running (send waits a turn, surfaced) or one mid-turn paste (same class as the TOCTOU; strictly better than today's no-guard).

What the gate DOES guarantee: a queued send is never silently lost (every outcome emits an event: `clickup:send-queued`/`-delivered`/`-dropped{reason}`), never double-delivered (destructive pop under one lock), and never blocks forever without user-visible recourse (cancel / deliver-now).

- **Restart semantics**: map + queue + guard_verified die with the process together (one struct, one lifetime) — no cross-restart desync; consistent with the accepted "queued send not persisted" residual.
- **`sessions.status` column untouched**: this revision does NOT write the DB column. The writeback closure (change 3) observes `Completed`, which is also never written today — that is change 3's premise to revisit, noted there, not solved here.

**Alternatives considered**:
- *Persist via `db.update_session_status`*: rejected — zombie `Running` on crash blocks delivery forever without a boot reset; per-turn DB writes; persisted guard + in-memory queue desync across restarts.
- *Frontend-tracked run-state*: rejected — backend write-safety gate must not depend on the webview.
- *TTL on Running*: rejected — timers guess wrong both ways (long legit turns vs fast interrupts); surfaced pending-send + user actions instead.
- *FIFO queue of N sends*: rejected — invites stale multi-delivery into one turn; replace-with-log is right for a one-shot imperative prompt.
- *Launch-time auto-reconcile of settings.json*: rejected — auto-editing user config at every app start is a product decision beyond this change; runtime-verified visibility covers the safety need.
- *Static settings.json check as the guard indicator*: rejected — false-positives in the common case (CC snapshots hooks at session start; pre-setup sessions would read "active" while never emitting the edge) and for 3 of 4 agents (the file is CC's, the session may not be).

## Revision 2: untrusted framing softened to "task brief" (user decision, walk round 1)

The Decision-6 stance treated ClickUp content as a multi-writer prompt-injection surface ("untrusted data, not instructions" fence + alarm-toned confirm). The user's walk verdict (2026-06-11): their workspace is a private, team-authored space; tasks are approved work items and some legitimately ARE direct instructions — the alarm framing was wrong for the product. Changed: fence relabeled "# ClickUp task brief" (team-authored work item), confirm dialog reworded as a review step with neutral styling. **Retained unchanged**: `sanitize_for_pty` on every PTY delivery path, fence-sentinel neutralization, confirm-before-submit, the send-gate. The technical protections were never about distrusting the team — they protect terminal integrity and submission control.

## Risks

- **[Risk] Byte budget**: a task with a long comment thread or description could bloat `injected_context` → cap with the Decision-3 attrition order (comments → checklists → subtasks → description), each step marked, omissions logged.
- **[Risk] Migration-number race** (round-1 #7): this change and `clickup-sync` both register in `db.rs:132`. This change's migration is **N+1 relative to clickup-sync's**, reconciled at merge — not two independent "next free" grabs that could collide on a fresh DB.
- **[Risk] Column residue on revert**: the additive `ALTER TABLE … ADD COLUMN` is forward-safe but not cleanly reversible pre-SQLite-3.35 (no `DROP COLUMN`); reverting leaves harmless dangling columns. Noted, accepted (forward-only migration convention).
- **[Risk] Rebind data loss**: replacing the active task silently could surprise → UI confirms the replacement; the dropped task remains in the mirror (nothing destroyed).
- **[Risk] Stale injected context**: a task changes in ClickUp after a session spawned → resume re-injects current (automatic via `pty.rs:396`); for a live session, `clickup_reinject_task` offers explicit refresh (never auto, mirroring the Obsidian hot-reload rule).
- **[Risk] Agent `Unsupported` for injection**: per the Obsidian contract, the attach silently records but the session-tab chip indicates injection is unsupported for that agent; no PTY-typed fallback.
