
## Frontend review

Scope: HintBar, Kbd size prop, and their use in six git views, PrViewer, and ConflictsPanel. Static review only, no rendered pass.

### Blocking

None.

### Important

- 🟡 `src/components/ui/hint-bar.tsx:9` — Hint labels use `text-muted-foreground/70` at 10px on `bg-card/95`. Contrast is likely under 4.5:1 for 10px text. Kbd chips add `/70` again. Raise label opacity to at least `/80`, or use plain `text-muted-foreground`.
- 🟡 `src/components/git/ConflictsPanel.tsx:1074-1075` — Two hints share the label "region". The first is navigate and the second is accept ours, theirs, or both. Rename the second to "ours/theirs/both" so the bar is not ambiguous.
- 🟡 `src/components/git/ConflictsPanel.tsx:1058` — The intent row still says "Ctrl+Shift+R sends" while the HintBar also lists `ctrl+shift+r ask agent`. This is a duplicated hint. Drop the intent-row text and keep the bar.
- 🟡 `src/components/ui/kbd.tsx:73` — `aria-label` is `Shortcut: arrowup`, `Shortcut: ctrl+arrowleft`. HintBar now renders many chips, so screen readers hear raw key ids repeatedly. Label with the formatted text, for example "Shortcut: Ctrl+←".

### Nits

- 🟢 `src/components/git/PrViewer.tsx:1160` — The `m merge` hint duplicates the Merge button chip `M` right below it. Keep one. The `Space` collapse action in `prViewerKeys.ts` has no hint.
- 🟢 `src/components/git/chips/StashesChip.tsx:423-429` — The bar repeats the Apply, Pop, Drop, and Branch chips already inside each expanded stash row. Same for `backspace back` in `PrsChip.tsx:316` versus the header chip at line 227.
- 🟢 `src/components/git/PrViewer.tsx:1156` — The body HintBar sits above the footer, so the footer is the true bottom. This matches design.md §3.4 ("shrink-0 sibling after the scroll container"). Mention it in the doc if intended.
- 🟢 `src/components/git/PrViewer.tsx:1190` — Apply with Agent is disabled while `applying`, but the label does not change. Add a "Sending…" label or PulseDots, as ConflictsPanel does for Ask Agent.
- 🟢 `src/components/git/ConflictsPanel.tsx:~407` — The toast reads "Sent to agent" in lowercase, while the button says "Ask Agent" and the other toast says "Apply with Agent". Pick one casing.

### Checked and fine

- All six chips, PrViewer, and ConflictsPanel use `Hint`, so labels are 10px and chips are `size="md"` everywhere. No leftover 9px hint strips.
- Placement is correct. Files, History, and Branches put the bar as the last child of the scroll container. Prs, Stashes, and PrViewer use a `shrink-0` sibling after the scroll area. Pickers use sticky inside `overflow-y-auto`. Nothing is covered by a sticky bar.
- `flex-wrap` handles narrow widths. Merge button chip `M` is present. Ask Agent and Apply are disabled while sending, backed by refs against double sends.
- No stray "Claude" in the changed git components. Agent-facing copy says "Agent".
- design.md §3.4 and patterns.md §4 match the implementation.

Verdict: PASS

## Adversarial review

reviewer: adversarial · reviewed_at: 2026-11-24 · scope: `git diff main -- src src-tauri/src docs` plus the four untracked files.

### Diff scope

- Files changed: 17 tracked + 4 new · LOC: +244 / -147 tracked, +300 new.

### Pass 1 — Bugs

No Important bug found. Checked and sound:

- Leader guard. The global dispatcher resolves `leader m` with `preventDefault` (`useKeyboardShortcuts.ts:119-123`). The viewer checks `defaultPrevented` and `leaderPendingAtom` (`PrViewer.tsx:662`). Either listener order is covered.
- Merge confirm. `requestMergeConfirm` re-reads `stateRef`, `mergingRef` and the PR identity after the await (`PrViewer.tsx:640-643`). The PR list uses `--state all` (`ship_pr.rs:45`), so a merged PR re-renders with `MERGED`.
- Key repeat and double press. `e.repeat` returns null (`prViewerKeys.ts:41`), and `mergeConfirmOpenRef` blocks a queued second dialog.
- Apply and Ask guards. Both refs are set synchronously and cleared in `finally`. `applyRef` removes the stale closure in the `Ctrl+Shift+R` listener.
- Rust. `deliver_prompt` takes every lock inside sync helpers. No guard lives across the `.await`, so the future is `Send`. Paste and submit errors both propagate.

### Pass 2 — Security

No finding. `deliver_prompt_to_session` refuses `::` ids before any write. It strips embedded paste markers through `write_bracketed` (`pty.rs:2052`). The confirm body escapes `baseRefName` (`PrViewer.tsx:631`).

### Pass 3 — Compliance

- [Important] `src/components/git/chips/FilesChip.tsx:332`, `HistoryChip.tsx:194`, `BranchesChip.tsx:462` — These bars sit inside the `flex-1 overflow-y-auto` list. `sticky bottom-0` only pins when the list overflows. With a short list, the bar renders right under the last row, mid-view. The requirement "Git panel hint bars share one standard" says the row is pinned to the bottom of its view. PRs, Stashes and Conflicts do pin, so the views still differ. `docs/design.md:310` blesses both placements. → Render these three bars as a `shrink-0` sibling after the scroll container, as `PrsChip.tsx:313` does. Add a short-list case to walk 6.4.
- [Important] Scenario "PR merged while the confirmation is open" (git-panel-v2) — not found. The code path exists (`PrViewer.tsx:642`). No test covers it, and no walk task (6.3–6.7) names it. → Add it to walk 6.3, or extract the post-confirm check into a tested helper.
- [Important] Scenario "Displayed PR changes while the confirmation is open" (git-panel-v2) — not found. The code path exists (`PrViewer.tsx:641`). No test or walk task covers it. → Same fix as above.
- [Nit] `docs/patterns.md:418` — The §8 contract says bare-letter verbs bail on Shift. `A` and now `M` accept Shift (`prViewerKeys.ts:41`). The contract also omits the new leader guard. ConflictsPanel `O` still races `leader o` (Toggle Ports) the same way. → Add the `defaultPrevented` / `leaderPendingAtom` guard to the §8 contract and state the Shift rule.
- [Nit] `src/components/git/PrViewer.tsx:1250` — The Merge button shows the `M` chip. Clicking it merges at once when no annotations exist. Pressing `M` always asks first. One chip now names two flows with different safety. → Accept as designed (D1), or note in the tooltip that `M` asks to confirm.
- [Nit] `src/components/git/PrViewer.tsx:1156-1161` — The PR viewer bar always lists `M merge` and `a annotate`. On a merged PR, `M` is inert. With no hunks, `a` is inert. The bar advertises keys that do nothing. → Render the `M` hint only when `state === "OPEN"`, and `a` only with hunks.
- [Nit] `docs/shortcuts.md:139` — The diff moves a vault path. It has nothing to do with FRI-609. → Commit it separately, or drop it from this change.
- [Nit] `src-tauri/src/pty_deliver_prompt_tests.rs:96-103` — The happy-path test reads one concatenated buffer. It cannot see that the CR is a separate write. It does not pin the 250 ms constant. The scenario "Prompt delivery submits after the paste" claims both. → Record write boundaries in `CapturedWriter`, and assert `PASTE_SUBMIT_SETTLE == 250 ms`.

Acceptance criteria not listed above have an implementing path and a test, or a named walk task. I treated carry-over scenarios whose text only swapped "Claude" for "Agent" as covered by walk 6.6 and 6.7.

### Harness entropy check

- Speculative abstractions: no. WHAT-comments: no new ones. New deps: none.

Verdict: PASS

## Resolution (orchestrator, 2026-11-24)

All Important findings are fixed in task group 7. Re-review was not run: the fixes are placement, labels and guards, each covered by a locked test or a walk step.

- Adversarial I1 (bars not at the bottom with a short list): fixed in 7.3. Every view bar is a `shrink-0` sibling after its scroll area; picker cards keep `sticky bottom-0`. Walk 6.5 checks a 2-file Files view.
- Adversarial I2 and I3 (post-confirm re-checks without coverage): fixed in 7.1/7.2. `mergeAfterConfirm` is pure and unit-tested for cancel, changed PR, changed workspace, merged state and a running merge.
- Frontend contrast: labels at full `text-muted-foreground` (7.3).
- Frontend duplicate "region" label and duplicate `Ctrl+Shift+R` hint: fixed in 7.4.
- Frontend raw `aria-label`: spoken form in 7.5.
- Nit, one chip with two safety levels: the Merge button now opens the same confirmation (7.2, design D1 revision 1).
- Nit, `M`/`a` hints shown when inert: hidden by state (7.2).
- Nit, patterns.md §8 contract and the `leader o` clash in Conflicts: contract updated; the Conflicts bare-key block bails on `defaultPrevented` or a pending leader chord (7.4, 7.6).
- Nit, the delivery test did not check a separate CR write or the 250 ms settle: added (7.1).
- Nit, Apply shows no sending state: "Sending…" (7.2).
- Nit, the PR viewer bar position undocumented: design.md §3.4 (7.6).
- Not changed, with reason:
  - `M` in the bar duplicates the button chip: the spec requires the PR viewer bar to list `M`.
  - Stashes and PRs bars repeat row or back keys: the standard lists each view's keys in its bar.
  - Toast "Sent to agent" casing: the text the user wrote in FRI-609.
  - `docs/shortcuts.md` path change: an unrelated user edit; it stays out of every commit.
