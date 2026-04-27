## Why

Today the commit → push → PR flow in cluihud requires multiple steps across different surfaces: stage in git panel, commit, switch to remote thinking, then `create_pr`. Merge conflicts from `merge_session` only surface as a toast, with no guided resolution. The sidebar's quick actions emphasize direct merge over PR, which misaligns with the project's trace/review bias. This change collapses the common path to a single keystroke, closes the loop after Claude's `/commit` skill runs, and gives conflicts a proper resolution surface.

## What Changes

- Add **Ship** action: atomic commit (if staged) + push + create PR, accessible via global shortcut `Ctrl+Shift+Y` and contextual `Ctrl+Shift+Enter` inside the git panel commit textarea.
- Add **Push** action: explicit push-only, global shortcut `Ctrl+Shift+U`, button in git panel when `ahead > 0 && !prInfo`.
- Add **Ship-it badge**: after any commit (manual via git panel OR Claude-driven via `/commit` skill), when `ahead > 0 && !prInfo`, show prominent badge in git panel header offering push + PR in one click.
- Add **PR preview dialog**: editable title (from last commit subject) + body (commits range `base..HEAD` + diffstat + optional `.cluihud/pr-template.md`), shown before `gh pr create`.
- Add **CI status polling**: after PR exists, poll `gh pr checks <n> --json` every 20s while PR is OPEN in the active session; surface ✓/✗/⏳ badge in git panel header.
- Add **Conflict resolution surface**: new tab type `conflict` in right panel with 3-panel ours/theirs/merged layout. Git panel lists conflicted files inline at top when `merge_session` returns `conflict: true`; clicking opens the tab. Tab is expandable to Zen Mode for full-screen review.
- **BREAKING** Sidebar quick actions: replace Merge button with PR button. Merge remains accessible via `Ctrl+Shift+M`, command palette, and an option in the git panel secondary actions.
- Add **contextual dispatch for `Ctrl+Shift+R`**: when focus is in plan/spec panel and a pending plan review exists → fires revise-plan (current behavior); when focus is in git panel or a conflict tab is active → fires resolve-conflict (open active conflict tab or inject conflict context to Claude).
- Extend Tauri commands: `git_push`, `git_ship` (commit+push+PR convenience), `get_pr_preview_data` (commits/diffstat), `poll_pr_checks`.

## Capabilities

### New Capabilities
- `ship-flow`: Atomic commit+push+PR flow, explicit push action, ship-it badge surfacing post-commit, PR preview dialog, CI status polling.
- `conflict-resolution`: Conflict tab type with 3-panel ours/theirs/merged layout, inline conflict list in git panel, expandable to Zen Mode, Claude-assisted resolution path.

### Modified Capabilities
- `keyboard-shortcuts`: Adds Ship (`Ctrl+Shift+Y`), Push (`Ctrl+Shift+U`), contextual ship-in-textarea (`Ctrl+Shift+Enter`), and contextual dispatch for `Ctrl+Shift+R` (revise-plan vs resolve-conflict based on active focus zone / tab type).
- `git-panel-v2`: Adds Ship and Push buttons to the commit bar, Ship-it badge, CI status indicator, and inline conflict list section at the top of the panel when conflicts are active.
- `tab-system`: Adds `conflict` as a new tab type with its own icon, singleton-per-session behavior, and data shape (path + ours/theirs/merged).

## Impact

- **Frontend**
  - `src/stores/shortcuts.ts`: new entries (Ship, Push), contextual handler for Ctrl+Shift+R
  - `src/stores/git.ts`: new atoms for PR preview data, CI status, conflict state
  - `src/stores/rightPanel.ts`: add `conflict` tab type
  - `src/components/git/GitPanel.tsx`: Ship/Push buttons, Ship-it badge, CI status indicator, conflict section
  - `src/components/git/ShipDialog.tsx` (new): preview dialog
  - `src/components/git/ConflictTab.tsx` (new): 3-panel resolution surface, Zen-expandable
  - `src/components/layout/Sidebar.tsx`: replace Merge quick-action with PR
  - `src/components/session/SessionRow.tsx`: PR action replaces Merge in hover actions
  - `src/hooks/useKeyboardShortcuts.ts`: may require contextual handler wiring

- **Backend (Rust)**
  - `src-tauri/src/commands.rs`: add `git_push`, `git_ship`, `get_pr_preview_data`, `poll_pr_checks`, conflict-file detection extension to `merge_session`
  - `src-tauri/src/worktree.rs`: `push`, `ship`, `pr_preview_data`, `pr_checks`, `conflicted_files`, `file_version` (for ours/theirs)
  - `src-tauri/src/lib.rs`: register new commands

- **Dependencies**: no new crates or npm packages. Uses existing `gh` CLI via `Command` in worktree module.

- **Out of scope (follow-up sprint)**: TopBar redesign to host session actions. Current sidebar-based discoverability stays.
