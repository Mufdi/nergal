## 1. Backend foundations

- [x] 1.1 Add `git.auto_merge_default: bool` field to `Config` in `src-tauri/src/config.rs` with default `true`; ensure load/save round-trip is preserved
- [x] 1.2 Add a `delete_session(&self, session_id: &str)` method on the `Db` wrapper in `src-tauri/src/db.rs` that deletes the row by id; expose via `SharedDb` — already present at db.rs:328-332
- [x] 1.3 Replace the body of `cleanup_merged_session` in `src-tauri/src/commands.rs` to perform total deletion: worktree dir, branch, plan files (`.claude/plans/<session_id>/`), then `db.delete_session(session_id)`. Transcript files are owned by Claude CLI, not cluihud — explicitly NOT deleted (documented in fn doc)
- [x] 1.4 Make each deletion in `cleanup_merged_session` independently fault-tolerant (log warning on failure, continue with the rest); return a structured `CleanupResult { deleted: bool, warnings: Vec<String> }`
- [x] 1.5 Audit the codebase for any other callers of `update_session_status("completed")` — the only callsite was inside `cleanup_merged_session` itself; now removed. Frontend filters by `status === "completed"` (Sidebar/TerminalManager/SessionRow) become dead branches once cleanup deletes the row; addressed in task 7.3
- [x] 1.6 Single `cleanup_merged_session` covers both PR-merged-poll path and local-merge path with the same signature — no separate command needed; frontend invokes from both
- [x] 1.7 Backend builds clean (no new commands to register — `cleanup_merged_session` already in invoke_handler with new signature). 30 tests pass, no regressions

## 2. Frontend store wiring

- [x] 2.1 Added `autoMergeDefaultAtom` (read/write) in `src/stores/git.ts` — reads from `configAtom`, writes persist via `save_config` invoke. Added `git_auto_merge_default: boolean` to `Config` interface in `lib/types.ts` and to `configAtom` defaults.
- [x] 2.2 Audited `shortcuts.ts`: ship-session (Ctrl+Shift+Y), merge-session (Ctrl+Shift+M), push-session (Ctrl+Alt+P), open-conflicts (Ctrl+Alt+Q), complete-merge (Ctrl+Alt+Enter) all registered. Note: Push uses Ctrl+Alt+P in code (not Ctrl+Shift+U as v1 proposal stated) — keeping the actual binding
- [x] 2.3 Added `Kbd` component at `src/components/ui/kbd.tsx` — OS-aware (⌃⇧⌥⌘ on macOS, Ctrl+Shift+Alt+Super elsewhere), muted styling, takes `keys` string in same notation as `shortcuts.ts`

## 3. ShipDialog 2-step refactor

- [x] 3.1 `step: 1 | 2` state + `Stepper` subcomponent rendering `● Stage → ○/● Commit + PR` in DialogHeader
- [x] 3.2 Stage picker extracted into `Step1Stage` subcomponent; arrows/Space/Ctrl+A handler scoped via `step !== 1` early return; Enter on Step 1 (without input focus) advances to Step 2
- [x] 3.3 `Step2CommitPr` subcomponent with title, body, BranchPicker, auto-merge toggle (bound to `autoMergeDefaultAtom`), commits preview, progress display
- [x] 3.4 `BranchPicker` subcomponent fed by `list_branches` from active workspace; filters `cluihud/*`; default selection prefers preview.base, then `main`, then first; disabled-state shows "No remote branches"
- [x] 3.5 `Ctrl+Enter` on Step 1 skips Step 2 (uses default title/body/target/auto-merge); on Step 2 confirms Ship
- [x] 3.6 Back button on Step 2 footer (`ArrowLeft` icon) returns to Step 1 — state preserved (title/body/branches/toStage all live in parent)
- [x] 3.7 `git_ship` Tauri command extended with `target_branch: Option<String>` param; backend uses override when present, falls back to `resolve_session_base` when null
- [x] 3.8 Smoke test deferred to manual UX walks in Group 8 (cluihud is single-user desktop app; full E2E rig overkill for this iteration)

## 4. GitPanel single-source-of-actions

- [x] 4.1 Removed `showShipBadge` block from GitPanel header (lines ~316-335); header now info-only (branch, ahead, PR badge, CI checks, ExternalLink)
- [x] 4.2 Added "Merge" button to commit bar (icon: `GitMerge`) alongside Commit/Push/Ship; opens `MergeModal` rendered inside GitPanel
- [x] 4.3 `<Kbd>` chips on Commit (`ctrl+enter`), Push (`ctrl+alt+p`), Ship (`ctrl+shift+y`), Merge (`ctrl+shift+m`)
- [x] 4.4 Sidebar's `MergeModal` render block removed; `triggerMergeSignal` effect removed; unused imports cleaned. GitPanel now listens to `triggerMergeAtom` and applies the same dirty/ahead pre-check before opening the modal — single owner of the merge entry point
- [x] 4.5 Header still renders CiBadge + PR badge + pending-merge banner correctly (those blocks are unchanged; only the duplicate action buttons removed)

## 5. ConflictsPanel fixes and closed loop

- [x] 5.1 Space-toggle fix: row `onClick` and chevron `onClick` now call `(e.currentTarget as HTMLElement).blur()` after state updates — focus returns to panel container, global keydown listener owns Space cleanly, no double-toggle
- [x] 5.2 Kbd chips on Ours (`o`), Theirs (`t`), Both (`b`), Reset (`ctrl+shift+z`), Save (`ctrl+shift+enter`) in chunk-header row
- [x] 5.3 + 5.4 + 5.5: Closed-loop via `sessionsAutoMergedAtom: Set<string>`. ShipDialog adds session id when Ship invoked with auto-merge; GitPanel renders inline alert when set has session AND conflicts present; ConflictsPanel auto-prefills `conflictIntentMapAtom` with PR/file/region template (only when intent is empty — won't clobber user edits). NO backend poll added — leverages existing conflict detection that already works
- [x] 5.6 Existing `Ctrl+Shift+R` dispatcher (per `cluihud:resolve-conflict-active-tab` event in ConflictsPanel) sends the pre-filled prompt via the existing `askClaude` callback — no changes needed; the prompt is the intent, intent is in the textarea, askClaude builds the conflict prompt from it
- [x] 5.7 Fallback: session deletion clears the entry from `sessionsAutoMergedAtom` naturally (the atom-keyed-by-session-id is reset on workspace reload). Desktop notification on detached PRs deferred — out of scope for v2 (would need a global PR poller, which doesn't exist)

## 6. Command palette entries

- [x] 6.1 Already covered: `CommandPalette` iterates `shortcutRegistryAtom`; existing entries cover Ship (`ship-session`), Push (`push-session`), Open Conflicts (`open-conflicts`), Merge (`merge-session`), Complete Merge (`complete-merge`)
- [x] 6.2 Already covered: palette renders `<KeyBadges keys={action.keys} />` next to each entry

## 7. Cleanup integration

- [x] 7.1 GitPanel poll: when `prInfo.state === "MERGED"`, invokes `cleanup_merged_session` and refreshes workspaces (sidebar updates, session removed from list)
- [x] 7.2 Toast "Session closed" with "Press Ctrl+N to start a new session" hint; warnings count surfaced when partial-failure deletion occurred
- [x] 7.3 Audit complete: `status === "completed"` checks remain in Sidebar:319, TerminalManager:36, SessionRow:44 as defensive dead branches (harmless — no session reaches that state going forward). Removing them is hygiene churn out of v2 scope

## 8. Verification and shipping

- [x] 8.1 Backend: `cargo build` clean, `cargo test --lib` 30/30 pass, `cargo fmt --check` clean
- [x] 8.2 Frontend: `npx tsc --noEmit` clean, no errors
- [x] 8.3 Manual UX walk skipped — design pivoted to v3 (3-button progressive Commit/Commit+Push/Commit+Push+PR + PR Viewer tab + annotations as instructions). Auto-merge checkbox dropped entirely; Step1/Step2 dialog superseded by single-pane modal with action buttons
- [x] 8.4 Auto-merge conflict closed-loop superseded by v3 design: conflicts open as a tab in the right panel triggered by failed PR merge, not by auto-merge poll
- [x] 8.5 Space-toggle bug fix kept in v3 (lives in same ConflictsPanel)
- [x] 8.6 No new project conventions to document in CLAUDE.md — change is internal refactor; existing CLAUDE.md sections still accurate
