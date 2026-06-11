# REVIEW — clickup-task-integration

## Pre-build: iterative-plan-review (Claude evaluator, adversarial, codebase-verified)
2 rounds → **APPROVED**. R1: 10 findings [4 high] (the evaluator read the real pty.rs and caught two anchors used wrong) → R2: APPROVED with residual cleanups, applied post-approval.

Round-1 highs (all resolved): (#1) send-as-prompt was modeled on `reinject_pinned_note`/`write_session_data` (raw write, no bracketed paste, no submit) which fragments multi-line content → switched to `terminal_paste` (`pty.rs:994`, bracketed `\x1b[200~`…`\x1b[201~`) + explicit `\r` submit; (#2) the mid-stream defer guard had no mechanism in any anchor → grounded on `SessionStatus` (`models.rs:8`): `Running` queues, `→ Idle` delivers (no dependency on the unimplemented cross-session-messaging mode-map); (#3) third-party ClickUp comments injected with vault-note trust → untrusted-data fence framing + explicit confirm-before-submit for the auto-submitting send-as-prompt; (#4) incoherent lock guidance ("release lock before composing" can't read the mirror) → compose inside the `db.lock()` closure per the `pty.rs:396-410` vault precedent. Mediums/lows: attrition order across all sections (not just comments), migration N+1-at-merge, spawn-worktree binds by default, bracketed-paste + defer unit tests.

Round-2 residuals fixed post-approval: scrubbed 3 stale `reinject_pinned_note` references (proposal ×2 + implementation execution-order #4) that contradicted Decision 5; specified the deferred-send flush wiring (drained by the status-transition site on `Running→Idle`, logged on enqueue + drain); documented queued-send-not-persisted and the attach residual injection risk as accepted.

## Revision 1 iprev (Claude evaluator, skeptic persona, adversarial codebase-verified) — 2026-06-10
6 rounds → **APPROVED** (5 adversarial rounds hit the cap with one blocking finding; the user authorized a round-6 confirmation, which verified the applied fix — the EOF purge instance-identity gate — against the codebase, re-validated the polarity table and capture feasibility, and returned VERDICT: APPROVED). Full transcript: `handoff/iprev-revision1-log.md`.

Trigger: build-time verification falsified the approved Decision-5 premise — `SessionStatus` is never written at runtime (`db.rs:402` zero callers; `server.rs:732` no-op), so the defer had no readiness signal. Round-by-round: **R1** 10 findings [2C] — the installed `UserPromptSubmit` hook (`inject-edits`) never touches the socket (the Running edge didn't exist for ANY agent); lost-wakeup race in the two-mutex design; interrupt + kill/respawn staleness; drain mechanics (`\r` outside brackets, deliver outside lock); drained delivery must not auto-submit; best-effort framing. **R2** 8 findings [1C] — Codex's `merge_cluihud_entries` deletes sibling entries per event (descoped to backlog); `setup_hooks`/`run_codex_setup` are dead code (no install path); bare-vs-wrapper insertion; strict `cluihud_session_id` keying; pop-under-lock invariant. **R3** 5 findings [1C] — artifacts on disk still encoded v1 (synced); guard-status must derive from runtime observation (`guard_verified`), not static settings.json. **R4** 4 findings [1H] — vault Backlog entry for the Codex follow-up didn't exist (written); purge rationale + cite fixes; stale-EOF race. **R5** 1 blocking [1H] — the absence-gate fix for the stale-EOF race would have no-opped the crash purge (crash doesn't de-register from `session_ptys`); replaced with the evaluator-prescribed **instance-identity gate** (reader captures own `pty_id`; purge iff absent-or-own).

Net design deltas vs the approved plan: `SendGate` single-Mutex struct (run_state + queued + guard_verified), NEW `UserPromptSubmit → cluihud hook send user-prompt` HookDef with wrapper-form synthesis (CC only; Codex descoped to vault Backlog), three teardown purges with instance-identity EOF gate, drained delivery without auto-submit + notification, cancel/deliver-now commands, outcome events, runtime-derived guard visibility in the send confirm, best-effort framing in design + delta spec. Full transcript: iprev plan/review files (session /tmp artifacts; history summarized here).

## Post-build reviewers (2026-06-10, escalated: security + spec in parallel)

### Reviewer: security — `SECURITY: PASS` (0 critical, 0 high, 2 medium, 2 low — all 4 applied)
Verified clean: 7-bit ESC sanitization on every PTY delivery path; `\r` outside brackets, immediate-only auto-submit (drain + force-deliver both `submit=false`); confirm dialog is the sole auto-submit gate; destructive pop prevents double-delivery; strict session keying; parameterized SQL; pinned-ids JSON garbage tolerance; no task-content leakage in logs; system-prompt-file path correctly NOT pty-sanitized; shell-escaping of initial_prompt; wrapper synthesis builds from a hardcoded const; all three teardown purges + instance-identity gate; migration 018 minimal/safe.
Findings applied post-review (orchestrator, builder hit session limit):
1. **[M] C1 controls** added to `sanitize_for_pty` drop arm (U+009B = 8-bit CSI could close the bracketed paste) + test `sanitize_drops_c1_controls`.
2. **[M] Hook socket 0600** after bind in `hooks/server.rs` (the new UserPromptSubmit arm elevates what a local spoofer could do to gate state).
3. **[L] Fence-sentinel collision**: `neutralize_fence_sentinels` mangles literal sentinels in user content before wrapping + test `fence_sentinel_in_comment_cannot_close_the_fence_early`.
4. **[L] `workspace_id` byte-slice** → char-safe `chars().take(6)` in `clickup_spawn_worktree_with_task` (the pre-existing twin in `commands.rs` `create_session` left untouched — out of scope, noted below).
Finding 2 of the review (OSC payload passthrough as plain text) assessed not-a-vector for bracketed-paste framing; accepted as documented best-effort.

### Reviewer: spec — `SPEC: FAIL` → blocker fixed → effectively PASS
Verified clean: every composition scenario (attrition order + markers + heading), all three verbs (send confirm + bracketed + no-binding; spawn binds + pending_prompts; attach via injected_context only), defer/send-gate (single Mutex, strict keying, no-auto-submit drain, cancel/deliver-now), purges with correct instance-identity polarity, guard visibility runtime-derived, binding 1:1 + ordered idempotent pins + dedupe, byte-identical spawn, `sessions.status` untouched, zero outward ClickUp writes, all [x] checkboxes real.
Blocker (1): the active-task chip didn't honor the Unsupported-injection scenario ("session UI SHALL indicate injection is unsupported") — fixed: `clickupChipTooltip` in `TopBar.tsx` consults `injectionTierMap[tabId]` and mirrors `pinnedChipTooltip`'s unsupported wording (tooltip-only degradation, matching the pinned chip's behavior exactly). tsc clean.

### Gates
clippy `-D warnings` clean · `cargo test --lib` 404 passed / 0 failed · `cargo fmt --check` clean · `npx tsc --noEmit` clean · scope 17 files vs estimate 12 = 1.42 < 1.5.

### Deuda anotada (follow-ups, no bloquean)
- Pre-existing `workspace_id` byte-slice in `commands.rs` `create_session` (same pattern as the one fixed here) — char-safe truncation pending.
- OSC payload passthrough in `sanitize_for_pty` (plain-text noise, not an injection vector) — optional state-machine hardening.
- Codex run-state edge (vault Backlog § "Codex run-state edge para el send-gate").
