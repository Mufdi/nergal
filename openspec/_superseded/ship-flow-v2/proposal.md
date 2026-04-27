## Why

After shipping v1 (`ship-flow` change, commit 0a8d347), real usage exposed friction: the Ship dialog crams 7 concerns into one pane (gh check, preview, staging, title, body, auto-merge, progress, error), the GitPanel duplicates Ship/Push buttons in two zones, conflict resolution has no closed loop with Claude, and the local-merge path opens from the Sidebar as a peer to Ship rather than a secondary action. There's also a Space-toggle bug in the ConflictsPanel region header (button focus + space double-activates) and shortcuts hide in `title` attributes (hover-only, never discoverable). This v2 reorganises the surface to match the user's actual workflow (worktree → commit → push → PR → auto-merge → if conflict, Claude resolves in the same session) and enforces keyboard-first discoverability across every action.

## What Changes

- **BREAKING** Sidebar no longer hosts Merge entry point — moved to GitPanel with its own visible button + `Ctrl+Shift+M` shortcut.
- **BREAKING** Session cleanup is now total deletion (DB row, worktree, branch, plan files, transcript) instead of `update_session_status("completed")` — matches Claude Code's own "close conversation" semantics; users wanted no orphan history.
- ShipDialog refactored into 2 explicit steps with stepper indicator: Step 1 = Stage picker (full-pane keyboard nav), Step 2 = Commit + PR (title, body, target branch picker, auto-merge toggle).
- Add **PR target branch picker** to Step 2 (dropdown sourced from existing `list_branches`, default `main`, filters `cluihud/*` worktree branches).
- GitPanel commit bar becomes the **single source** for Commit/Push/Ship/Merge — header bar removes its duplicate Ship/Push buttons (header keeps only passive info: branch, ahead, PR badge, CI checks).
- Conflict resolution closes the loop in the **same session**: when auto-merge poll detects a PR blocked by conflict, the GitPanel surfaces an inline alert + opens the Conflicts panel + pre-fills an Ask-Claude prompt with PR/file/diff context — user confirms with shortcut to actually send.
- Fix ConflictsPanel **Space-toggle bug** by blurring the row/chevron buttons after click so the global keydown listener owns Space cleanly.
- Every git action gains a visible `<kbd>` chip showing its shortcut (no more `title`-only hover discoverability). Applies to GitPanel commit bar, ShipDialog footer, ConflictsPanel actions (O/T/B/Reset/Save).
- Persist `git.autoMergeDefault` in config — the toggle initialises from config and writes back on change, removing the per-Ship reset friction.
- Command palette gains git entries: "Ship session", "Push", "Open conflicts", "Merge locally…".

## Capabilities

### New Capabilities
- `session-cleanup`: Total deletion of a session's persisted state (DB row, worktree, branch, plan files, transcript) when the session reaches a terminal state (PR merged + remote cleaned, or local merge confirmed). Toast confirms with CTA to create a new session.

### Modified Capabilities
- `ship-flow`: Refactored into a 2-step dialog with stepper, PR target branch picker, persisted auto-merge default, and triggers session-cleanup on terminal states. Backend Tauri commands stay stable.
- `conflict-resolution`: Adds a closed-loop auto-handoff path — when auto-merge detects conflict, the same session opens the Conflicts panel and pre-fills an Ask-Claude prompt requiring user confirmation before send. Fixes Space-toggle bug in region header.
- `git-panel-v2`: Removes duplicate Ship/Push buttons from the header (header becomes info-only). Commit bar is the single source for Commit/Push/Ship/Merge, with visible `<kbd>` chips on every action and a new visible Merge-local button.
- `keyboard-shortcuts`: Adds command-palette entries for Ship/Push/Open-conflicts/Merge-locally; mandates that every git action has a discoverable shortcut surfaced via `<kbd>` chip (not `title`-only).

## Impact

- **Frontend**
  - `src/components/git/ShipDialog.tsx` — split into 2 steps, add target branch picker, init auto-merge from config
  - `src/components/git/GitPanel.tsx` — remove duplicate header buttons, add visible Merge button + `<kbd>` chips on all actions
  - `src/components/git/ConflictsPanel.tsx` — blur-after-click fix on row/chevron buttons, auto-handoff inline alert + prompt prefill
  - `src/components/session/MergeModal.tsx` — invocation moved from Sidebar to GitPanel
  - `src/components/layout/Sidebar.tsx` — remove Merge entry point
  - `src/components/command/CommandPalette.tsx` — add git entries
  - `src/stores/git.ts` — atom for `autoMergeDefault` config sync
  - `src/stores/shortcuts.ts` — verify/extend git shortcuts coverage

- **Backend (Rust)**
  - `src-tauri/src/commands.rs` — replace `cleanup_merged_session` body to do full deletion; add new command for post-PR-merged auto-cleanup trigger if missing
  - `src-tauri/src/db.rs` — add `delete_session` if not already present
  - `src-tauri/src/config.rs` — add `git.auto_merge_default: bool` field with default `true`

- **Specs (delta files in this change)**
  - `specs/ship-flow/spec.md` — modified
  - `specs/conflict-resolution/spec.md` — modified
  - `specs/git-panel-v2/spec.md` — modified
  - `specs/keyboard-shortcuts/spec.md` — modified
  - `specs/session-cleanup/spec.md` — new

- **Compatibility note**
  - The v1 `ship-flow` change in `openspec/changes/ship-flow/` is implemented but not archived. The v2 deltas in this change extend the v1 deltas in place. Recommended order: archive v1 first to promote its specs into `openspec/specs/`, then apply v2 deltas against the promoted specs.
