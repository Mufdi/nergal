Linear: FRI-609

## Why

The checkpoint-1 walk (2026-11-24) found four defects in the git panel and in the actions that send a prompt to the agent:

- `Ctrl+Enter` in the PR viewer merges the PR. It is also the global fullscreen-terminal shortcut, and both listeners run on the same keypress.
- The git panel hint bars use 9px text, differ from view to view, and the PR viewer has none. Nothing tells the user that `a` annotates a hunk.
- Actions that hand a prompt to the agent are labelled "Claude". Nergal is agent-agnostic, so the labels name "Agent".
- "Apply with Claude" pastes the prompt but does not submit it. "Ask Claude" in the conflicts view types the prompt with a raw `\r`, so a multi-line prompt arrives as several lines.

## What Changes

- **Merge key**: in the PR viewer, `M` opens a confirmation dialog, and confirming merges the PR. A leader chord ending in `m`, a key repeat, or a PR that is no longer open never starts a merge. The viewer no longer handles `Ctrl+Enter`, so `Ctrl+Enter` only toggles the fullscreen terminal. The Merge button shows `M` as its chip.
- **Hint bar**: one shared `HintBar` component renders every git panel hint bar: Files, History, Stashes, PRs list, PR viewer, Conflicts, Branches and the PR viewer and Conflicts file pickers. Inline hints beside one control move to 10px. It is a sticky flex row at the bottom, with a background, `Kbd` chips at 10px and 10px labels. The PR viewer gets a hint bar with `j/k`, `a`, `Ctrl+←/→` and `M`.
- **Kbd size**: `Kbd` gains a `size` option. The hint bar uses the 10px size; buttons and tooltips keep 9px.
- **Labels**: agent-facing actions say "Agent" instead of "Claude": "Apply with Agent", "Ask Agent", "Intent for the agent (optional)", "Sent to agent", "Send to Agent", "The agent will revise the plan" and "Waiting for the agent to revise...". Product names ("Claude Code" in setup, MCP and settings) and the "Claude" status provider stay.
- **Submit**: a new Tauri command `deliver_prompt_to_session` pastes a prompt into the session as one bracketed paste, submits it after a short settle, and resolves only once the submit is written. "Apply with Agent" and "Ask Agent" use it. `terminal_deliver` is removed: the PR viewer was its only caller.
- **Docs**: `docs/patterns.md` §4 and `docs/design.md` §3.4 describe the hint bar standard and the `M` verb.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `git-panel-v2`: the PR viewer merges with `M` after a confirmation; the git panel views share one hint bar; "Apply with Agent" submits its prompt at once.
- `terminal-wezterm`: the agent delivery used by the git panel is `deliver_prompt_to_session`, which also submits; `terminal_deliver` is gone.
- `conflict-resolution`: "Ask Agent" submits the conflict prompt as one paste through the same command; the contextual shortcut names "Ask Agent".
- `ship-flow`: the Apply button label reads `Apply with Agent (N)`.
- `spec-annotations`: the SpecPanel action is labelled "Send to Agent".

## Impact

- Frontend: `src/components/git/PrViewer.tsx`, `src/components/git/ConflictsPanel.tsx`, `src/hooks/useConflictResolution.ts`, a new `src/lib/prViewerKeys.ts`, `src/components/git/chips/{FilesChip,HistoryChip,BranchesChip,StashesChip,PrsChip}.tsx`, `src/components/ui/kbd.tsx`, a new `src/components/ui/hint-bar.tsx`, `src/components/spec/SpecPanel.tsx`, `src/components/plan/{PlanPanel,PlanAnnotationToolbar}.tsx`.
- Backend: `src-tauri/src/pty.rs` (new command, `terminal_deliver` removed), `src-tauri/src/lib.rs` (command registration).
- Docs: `docs/patterns.md`, `docs/design.md`.
- No config, schema or hook change. No new dependency.
