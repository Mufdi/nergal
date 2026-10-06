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

## 3. Dropdown rows and state sub-groups

- [x] 3.1 Test (Red): `src/lib/linearStateOrder.test.ts` covers `groupByState` (rank order with closed last, same-name merge, No state last, empty input)
- [x] 3.2 Add `groupByState` to `src/lib/linearStateOrder.ts`
- [x] 3.3 Extract `GroupHeader` in `LinearPanel.tsx` and render dropdown options as header-style rows with hidden states dimmed
- [x] 3.4 Render state sub-groups in Project, Assignee and Cycle sections with collapsed keys `<sectionKey>|state:<name>` in the shared `collapsed` set

## 4. Persist folded groups

- [x] 4.1 Test (Red): `src/stores/linearPrefs.test.ts` (`boundRecent`) and `src/stores/linearPrefsAtoms.test.ts` (atoms read config, write only the changed key through `save_config`, bound collapsed groups); Rust `config.rs` round-trip and legacy-config tests, `config_misc.rs` partial-overlay merge test
- [x] 4.2 Add `linear_hidden_states` and `linear_collapsed_groups` to `Config` (`src-tauri/src/config.rs`, `#[serde(default)]`, frontend-owned) and to the frontend `Config` type and `configAtom` defaults
- [x] 4.3 Back `linearHiddenStatesAtom` and `linearCollapsedGroupsAtom` in `src/stores/linear.ts` with `configAtom`, persisting per key via `save_config`; remove the localStorage helpers (`hiddenStatesStorage.ts`) because WebKitGTK flushes localStorage lazily and lost writes on quit
- [x] 4.4 Bound the collapsed set to 500 keys with `boundRecent` in `src/stores/linearPrefs.ts`

## 4b. Persist show-completed

- [x] 4b.1 Test (Red): `linearPrefsAtoms.test.ts` (show-completed defaults true, reads config, writes only `linear_show_completed`); Rust `config.rs` default-true-when-absent and round-trip
- [x] 4b.2 Add frontend-owned `linear_show_completed` (`#[serde(default = "default_true")]`) to `Config`, the frontend `Config` type and `configAtom` defaults; back `linearShowCompletedAtom` in `src/stores/linear.ts` with `configAtom`, persisting only that key via `save_config`

## 5. Verification

- [x] 5.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit && pnpm test`
- [ ] 5.2 Manual walk: fold groups and sub-groups, restart the app and check they stay folded; open the Linear panel in State view and check the group order; in Project, Assignee and Cycle check the state sub-headers fold with click and Arrow Left/Right; hide Done and Backlog, close and reopen the panel, and check they stay hidden; toggle show-completed (H) and check Pending stays visible
