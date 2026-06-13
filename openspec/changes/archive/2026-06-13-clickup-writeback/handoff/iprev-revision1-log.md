# iprev — design Revision 1 (closure trigger re-anchor) — 2026-06-11

Evaluator: Claude (opus, adversarial codebase-verified). Scope: Revision 1 only
(ship-success + manual trigger, optional halves) + its blast radius; the rest of
the change kept its original APPROVED status.

## Round 1 — REVISE (4 findings)

1. **[critical] Status-picker read path did not exist.** `clickup_statuses` is
   written by `mirror::upsert_status` but has NO production reader; `TaskDetail`
   carries no statuses; no registered command exposes them. The plan said "read
   `clickup_statuses` — no live call" as if the path existed (the same class of
   falsified anchor Revision 1 was written to correct). Fix: new task 1.5 —
   `clickup_read_list_statuses(list_id)` mirror read ordered by `orderindex`,
   registered in lib.rs, consumed by the §6.1 picker + closure prompt + the 1.3
   server-side validation.
2. **[high] "ShipDialog already holds everything" overstated.** It holds
   `state.sessionId` + `ShipResult.pr_info` but NOT binding state; `findSession`
   in `stores/clickup.ts` is module-private. Fix: 5.2(a) + implementation anchor
   now require resolving via `workspacesAtom` + `clickupBindingMapAtom` +
   `resolveActiveClickUpTask` against the SHIPPED session's row (never the
   active session), exporting a by-id selector, and raising the offer
   before/alongside the success path's synchronous `close()`.
3. **[medium] ARCHITECT-BRIEF carried the falsified premises.** Fix: STALE
   NOTICE block at the top subordinating the brief to design.md (Revision 1 +
   Decisions 1/2).
4. **[medium] Manual verb key-binding unspecified** (single-letter namespace
   exhausted: S/W/P/B + A/C). Fix: 5.2(b) pins "plain `ToolbarAction` button,
   no contextual letter".

Non-blocking notes verified: `ShipResult { commit_hash, pr_info }` +
`PrInfo.url` real; `runCommitPushPr` is the only `ShipResult`-producing path
(push-only paths return a bool from `git_push` → correctly never offer);
`git_ship` is agent-agnostic (no agent branching); no path in the change
touches `~/.claude`, hooks, or agent settings; optional-halves contract
consistent across all artifacts; token boundary not contradicted.

## Round 2 — APPROVED

All four fixes verified landed and codebase-accurate (`clickup_statuses` has
`list_id`+`orderindex` per migration 015; `findSession` confirmed private;
S/W/P/B + A/C confirmed taken; stale-notice present). No new inconsistencies
across proposal/design/spec/tasks/implementation.
