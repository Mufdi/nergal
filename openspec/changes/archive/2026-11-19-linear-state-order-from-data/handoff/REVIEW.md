# Review: linear-state-order-from-data

Scope: `src/lib/linearStateOrder.ts`, its tests, `linearFold.ts` callers in `LinearPanel.tsx`. Vitest on both lib test files passed (with cssColor: 48 tests).

## Bugs

### Critical
None found.

### Important
1. **Same-named states across teams order by arrival.** `groupByState` (`linearStateOrder.ts:101-115`) and `stateFilterOptions` (`:76-93`) merge by name and keep the first issue as `state`. If two teams give one name different `position` or `type`, the merged group's rank depends on which issue came first. The proposal claims order does not depend on arrival; that holds only for equal type and position. Today's mirror (FRI, OPS) has equal positions for shared names, so no live impact. Fix: keep the best rank (min compare) of the merged states, or document the assumption in the spec.

## Spec gaps
- Positions are per-team scales. In project, assignee and cycle groupings, `groupCompare` (`LinearPanel.tsx:417-420`) compares positions across teams, for example a custom started "QA" at 3 against "In Review" at 1002. The spec says nothing about cross-team scales.
- Tasks 2.1 and 2.2 are unchecked (expected until the walk).
- Spec says the glyph requirement covers `duplicate` as a distinct type. `LinearStatusIcon.tsx:59` handles it. `linearStateToIconType` in `LinearPanel.tsx:105-121` and `LinearTaskView.tsx:70-85` still map `duplicate` to `null`. Out of scope of this change but inconsistent with the new closed rule.

## Nits
- `compareStates` now breaks ties by name (`linearStateOrder.ts:53`). In non-state groupings, two different states with equal type and position never fall through to the active sort. This matches the spec; note it for anyone reading `compare(a, b)` at `LinearPanel.tsx:420`.
- `localeCompare` uses the runtime locale. Order for ties can differ per machine. Use a fixed locale or `<`.
- Comments: none are WHAT comments. Both new comments (`linearStateOrder.ts:18-19`, `:45-46`) explain a reason.

## Verified OK
- Mirror data matches the claim: `~/.config/nergal/nergal.db` stores Duplicate as type `duplicate` for FRI and OPS; Shipped is `completed` at 954.09; In Review is `started` at 1002. The new order gives In Review, In Progress, Todo, Backlog, Triage, Done, Shipped, Duplicate, Canceled.
- Placeholder states (`mirror.rs:80`) are type `backlog` with NULL position: rank 3, position 0. No crash path.
- Callers of the closed rule: H fold (`linearFold.ts:17`, `LinearPanel.tsx:477,514`), overdue (`:1520`), sub-issue done count (`:1524`). All go through `isClosedState`; Shipped now folds and counts as done, a started "Pending" stays open. Matches the spec.
- No state-name literals remain in the code; `normalizeStateName` only dedups and ties.

Verdict: spec-conformant and well tested; only the cross-team merge ordering is a latent issue.

## Resolution (main thread, 2026-11-18)

- Important (merged same-name states ranked by arrival order): **fixed**. `groupByState` and `stateFilterOptions` keep the best-ranked member as the group's representative. Regression test `linearStateOrderTeams.test.ts` (separate test-writer) fails 3 of 5 on the old code and passes now, verified by swapping the old code back temporarily.
- Spec gap (`duplicate` mapped to no glyph): **fixed**, mapped to the closed glyph in both panels (task 1.3).
- Nits accepted: name tie-break over the active sort between distinct equal-rank states; `localeCompare` on lower-cased names.
