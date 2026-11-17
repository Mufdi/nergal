---
type: review
feature: session-resume-by-id
reviewer: adversarial + frontend (consolidated by the orchestrator)
reviewed_at: 2026-11-17
verdict: FAIL
---

# Review — session-resume-by-id

## Diff scope

- Files changed: 23 (without `docs/shortcuts.md`) · LOC: +1310 / -52

## Adversarial (opus) — FAIL (6 Important)

### Pass 1 — Bugs

- No Important bug in the core logic. Verified: `agent_shell_pid` never holds both locks (`src-tauri/src/pty.rs:157-160`); resume reads under the lock, file checks after it (`pty.rs:767-786`); `delete_session` checks before `stop_event_pump` and in both locked scopes; `cleanup_merged_session` refuses before side effects; a detached nested `claude -p` reads as nested.
- [Nit] `src-tauri/src/commands/workspaces_sessions.rs:718-723` — the final scope of `cleanup_merged_session` does not repeat `refuse_last_root_session`; a concurrent `delete_session(r2)` + `cleanup_merged_session(r1)` can remove both repo-root sessions. → Fix R-F4.
- [Nit] `src-tauri/src/hooks/server.rs:1018-1027` — a nested `Stop` still triggers `drain_idle`/`drain_worktree_outcomes`, pasting cross-session messages mid-turn. → Fix R-F5.

### Pass 2 — Security

- [Nit] `src-tauri/src/agents/claude_code/conversation.rs:12-14` — `is_valid_conversation_id` accepts a leading `-`, so an id shaped like a CLI flag reaches `claude --resume <id>` (needs a matching `.jsonl` and a same-uid payload). → Fix R-F6.

### Pass 3 — Compliance

- [Important] `openspec/specs/cc-adapter/spec.md:192` — "Zero regression for CC users" still names `claude --continue`. → Fixed in the delta (`specs/cc-adapter/spec.md`, MODIFIED).
- [Important] No test for "Nested Stop does not move the transcript marker" (`server.rs:903`). → R-T2.
- [Important] No test for the hook CLI attaching `agent_chain` only on `SessionStart`/`Stop` (`hooks/cli.rs:44-64`) nor for `agent_chain().complete` (`platform_proc.rs:502-538`). → R-T3, R-T4.
- [Important] No test for "the second fetch returns `[]`" (`lib.rs:1199`). → R-T5.
- [Important] No test for the frontend reload + toast on a refused delete (`src/stores/pendingDeletes.ts:139-147`), nor for the command-level refusals. → R-T6, R-T9.
- [Important] The 4 `cc-adapter` scenarios are vacuous on a runner without `claude` on `PATH` (`adapter.rs:417`, `if let Ok(spec)`). → R-T1.
- [Nit] `src-tauri/src/lib.rs:1218` — `reconcile_worktrees` stats each path with the DB guard held (`db-lock-discipline`). → Fix R-F7.
- [Nit] Stale comments: `src/components/layout/Sidebar.tsx:609-615` (`--continue` equivalents), `src-tauri/src/platform_proc.rs:499-501` and `implementation.md` (claim the hook blocks the agent; `SessionStart`/`Stop` are `is_async: true`, `src-tauri/src/setup.rs:12, 86`), `adapter.rs:178` ("--resume/--continue"). → Fix R-F8.

## Frontend (sonnet) — FAIL (2 Important)

- [Important] `src/stores/pendingDeletes.ts:130-147` — after a refused delete only `workspacesAtom` comes back; the tab id and the active session are gone and the terminal was destroyed. → Fix R-F1 (restore the tab and, when it was active, the active session; the terminal relaunches on selection).
- [Important] `src/stores/pendingDeletes.ts:137` — if `get_workspaces` also fails the session stays hidden. → Fix R-F1 (reinsert at `insertAt`, as Undo does).
- [Nit] `src/stores/hooks.ts:481` — the `/* tolerate failure … */` comment restates the empty catch. → Fix R-F3.
- [Nit] `src/stores/hooks.ts:477` — "x now run…" is ungrammatical for one name; "their own conversation" is internal jargon. → Fix R-F2.
- [Nit] `src/lib/sessionDelete.test.ts` — no case for a workspace with a single repo-root session and nothing else. → R-T8.
- Verified: the sidebar `d` shortcut only triggers the rendered delete button, so keyboard users cannot reach a refused delete; `setupHookListeners` runs once per mount; no unhandled rejection; no layout shift.

## Harness entropy check

- Speculative abstractions: no · WHAT-comments: one (R-F3) · New deps: no · Cross-platform: `sysinfo` only, no ungated unix seam.

## Verdict

FAIL — core logic matches the design and specs, but 8 Important findings (6 compliance/test gaps, 2 frontend restore gaps) must close before B7. Fix group 7 in `tasks.md`.
