## 1. Name-first state order

- [x] 1.1 Test (Red): `src/lib/linearStateOrder.test.ts` covers group order, Duplicate vs Canceled by name, Pending with In Review, type fallback, no interleaving, position ties, no-state last
- [x] 1.2 Add `src/lib/linearStateOrder.ts` with `stateRank`, `isClosedState` and `compareStates`
- [x] 1.3 Replace the type-only `STATE_TYPE_RANK` sort in `LinearPanel.tsx` with `compareStates` for state groups and for issues inside project, assignee and cycle groups
- [x] 1.4 Make the show-completed filter use `isClosedState` in the list pool and the sub-issue pool

## 2. State visibility dropdown

- [x] 2.1 Test (Red): `src/lib/linearStateOrder.test.ts` covers `filterHiddenStates` and `stateFilterOptions` (dedup, rank order, unnamed states skipped)
- [x] 2.2 Add `filterHiddenStates`, `stateFilterOptions` and `normalizeStateName` to `src/lib/linearStateOrder.ts`
- [x] 2.3 Add `linearHiddenStatesAtom` to `src/stores/linear.ts`
- [x] 2.4 Add the header dropdown to `LinearPanel.tsx`, copying the label filter popover mechanics with `e.key` navigation, and apply the filter to the list pool and the sub-issue pool

## 3. Verification

- [x] 3.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit && pnpm test`
- [ ] 3.2 Manual walk: open the Linear panel in State view and check the group order; hide Done and Backlog, close and reopen the panel, and check they stay hidden; toggle show-completed (H) and check Pending stays visible
