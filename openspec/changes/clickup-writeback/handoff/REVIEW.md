# REVIEW — clickup-writeback

## Pre-build: iterative-plan-review (Claude evaluator, adversarial, codebase-verified)
2 rounds → **APPROVED**. R1: 12 findings [2 critical, 3 high] → R2: APPROVED with low/accepted residuals (documented in design Risks).

Round-1 criticals (resolved): (1) split-brain — optimistic value persisted to the durable mirror before API ack → moved optimism to a **frontend overlay**; the durable mirror is written only on ack (a crash loses at most an un-acked edit, never corrupts the mirror); (2) comments don't fit the field-keyed echo/rollback model and lack idempotency → a **separate post-once model**: insert-after-id, no auto-retry on ambiguous failure, re-fetch-before-retry, never optimistically rolled back. Round-1 highs (resolved): (3) UI-only security boundary on a full-access token → a **backend confirmation-token gate** as the sole entry for closure + comment writes + command-boundary validation (mirrors the agent-spawned-worktrees structural gate); (4) assumed per-field reconcile → echo/conflict read field values from the **fetched whole-task payload** (only "poller hands writeback the payloads" is required); (5) `date_updated` too coarse → demoted to a "task changed" trigger, echo/order by **value comparison**. Mediums: field-class conflict resolution (scalar LWW+warn / additive merge-no-false-warning), edge-triggered idempotent closure, TTL ≥ 2× poll, partial-closure irreversibility surfaced, type-correct custom-field writes, cross-change echo-before-assignment ordering regression test, prefill sanitization.

Round-2 residuals (documented, non-blocking): benign `recent_writes` crash-loss (one spurious self-notification, no corruption); value-equality echo can mask a coincidental same-value remote write; routine reversible writes token-gated only by server-side validation (accepted tradeoff, flagged for the security reviewer); comment echo-match could collapse two identical comments cosmetically. All in design § Risks.

## Post-build reviewers
_Placeholder — populated during Mode B execute (security escalation for the write paths + the token-gated closure; verify the gate is un-bypassable and echo-before-assignment ordering holds)._

---

# Build-time reviews (2026-06-11, post-implementation)

## Reviewer: security (opus, authoritative) — PASS

Findings (non-blocking):
1. [low] `sanitize_comment_text` neutralizes ASCII `@`/`#` only; Unicode
   confusables (U+FF20/U+FF03) pass. Not a live vector (ClickUp's parser keys
   on ASCII) and `add_comment` hardcodes `notify_all: false` as second layer.
   Hardening backlog: NFKC-normalize before the scan (needs a new dep — deferred).
2. [low] Comment-length cap enforced at token issuance only — fine by
   construction (text frozen into the token; execute reads only the tuple).

Verified holds: token gate un-bypassable (UUIDv4 CSPRNG, single-use destructive
take under Mutex, expiry on take, `clickup_execute_closure` takes ONLY the
token, no non-token comment command registered); routine-write server-side
validation present (status-vs-List, computed-field reject in Rust); echo runs
before assignment detection with regression test; secret hygiene clean
(`set_sensitive(true)`, no token logging); no frontend auto-fire (all writes
behind click handlers; ShipDialog raises only the offer atom); agent-agnostic
(nothing touches ~/.claude, hooks, or agent branches).

## Reviewer: spec — PASS

Divergence notes (non-blocking):
1. Assignee UI is remove-only (no member directory in the mirror); API models
   add/remove diffs per spec. Spec delta amended to state the UI scope.
2. tasks.md checkboxes lagged the build — reconciled.
3. Detail comment composer routes through the closure-token path — consistent
   with Requirement 5 (observation, not divergence).
4. Uncertain-comment verify that finds the comment landed does not insert it
   into the mirror immediately (next poll reconciles). UX gap, not a spec
   violation — noted for the walk.

All six requirements verified implemented with file:line evidence; push-only
never offers; partial-closure surfacing correct; optional halves enforced.
