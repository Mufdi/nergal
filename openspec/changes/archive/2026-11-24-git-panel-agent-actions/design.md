## Context

The checkpoint-1 walk of FRI-604/506/507/473 found four git panel defects (FRI-609). The user settled each fix on 2026-11-24. This design records the decisions and the alternatives that were rejected. No flow or state machine changes, so there is no diagram.

## Goals / Non-Goals

**Goals:**
- One key, one action: `Ctrl+Enter` toggles the fullscreen terminal everywhere, including the PR viewer.
- One hint bar standard across the git panel, with the PR viewer keys visible.
- Agent-agnostic labels on actions that hand a prompt to the agent.
- Prompts from the git panel arrive as one turn and are submitted.

**Non-Goals:**
- The Ship dialog hint lines (`ShipDialog.tsx:728`, `:814`, `:997`). The Ship dialog is a modal, not a git panel view.
- Opening a worktree session on a PR branch (FRI-610).
- Product names: "Claude Code" in setup, MCP and settings, and the "Claude" status provider.
- The FilesChip commit `Ctrl+Enter` (`FilesChip.tsx:358`) and the StashesChip `Ctrl+Enter` (`StashesChip.tsx:417`). Both bind inside text fields, where the global dispatcher already skips `fullscreen-terminal` (`useKeyboardShortcuts.ts:341`).

## Decisions

### D1. Merge moves to `M` with a confirmation

`M` (no Ctrl, Alt or Meta; Shift is allowed, as with `a`) in the PR viewer opens `confirm()` (`src/lib/confirm.ts`). Confirming merges the PR. The dialog states the target branch and the number of unresolved annotations. The viewer stops handling `Ctrl+Enter`.

This follows the bare-letter verb pattern of the plan panel. `X` there is confirm-gated because a bare letter is easy to hit by accident (`PlanPanel.tsx:226-275`). A merge is not reversible from Nergal, so it gets the same gate.

Guards, all in the viewer handler or the merge request:
- The handler returns when `e.defaultPrevented` or a leader chord is pending (`leaderPendingAtom`, `src/stores/leader.ts:20`). Both the global dispatcher and the viewer listen on `window` in capture phase, and their registration order changes when either effect re-runs. Checking both signals covers either order, so `leader m` (Complete Merge) never also opens the merge confirmation. The same guard fixes `leader a` opening an annotation today.
- `e.repeat` is ignored, and a `mergeConfirmOpenRef` blocks a second confirmation while one is pending. `confirm()` queues calls (`confirm.ts:63-64`), so without the ref a fast double press would queue two dialogs.
- The request reads `state`, `merging` and `performMerge` through refs that always hold the latest render's values, and checks `state === "OPEN" && !merging` before opening the dialog and again after the user confirms. It also captures `{ workspaceId, prNumber }` when the dialog opens and aborts if the viewer shows another PR when the user confirms: the viewer is not remounted per PR (`PrsChip.tsx:234`), so the latest `performMerge` would target the new PR. The keydown effect does not list these values in its deps, so a closure would see stale ones.
- The pure helper `prViewerKeyAction` receives `canMerge` (PR open and no merge running) so the gate is unit-tested.

Revision 1 (adversarial review): the Merge button click opens the same confirmation as `M`. Its chip names `M`, so a click that merged at once while the key asked first would give one chip two safety levels. The inline "merge anyway" strip goes away: the dialog already states the unresolved annotation count. One merge flow remains.

The decision after the dialog lives in a pure helper, `mergeAfterConfirm`, so the re-checks (state still open, no merge running, same `{ workspaceId, prNumber }`) are unit-tested.

Alternatives:
- Keep `Ctrl+Enter` and call `stopImmediatePropagation` in the viewer. Rejected: `Ctrl+Enter` would mean two things depending on the panel, and the viewer cannot rely on running before the global listener.
- A leader chord (`leader m`). Rejected: `leader m` already means "Complete Merge" (`shortcuts.ts:786`, shown in `GitPanel.tsx:462`).
- `M` without a confirmation. Rejected by the user.
- Reject `Shift+M`. Rejected: the chip renders an upper-case `M`, and `a` already accepts Shift.

### D2. One shared `HintBar` with 10px `Kbd` chips

Placement (revision 1): in a view, the bar is a `shrink-0` sibling after the scroll container, so it sits at the bottom even when the list is short. A `sticky bottom-0` last child only reaches the bottom when the list overflows. Inside a self-sizing picker card, the bar is the last child of the card. The PR viewer bar sits between the diff and the footer buttons. The bar hides a hint whose key does nothing in the current state (`M` on a PR that is not open, `j/k` and `a` with no hunks).

Scope: the key-hint row at the bottom of each git panel view and of each file picker (the PR viewer picker and the Conflicts picker). Inline hints beside one control stay inline but move to 10px: the Conflicts pane headers (`ConflictsPanel.tsx:1456`) and the intent field's `Ctrl+Shift+R sends` (`ConflictsPanel.tsx:1057`).

A new `src/components/ui/hint-bar.tsx` exports `HintBar`. It renders a sticky bottom row: `sticky bottom-0 z-10 flex flex-wrap items-center gap-x-1.5 gap-y-0.5 border-t border-border/40 bg-card/95 px-3 py-1 text-[10px] text-muted-foreground`. Its children are `Kbd` chips and labels. `Kbd` gains `size?: "sm" | "md"`: `sm` keeps today's 9px, and `md` renders 10px text. The hint bar uses `md`.

The chips stay because the user chose them: chips read faster than plain glyph text. `docs/patterns.md` §4 changes to bless "Kbd chips plus 10px labels in a hint bar". The rule against chips in input placeholders stays.

Alternatives:
- Plain glyph text, as §4 says today. Rejected by the user.
- Override the chip size with `className="text-[10px]"`. Rejected: `Kbd` already sets `text-[9px]`, and two font-size utilities on one element resolve by stylesheet order, not by class order.
- Fix each bar in place without a component. Rejected: the bars drifted apart because each view copied the classes.

### D3. One awaited submit command for git panel prompts

A new async Tauri command `deliver_prompt_to_session(session_id, text)` pastes the prompt as one bracketed paste (`paste_to_session(.., false)`, `pty.rs:2071`), waits 250 ms, then writes a lone CR (`submit_to_session`, `pty.rs:2097`). It resolves only after the CR is written and returns any paste or submit error. Linear and ClickUp "send to session" use the same two steps through `paste_and_submit_settled` (`pty.rs:2122`, called from `linear/mod.rs:452` and `clickup/mod.rs:828`), which avoids the race where an Enter in the same burst as the end-of-paste marker submits too early. That helper fires the CR on a detached task and drops its error (`pty.rs:2137`), so the git panel cannot know when the turn was sent.

Because the command resolves after the CR, the callers' in-flight guards cover the whole delivery: "Ask Agent" keeps its `sending` state, and "Apply with Agent" gains an `applyingRef` (a ref, because the `Ctrl+Shift+R` listener closes over stale state). "Ask Agent" moves focus to the terminal only after the delivery resolves, so keys typed during the settle cannot land between the paste and the CR. A double click or a repeated `Ctrl+Shift+R` within the 250 ms window no longer sends paste, paste, CR, CR.

"Apply with Agent" (`PrViewer.tsx:502`) and "Ask Agent" (`ConflictsPanel.tsx:421`) call it. `terminal_deliver` (`pty.rs:2008`) is removed: the PR viewer is its only caller.

Alternatives:
- Reuse `paste_and_submit_settled` as is. Rejected: it resolves before the CR and hides the submit error, so the success toast could lie and a double click doubles the prompt.
- Add a `submit` flag to `terminal_deliver`. Rejected: no caller wants a delivery without a submit, so the flag would be dead.
- Type a CR from the frontend after `terminal_deliver` resolves. Rejected: it splits one delivery across two IPC calls, and a session switch in between sends the CR elsewhere.
- Keep "Ask Agent" on `writeToSession(prompt + "\r")`. Rejected: an unbracketed multi-line prompt submits its first line as a turn.

### D4. Labels say "Agent"

Visible strings, the matching function names and the comments that name them change: `applyWithClaude` → `applyWithAgent`, `askClaude` → `askAgent`, `onAskClaude` → `onAskAgent` (`src/hooks/useConflictResolution.ts` and `ConflictsPanel.tsx`), `handleSendToClaude` → `handleSendToAgent`. Master specs `ship-flow` and `conflict-resolution` name the old labels too, so they get deltas. The spec requirement titles keep their names, so the archive matches the existing blocks.

## Risks / Trade-offs

- [A user with `Ctrl+Enter` merge in muscle memory toggles fullscreen instead] → the Merge button chip shows `M`, and the PR viewer hint bar lists `M`.
- [`M` fires while focus sits in the terminal] → the viewer handler already returns when the target is a text field (`PrViewer.tsx:601-608`), and the terminal input is a textarea.
- [The 250 ms settle is a heuristic] → the same value already serves Linear and ClickUp delivery without reported failures.
- [Text the user left half-typed in Claude Code's input is submitted together with the prompt] → accepted: the user chose an immediate submit, the same behavior Linear and ClickUp delivery already have.
- [`deliver_prompt_to_session` is a new IPC entry point that types into a PTY] → it accepts the same inputs `terminal_deliver` accepted, and it refuses auxiliary shells through `paste_to_session`.
