## 1. Backend — worktree primitives

- [x] 1.1 Add `push(cwd, branch)` in `src-tauri/src/worktree.rs` invoking `git push -u origin <branch>`, returning `Result<(), anyhow::Error>`
- [x] 1.2 Add `pr_preview_data(cwd, base, head) -> PrPreviewData` returning `{ base, commits: Vec<{hash, subject}>, diffstat: {added, removed, files}, template: Option<String> }` — read commits from `git log <base>..<head> --format=%h%x00%s`, diffstat from `git diff --stat <base>..<head>`, template from `.cluihud/pr-template.md` at repo root; dedupe commit subjects
- [x] 1.3 Add `pr_checks(cwd, pr_number) -> PrChecks` invoking `gh pr checks <n> --json state,conclusion` and aggregating `{ passing, failing, pending, total }`
- [x] 1.4 Add `gh_available() -> bool` checking `gh auth status` exit code
- [x] 1.5 Add `conflicted_files(cwd) -> Vec<String>` parsing `git status --porcelain` for entries matching `UU`, `AA`, `DD`, `AU`, `UA`, `UD`, `DU`
- [x] 1.6 Add `file_conflict_versions(cwd, path) -> ConflictVersions` reading stages via `git show :1:<path>` (base), `:2:<path>` (ours), `:3:<path>` (theirs) plus working copy for merged
- [x] 1.7 Add `ship(cwd, branch, base, commit_message: Option<String>, pr_title, pr_body)` composing: if `commit_message.is_some() && staged > 0` call `commit`; always call `push`; call `create_pr`; return `ShipResult { commit_hash: Option<String>, pr_info: PrInfo }`; emit progress via a callback/event parameter

## 2. Backend — Tauri commands

- [x] 2.1 Add `git_push(session_id)` command in `src-tauri/src/commands.rs` resolving session cwd + branch and calling `worktree::push`
- [x] 2.2 Add `git_ship(session_id, message: Option<String>, pr_title, pr_body) -> ShipResult` command emitting `ship:progress` events per stage via `AppHandle::emit`
- [x] 2.3 Add `get_pr_preview_data(session_id) -> PrPreviewData` command (resolves base from list_branches like existing `create_pr`)
- [x] 2.4 Add `poll_pr_checks(session_id) -> Option<PrChecks>` command (returns None if no PR or PR not OPEN)
- [x] 2.5 Add `gh_available() -> bool` command
- [x] 2.6 Add `get_conflicted_files(session_id) -> Vec<String>` command
- [x] 2.7 Add `get_file_conflict_versions(session_id, path) -> ConflictVersions` command
- [x] 2.8 Add `save_conflict_resolution(session_id, path, merged: String) -> Vec<String>` command — writes merged content, stages the file via `git add`, returns updated conflicted files list
- [x] 2.9 Extend `set_pending_annotations` (or add `enqueue_conflict_context`) to accept `{ path, ours, theirs, merged, instruction }` payload appended to the next prompt via the existing `inject-edits` hook
- [x] 2.10 Register all new commands in `src-tauri/src/lib.rs` invoke_handler
- [x] 2.11 Add `cargo clippy -- -D warnings && cargo test` gate — fix any warnings introduced

## 3. Frontend — state and atoms

- [x] 3.1 In `src/stores/rightPanel.ts`, extend `Tab["type"]` union with `"conflict"` and extend tab `data` shape to optionally carry `{ path: string }` for conflict tabs
- [x] 3.2 In `src/stores/git.ts`, add `conflictedFilesMapAtom: atom<Record<string, string[]>>({})` keyed by sessionId and derived `activeConflictedFilesAtom`
- [x] 3.3 In `src/stores/git.ts`, add `prChecksMapAtom: atom<Record<string, PrChecks | null>>({})` and derived `activePrChecksAtom`
- [x] 3.4 In `src/stores/git.ts`, add `refreshConflictedFilesAtom` write-only atom invoking `get_conflicted_files`
- [x] 3.5 Add `shipDialogAtom: atom<{ open: boolean; sessionId: string | null }>({ open: false, sessionId: null })` in a new `src/stores/ship.ts` plus a `triggerShipAtom` signal atom
- [x] 3.6 Add `triggerPushAtom` signal atom in `src/stores/shortcuts.ts` (keeping pattern consistent with existing trigger atoms)

## 4. Frontend — keyboard shortcuts

- [x] 4.1 Add `ship` entry (`ctrl+shift+y`, `category: "action"`, keywords `["ship","pr","push","commit","deploy"]`) to `shortcutRegistryAtom`
- [x] 4.2 Add `push` entry (`ctrl+shift+u`, `category: "action"`, keywords `["push","upload","remote"]`) that invokes `git_push` and shows toast if `ahead === 0`
- [x] 4.3 Convert the existing `revise-plan` entry handler into a contextual dispatcher reading `activeTabAtom` + `activeConflictedFilesAtom` and emitting one of `cluihud:resolve-conflict-active-tab`, `cluihud:open-first-conflict`, `cluihud:revise-plan`; update label to "Revise Plan / Resolve Conflict (contextual)" for the command palette

## 5. Frontend — Ship dialog + PR preview

- [x] 5.1 Create `src/components/git/ShipDialog.tsx` — shadcn Dialog with editable title/body textareas, read-only commits list, diffstat summary, Ship/Cancel buttons, inline warning when `gh_available` is false
- [x] 5.2 On mount, call `gh_available` + `get_pr_preview_data`; on open via `shipDialogAtom`, prefill fields
- [x] 5.3 Confirm action calls `git_ship` with dialog title/body and textarea message (if Ship was triggered from textarea path); listen to `ship:progress` events to update step-by-step progress UI; on completion, set `prInfo`, close dialog, show toast with PR URL
- [x] 5.4 Mount `ShipDialog` at `Workspace.tsx` level so it's available regardless of which panel is open
- [x] 5.5 Wire `triggerShipAtom` effect in `Workspace.tsx` to open the dialog (shortcut path)

## 6. Frontend — GitPanel changes

- [x] 6.1 Add Ship button and Push button to the commit bar (bottom) in `GitPanel.tsx`; replace prior "Create PR" button with Ship button that opens the dialog
- [x] 6.2 Add Ship-it badge to panel header: visible when `ahead > 0 && !prInfo && !committing`; includes Ship + Push actions
- [x] 6.3 Add CI status indicator next to PR pill: fetches via `poll_pr_checks` every 20s while panel mounted and `prInfo.state === "OPEN"`; clears interval on unmount + session switch
- [x] 6.4 Add conflict list section at top of panel: fetches via `refreshConflictedFilesAtom` on mount + on `files:modified`; renders "Conflicts (N)" header + rows with Resolve buttons that call `openTabAction` with `{ type: "conflict", data: { path } }`
- [x] 6.5 Add `onKeyDown` handler to commit textarea for `Ctrl+Shift+Enter` → fire Ship via `triggerShipAtom`; if textarea message non-empty and staged > 0, pass message inline so dialog skips to PR confirmation step
- [x] 6.6 Wire existing `files:modified` listener to also refresh conflicted files list

## 7. Frontend — ConflictTab component

- [x] 7.1 Create `src/components/git/ConflictTab.tsx` rendering 3-column layout (ours | theirs | merged) using CodeMirror 6 editors
- [x] 7.2 On mount, call `get_file_conflict_versions(sessionId, path)` and populate editors; merged is editable, ours/theirs are read-only with conflict-marker highlighting
- [x] 7.3 Implement toolbar actions: Accept Ours (copy to merged), Accept Theirs (copy to merged), Ask Claude (calls backend to enqueue conflict context + focuses terminal), Save Resolution (calls `save_conflict_resolution`, closes tab, refreshes conflicted files)
- [x] 7.4 Add Expand button that opens Zen Mode with `mode: "conflict"` prop passing the same 3-panel layout
- [x] 7.5 Register `conflict` tab type in `RightPanel.tsx` routing so `<ConflictTab>` renders when active tab type is `conflict`
- [x] 7.6 Listen to `cluihud:resolve-conflict-active-tab` custom event to trigger Ask Claude action from contextual shortcut

## 8. Frontend — Zen Mode extension

- [x] 8.1 Extend `ZenMode` (`src/components/zen/ZenMode.tsx`) to accept a `mode: "diff" | "conflict"` prop (and corresponding atom-driven `zenModeAtom` payload shape)
- [x] 8.2 When `mode === "conflict"`, render the 3-panel conflict layout at full-screen dimensions, sharing state with the source `ConflictTab` via an atom (so edits persist)

## 9. Sidebar and SessionRow — PR replaces Merge

- [x] 9.1 In `src/components/session/SessionRow.tsx`, replace the Merge hover-action with a Ship/PR hover-action that sets active session and opens the Ship dialog
- [x] 9.2 Move Merge to an overflow (three-dot) menu in the session row, preserving its existing `check_session_has_commits` precondition
- [x] 9.3 Keep `Ctrl+Shift+M` binding unchanged and the command-palette entry for Merge (no change needed in registry)

## 10. Hook integration — Ship-it badge observability

- [x] 10.1 Confirm `files:modified` event fires after `/commit` skill runs (Claude-driven commits via Bash `git commit`); if not fully reliable, add a lighter poll in GitPanel (every 10s when panel mounted and `committing === false`) to keep the badge responsive. Trade-off accepted per design doc.

## 11. Verification

- [x] 11.1 Run `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` — tests pass (26/26), cargo check passes, cargo fmt --check passes; clippy has 18 pre-existing warnings unrelated to ship-flow (none in new code)
- [x] 11.2 Run `npx tsc --noEmit` from project root — passes clean
- [x] 11.3 Manual E2E: stage file → Ctrl+Shift+Enter in textarea → dialog confirms → PR created → CI indicator appears — verified in production usage; refinements rolled into ship-flow-v2
- [x] 11.4 Manual E2E: trigger conflict via `merge_session` against a known conflicting branch → conflict list appears in git panel → Resolve opens ConflictTab → Accept Theirs + Save → conflict list empties — verified; space-toggle bug deferred to ship-flow-v2
- [x] 11.5 Manual E2E: Claude runs `/commit` → Ship-it badge appears → click Ship → PR created — verified via real workflow
- [x] 11.6 Manual E2E: Ctrl+Shift+R in plan panel with pending review still revises; Ctrl+Shift+R in git panel with conflicts opens first conflict tab — verified; closed-loop refinement rolled into ship-flow-v2
