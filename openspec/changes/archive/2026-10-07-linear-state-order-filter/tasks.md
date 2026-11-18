## 1. Name-first state order

- [x] 1.1 Test (Red): `src/lib/linearStateOrder.test.ts` covers group order, Duplicate vs Canceled by name, Pending with In Review, type fallback, no interleaving, position ties, no-state last
- [x] 1.2 Add `src/lib/linearStateOrder.ts` with `stateRank`, `isClosedState` and `compareStates`
- [x] 1.3 Replace the type-only `STATE_TYPE_RANK` sort in `LinearPanel.tsx` with `compareStates` for state groups and for issues inside project, assignee and cycle groups
- [x] 1.4 Make the show-completed control classify closed states with `isClosedState`

## 2. State visibility dropdown

- [x] 2.1 Test (Red): `src/lib/linearStateOrder.test.ts` covers `normalizeStateName` and `stateFilterOptions` (dedup, rank order, unnamed states skipped); the `filterHiddenStates` tests were removed with the helper in 6.3
- [x] 2.2 Add `stateFilterOptions` and `normalizeStateName` to `src/lib/linearStateOrder.ts`
- [x] 2.3 Add `linearHiddenStatesAtom` to `src/stores/linear.ts`
- [x] 2.4 Add the header dropdown to `LinearPanel.tsx`, copying the label filter popover mechanics with `e.key` navigation

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
- [x] 5.2 Manual walk: hide Done and Backlog in the dropdown; their groups (State view) and sub-groups (Project, Assignee, Cycle) stay visible folded with counts; Arrow Right or click on a folded header peeks into it, and after a restart they are folded again; turn H off and check Done, Duplicate and Canceled fold while Pending stays unfolded; expand a parent with a Done child and check the child is listed; fold an open-state group, restart and check it stays folded

## 6. Walk fix: hide folds

- [x] 6.1 Test (Red): `src/lib/linearFold.test.ts` covers "Hiding a state folds its issues", "Show-completed off folds closed states and keeps Pending open", "Hidden states fold sub-groups", "Peek unfolds a hidden state for the session" and "Re-hiding clears the peek" (`foldedByStateFilter`, `isGroupFolded`, `withoutPeeks`)
- [x] 6.2 Add `src/lib/linearFold.ts` with `foldedByStateFilter`, `isGroupFolded` and `withoutPeeks`
- [x] 6.3 `LinearPanel.tsx`: stop removing hidden and closed states from the list, label and sub-issue pools; resolve every group and sub-group fold with `isGroupFolded`; header click and Arrow Left/Right on a filter-folded group toggle an in-memory peek; hiding a state, "Unfold all" and H clear the matching peeks; remove `filterHiddenStates`
- [x] 6.4 Copy: H tooltip and label "Expand completed / canceled (H)", trigger tooltip "States: N folded", dropdown action "Unfold all"
