# REVIEW — cross-session-messaging

## Pre-build: iterative-plan-review (Claude evaluator, adversarial, codebase-verified)
4 rounds → **APPROVED**. Trajectory: R1 16 findings [1 critical] → R2 6 NEW [3 high] → R3 1 residual [1 medium] → R4 APPROVED. Key outcomes: the `relayed_origin` provenance gate was dropped (cluihud cannot attribute a downstream action to a relayed message → labeling + kill-switch only); delivery is via the Stop-hook CLI stdout (socket is fire-and-forget) with an idle-transition drain owned by the mode-map writer; reach hop cap (new participants) separated from conversation length (msg_budget) so two-party dialogue isn't amputated; budget is count+wall-clock with an active sweeper (not tokens); `agent_consumed_at`/`human_seen_at` separated so the UI never cancels delivery.

## Post-build reviewers
_Placeholder — populated during Mode B execute (security escalation for delivery + non-authoritative phases)._
