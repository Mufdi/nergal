## 1. State order from data

- [x] 1.1 Test (Red): rewrite `src/lib/linearStateOrder.test.ts` for the data-only rule: team RR's nine states sort In Review, In Progress, Todo, Backlog, Triage, Done, Shipped, Duplicate, Canceled; a renamed started state with a higher position ranks first; open types sort by position descending and closed types ascending; an unknown type ranks after Triage and above closed; no-state issues last; `isClosedState` is true for `completed`, `duplicate`, `canceled` types and false for a `started` state named "Pending"; `stateFilterOptions` and `groupByState` follow the new order. Keep the normalize and dedup tests.
- [x] 1.1b Test (Red): ties by name, missing position as 0, `stateRank` equal per type, name-without-type as unknown open; `src/lib/linearFold.test.ts` replaces the Pending case (Shipped folds; a started "Pending" stays open).
- [x] 1.2 Rewrite `stateRank` (type rank only; `cancelled` kept as a type spelling; no type with a name = unknown open; nothing = last), `compareStates` (type rank, then position desc for open types and asc for closed, missing position = 0, then name ascending) and `isClosedState` (type rank) in `src/lib/linearStateOrder.ts` without `RANK_BY_NAME`; keep the exported API.

- [x] 1.3 Map the `duplicate` type to the closed glyph in `linearStateToIconType` (`LinearPanel.tsx`, `LinearTaskView.tsx`), as the glyph requirement says. Visual-only: checked in walk 2.2.

## 2. Verification

- [x] 2.1 `npx tsc --noEmit && pnpm test`; `~/.claude/skills/work/scripts/work-gates.sh main linear-state-order-from-data`.
- [x] 2.2 Manual walk: in the Linear panel grouped by state, the order is In Review, In Progress, Todo, Backlog, Triage, Done, Shipped, Duplicate, Canceled; with H off, Done, Shipped, Duplicate and Canceled fold; Duplicate shows the filled disc with an ✕.
