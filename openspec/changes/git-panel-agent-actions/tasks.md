## 1. Prompt delivery that submits

- [ ] 1.1 Test (Red): `src-tauri/src/pty_deliver_prompt_tests.rs` (`#[path]` module of `pty.rs`, `#[tokio::test]`): `deliver_prompt` on `PtyManager::new()` returns an error for an auxiliary shell id (`"sess::shell"`) and for a session with no PTY (`"sess-1"`), writing nothing. Happy path: a manager whose session `sess-1` maps to a real `PtyInstance` with a capturing `SharedWriter` (construction as in `pty_pixel_tests.rs:37-62`); `deliver_prompt(.., "x\ny", 10 ms).await` returns `Ok`, and once it resolves the buffer equals `ESC[200~x\nyESC[201~` followed by `\r`, with the `\r` as the last byte. The test reaps the spawned child as `pty_pixel_tests.rs:64-70` does.
- [ ] 1.2 Add `deliver_prompt` and the async command `deliver_prompt_to_session(session_id, text)` in `src-tauri/src/pty.rs` (paste, settle, CR, resolve after the CR, return any error); share the 250 ms settle constant with `paste_and_submit_settled`; delete `terminal_deliver`; swap the registration in `src-tauri/src/lib.rs`.
- [ ] 1.3 `src/components/git/PrViewer.tsx`: `applyWithClaude` → `applyWithAgent`, invoking `deliver_prompt_to_session` behind an `applyingRef` checked and set synchronously on entry (covers the stale `Ctrl+Shift+R` listener) plus an `applying` state for the button's `disabled`, both set after the early-return gates and cleared in `.finally()`; `onApply()` calls an `applyRef` refreshed every render; toast title "Apply with Agent"; button label "Apply with Agent (N)"; update the comments at `:264` and `:716`.
- [ ] 1.4 `src/components/git/ConflictsPanel.tsx` + `src/hooks/useConflictResolution.ts`: `askClaude` → `askAgent`, `onAskClaude` → `onAskAgent`, invoking `deliver_prompt_to_session` instead of `writeToSession(prompt + "\r")`; a synchronous `sendingRef` guard beside `sending`, both cleared in `finally`; focus moves to the terminal after it resolves; toast "Sent to agent".

## 2. Merge with M

- [ ] 2.1 Test (Red): `src/lib/prViewerKeys.test.ts` for `prViewerKeyAction(e, { pickerOpen, hunkCount, fileCount, canMerge })`: `KeyM` and `Shift+M` with `canMerge` → `"merge"`; `KeyM` with `canMerge: false`, with `repeat: true`, with Ctrl, Alt or Meta → `null`; `Ctrl+Enter` and `Meta+Enter` → `null`; `KeyA` → `"annotate"` with hunks and `null` without; `j`/`k`/`↓`/`↑` → hunk next/prev with hunks; `Space` → `"toggle-collapse"` with hunks, `null` with any modifier; `Ctrl+←/→` → file prev/next only with 2+ files; picker open: `Escape` → `"picker-close"`, `j`/`k` → picker next/prev with files, `Enter` → `"picker-pick"`, `KeyM` → `null`.
- [ ] 2.2 Add `src/lib/prViewerKeys.ts` with `prViewerKeyAction` and route the `PrViewer.tsx` keydown handler through it. The component keeps the annotation, dialog and in-field guards and returns first when `e.defaultPrevented` or `leaderPendingAtom` is set.
- [ ] 2.3 `"merge"` → `requestMergeConfirm()` per implementation.md (latest-value refs for `state`, `merging`, `performMerge` and `prIdentityRef` `{ workspaceId, prNumber }`; `mergeConfirmOpenRef`; `confirm()` "Merge PR #N?" with the target branch escaped by `escapeHtml` and the unresolved annotation count; confirm label "Merge"; re-check state and that `{ workspaceId, prNumber }` is unchanged after confirm; `ctx.canMerge` built from the refs). The Merge button chip becomes `keys="m"`.

## 3. Shared hint bar

- [ ] 3.1 Screenshot check: each git panel view against `Git panel hint bars share one standard` and `The PR viewer lists its keys` (walk 6.4).
- [ ] 3.2 `src/components/ui/kbd.tsx`: add `size?: "sm" | "md"` (`sm` = 9px default, `md` = 10px).
- [ ] 3.3 Add `src/components/ui/hint-bar.tsx` (`HintBar` + `Hint`) with the classes from design D2.
- [ ] 3.4 PR viewer: a `HintBar` below the diff with `j/k` hunk, `a` annotate, `Ctrl+←/→` file, `M` merge; the file picker hint moves to a `HintBar` at the picker bottom.
- [ ] 3.5 `FilesChip`, `HistoryChip`, `BranchesChip`, `StashesChip` and the `PrsChip` list footer use `HintBar`; drop the duplicate PRs header hint (`PrsChip.tsx:258-260`); Stashes hints move to the bottom of the view.
- [ ] 3.6 `ConflictsPanel.tsx`: the hint line becomes a `HintBar` at the bottom of the view, with chips and "ask agent"; the Conflicts file picker hint (`:1084-1090`) moves to a `HintBar` at the picker bottom; the inline hints at `:1057` and `:1456` move to 10px.

## 4. Agent labels

- [ ] 4.1 `ConflictsPanel.tsx`: "Intent for the agent (optional)", "Ask Agent", and the `{/* Intent + Ask Claude */}` comment.
- [ ] 4.2 `src/components/spec/SpecPanel.tsx`: `handleSendToClaude` → `handleSendToAgent`, label "Send to Agent".
- [ ] 4.3 `src/components/plan/PlanPanel.tsx`: "The agent will revise the plan"; `src/components/plan/PlanAnnotationToolbar.tsx`: "Waiting for the agent to revise...".

## 5. Docs

- [ ] 5.1 `docs/patterns.md` §4: hint bars use `HintBar` (sticky bottom, 10px labels, 10px `Kbd` chips, wraps); placeholders keep plain glyph text. Add `M` (confirm-gated) to the PrViewer bare-letter precedents (`patterns.md:96`, `:406`).
- [ ] 5.2 `docs/design.md` §3.4: document the `Kbd` `size` prop and `HintBar`.

## 6. Verification

- [ ] 6.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- [ ] 6.2 `~/.claude/skills/work/scripts/work-gates.sh main git-panel-agent-actions` passes; grep finds no `terminal_deliver`, `applyWithClaude`, `askClaude` or `handleSendToClaude` in `src/` or `src-tauri/src/`.
- [ ] 6.3 Manual walk: in the PR viewer of an open PR, `M` opens "Merge PR #N?"; Cancel merges nothing; holding `M` opens one dialog; `Ctrl+Enter` toggles the fullscreen terminal and opens no merge confirmation; `Ctrl+Space` then `m` opens no merge confirmation.
- [ ] 6.4 Manual walk: every git panel view (Files, History, Branches, Stashes, PRs list, PR viewer, PR file picker, Conflicts, Conflicts file picker) shows the same bottom hint bar with 10px chips; the PR viewer bar lists `j/k`, `a`, `Ctrl+←/→` and `M`; a narrow panel wraps the Branches bar.
- [ ] 6.5 Manual walk: `M` on a merged PR opens nothing; with focus in the terminal, `m` types into the terminal; with 2 annotations, the confirmation body says 2 annotations are unresolved.
- [ ] 6.6 Manual walk: annotate a PR hunk and press "Apply with Agent": the agent receives the multi-line prompt as one turn and starts without an Enter; a double click, or `Ctrl+Shift+R` twice in a row, sends it once. In a conflict, "Ask Agent" with an intent does the same.
- [ ] 6.7 Manual walk: the SpecPanel button reads "Send to Agent"; a plan Revise shows "The agent will revise the plan".
