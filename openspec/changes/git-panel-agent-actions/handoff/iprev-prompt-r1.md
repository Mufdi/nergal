You are a senior engineer performing an independent review of a proposed implementation plan (an OpenSpec change in the repo at /home/felipe/Projects/cluihud, a Tauri 2 + React 19 desktop wrapper for Claude Code). You may read any repo file to verify claims (read-only; do not edit anything).

Evaluate it for:
1. Correctness — does the approach actually solve the stated problem?
2. Completeness — missing steps, edge cases, failure modes
3. Concurrency / state integrity — races, stale reads
4. Test coverage — is there a clear way to verify each change?
5. Security — input validation, injection surfaces
6. Rollback — is the change reversible?

The decisions in handoff/.work-modules.json interview_answers are settled by the user; do not re-litigate them, review their execution.
Verify the path:line facts in implementation.md against the code.

List issues as a numbered markdown list with severity (critical/high/medium/low). Be specific — cite sections of the plan. Do not soften findings.

End your review with exactly one of these two lines (no quotes, no trailing content):
VERDICT: APPROVED
VERDICT: REVISE


PLAN:
---

===== proposal.md =====
Linear: FRI-609

## Why

The checkpoint-1 walk (2026-11-24) found four defects in the git panel and in the actions that send a prompt to the agent:

- `Ctrl+Enter` in the PR viewer merges the PR. It is also the global fullscreen-terminal shortcut, and both listeners run on the same keypress.
- The git panel hint bars use 9px text, differ from view to view, and the PR viewer has none. Nothing tells the user that `a` annotates a hunk.
- Actions that hand a prompt to the agent are labelled "Claude". Nergal is agent-agnostic, so the labels name "Agent".
- "Apply with Claude" pastes the prompt but does not submit it. "Ask Claude" in the conflicts view types the prompt with a raw `\r`, so a multi-line prompt arrives as several lines.

## What Changes

- **Merge key**: in the PR viewer, `M` opens a confirmation dialog, and confirming merges the PR. The viewer no longer handles `Ctrl+Enter`, so `Ctrl+Enter` only toggles the fullscreen terminal. The Merge button shows `M` as its chip.
- **Hint bar**: one shared `HintBar` component renders every git panel hint bar: Files, History, Stashes, PRs list, PR viewer, Conflicts, Branches and the PR viewer file picker. It is a sticky flex row at the bottom, with a background, `Kbd` chips at 10px and 10px labels. The PR viewer gets a hint bar with `j/k`, `a`, `Ctrl+←/→` and `M`.
- **Kbd size**: `Kbd` gains a `size` option. The hint bar uses the 10px size; buttons and tooltips keep 9px.
- **Labels**: agent-facing actions say "Agent" instead of "Claude": "Apply with Agent", "Ask Agent", "Intent for the agent (optional)", "Sent to agent", "Send to Agent", "The agent will revise the plan" and "Waiting for the agent to revise...". Product names ("Claude Code" in setup, MCP and settings) and the "Claude" status provider stay.
- **Submit**: a new Tauri command `deliver_prompt_to_session` pastes a prompt into the session as one bracketed paste and submits it after a short settle (`paste_and_submit_settled`). "Apply with Agent" and "Ask Agent" use it. `terminal_deliver` is removed: the PR viewer was its only caller.
- **Docs**: `docs/patterns.md` §4 and `docs/design.md` §3.4 describe the hint bar standard and the `M` verb.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `git-panel-v2`: the PR viewer merges with `M` after a confirmation; the git panel views share one hint bar; "Apply with Agent" submits its prompt at once.
- `terminal-wezterm`: the agent delivery used by the git panel is `deliver_prompt_to_session`, which also submits; `terminal_deliver` is gone.
- `conflict-resolution`: "Ask Agent" submits the conflict prompt as one paste through the same command.
- `spec-annotations`: the SpecPanel action is labelled "Send to Agent".

## Impact

- Frontend: `src/components/git/PrViewer.tsx`, `src/components/git/ConflictsPanel.tsx`, `src/components/git/chips/{FilesChip,HistoryChip,BranchesChip,StashesChip,PrsChip}.tsx`, `src/components/ui/kbd.tsx`, a new `src/components/ui/hint-bar.tsx`, `src/components/spec/SpecPanel.tsx`, `src/components/plan/{PlanPanel,PlanAnnotationToolbar}.tsx`.
- Backend: `src-tauri/src/pty.rs` (new command, `terminal_deliver` removed), `src-tauri/src/lib.rs` (command registration).
- Docs: `docs/patterns.md`, `docs/design.md`.
- No config, schema or hook change. No new dependency.


===== design.md =====
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

`M` (no modifier) in the PR viewer opens `confirm()` (`src/lib/confirm.ts`). Confirming merges the PR. The dialog states the target branch and the number of unresolved annotations. The viewer stops handling `Ctrl+Enter`.

This follows the bare-letter verb pattern of the plan panel. `X` there is confirm-gated because a bare letter is easy to hit by accident (`PlanPanel.tsx:226-275`). A merge is not reversible from Nergal, so it gets the same gate.

The Merge button keeps its click flow. A click without annotations merges at once. A click with annotations shows the inline "merge anyway" strip. A click is deliberate, and the button text names the target branch.

Alternatives:
- Keep `Ctrl+Enter` and call `stopImmediatePropagation` in the viewer. Rejected: `Ctrl+Enter` would mean two things depending on the panel, and the global listener registers first, so the viewer cannot pre-empt it without reordering listeners.
- A leader chord (`leader m`). Rejected: `leader m` already means "Finish merge" in the conflicts flow (`GitPanel.tsx:462`).
- `M` without a confirmation. Rejected by the user.

### D2. One shared `HintBar` with 10px `Kbd` chips

A new `src/components/ui/hint-bar.tsx` exports `HintBar`. It renders a sticky bottom row: `sticky bottom-0 z-10 flex flex-wrap items-center gap-x-1.5 gap-y-0.5 border-t border-border/40 bg-card/95 px-3 py-1 text-[10px] text-muted-foreground`. Its children are `Kbd` chips and labels. `Kbd` gains `size?: "sm" | "md"`: `sm` keeps today's 9px, and `md` renders 10px text. The hint bar uses `md`.

The chips stay because the user chose them: chips read faster than plain glyph text. `docs/patterns.md` §4 changes to bless "Kbd chips plus 10px labels in a hint bar". The rule against chips in input placeholders stays.

Alternatives:
- Plain glyph text, as §4 says today. Rejected by the user.
- Override the chip size with `className="text-[10px]"`. Rejected: `Kbd` already sets `text-[9px]`, and two font-size utilities on one element resolve by stylesheet order, not by class order.
- Fix each bar in place without a component. Rejected: the bars drifted apart because each view copied the classes.

### D3. One submit command for git panel prompts

A new Tauri command `deliver_prompt_to_session(session_id, text)` calls `crate::pty::paste_and_submit_settled` (`pty.rs:2122`). That helper writes one bracketed paste, then writes a lone CR 250 ms later. Linear and ClickUp "send to session" use the same helper (`linear/mod.rs:452`, `clickup/mod.rs:828`), and it avoids the race where an Enter in the same burst as the end-of-paste marker submits too early.

"Apply with Agent" (`PrViewer.tsx:502`) and "Ask Agent" (`ConflictsPanel.tsx:420`) call it. `terminal_deliver` (`pty.rs:2008`) is removed: the PR viewer is its only caller.

Alternatives:
- Add a `submit` flag to `terminal_deliver`. Rejected: no caller wants a delivery without a submit, so the flag would be dead.
- Type a CR from the frontend after `terminal_deliver` resolves. Rejected: that recreates the timing race the backend helper exists to avoid.
- Keep "Ask Agent" on `writeToSession(prompt + "\r")`. Rejected: an unbracketed multi-line prompt submits its first line as a turn.

### D4. Labels say "Agent"

Visible strings and the matching function names change: `applyWithClaude` → `applyWithAgent`, `askClaude` → `askAgent`, `onAskClaude` → `onAskAgent`, `handleSendToClaude` → `handleSendToAgent`. The spec requirement titles keep their names, so the archive matches the existing blocks.

## Risks / Trade-offs

- [A user with `Ctrl+Enter` merge in muscle memory toggles fullscreen instead] → the Merge button chip shows `M`, and the PR viewer hint bar lists `M`.
- [`M` fires while focus sits in the terminal] → the viewer handler already returns when the target is a text field (`PrViewer.tsx:603-608`), and the terminal input is a textarea.
- [The 250 ms settle is a heuristic] → the same value already serves Linear and ClickUp delivery without reported failures.
- [`deliver_prompt_to_session` is a new IPC entry point that types into a PTY] → it accepts the same inputs `terminal_deliver` accepted, and it refuses auxiliary shells through `paste_to_session`.


===== implementation.md =====
# Implementation Plan: git-panel-agent-actions

> Grounded in the codebase at `05c263b` (main, 2026-11-24). Behaviour verified for the load-bearing claims below.

## Verified codebase facts (do not re-assume)

- PR viewer keyboard: one `window` keydown listener in capture phase (`src/components/git/PrViewer.tsx:596-693`), live only while `isActive && listenerActive` (`:597-598`, `listenerActive` at `:250-252`). It returns early in annotation mode, while any `[data-slot="dialog-content"]` exists, and when the target is an input, textarea, `.cm-editor` or contenteditable (`:600-608`). `a` calls `startAnnotating()` (`:666-671`). `Ctrl+Enter` calls `handleMergeClick()` (`:672-677`). `Ctrl+←/→` change file (`:679-690`). The effect deps list is at `:695`.
- Merge: `handleMergeClick()` (`PrViewer.tsx:584-591`) returns unless `state === "OPEN"`, shows the inline "merge anyway" strip when annotations exist, else calls `performMerge()` (`:519-582`). `merging` state gates the button (`:1205`).
- Merge button chip: `<Kbd keys="ctrl+enter" />` at `PrViewer.tsx:1214`. Apply button label at `:1160`, its `ctrl+shift+r` chip at `:1161`, tooltip strings at `:1166-1170`.
- Apply: `applyWithClaude()` (`PrViewer.tsx:478-517`) builds the prompt and calls `invoke("terminal_deliver", { sessionId, text })` (`:502`), toast title "Apply with Claude" (`:505`). `onApply()` at `:721` wraps it for the `ctrl+shift+r` route (`:716` comment).
- PR file picker header hint: `PrViewer.tsx:1083-1090`, a sticky top header with a 9px plain-text hint `j/k move · Enter pick · Esc close`.
- PR viewer footer: `PrViewer.tsx:1128` (`flex shrink-0 items-center gap-2 border-t ... px-3 py-1.5`). The diff body sits above it.
- `terminal_deliver` (`src-tauri/src/pty.rs:2008-2014`) calls `paste_to_session(&state, &session_id, &text, false)`. Registered in `src-tauri/src/lib.rs:533`. Its only frontend caller is `PrViewer.tsx:502` (grep `terminal_deliver` in `src/`).
- `paste_and_submit_settled(app, session_id, text)` (`pty.rs:2122-2141`): `paste_to_session(.., false)` then, on an async task, sleeps 250 ms and calls `submit_to_session` (writes `\r`, `pty.rs:~2090-2113`). `paste_to_session` (`pty.rs:2071`) rejects auxiliary shell ids (test `paste_to_session_rejects_aux_shells`, `pty.rs:2681`).
- Conflicts "Ask": `askClaude` (`src/components/git/ConflictsPanel.tsx:407-428`) invokes `build_conflict_prompt`, focuses the terminal, then `terminalService.writeToSession(sessionId, prompt + "\r")` (`:420`), toast "Sent to Claude" (`:421`). Exposed as `onAskClaude` (`:483`). Labels at `:1052` (comment), `:1056` ("Intent for Claude (optional)"), `:1068` ("Ask Claude"). Hint line at `:935-937` (9px plain text, ends with "ask Claude"), inside the toolbar block, not at the view bottom.
- SpecPanel: `handleSendToClaude` (`src/components/spec/SpecPanel.tsx:339`) queues feedback via `set_pending_annotations`; label "Send to Claude" at `:559`; callers `:247`, `:551`.
- Plan labels: `PlanPanel.tsx:157` toast description "Claude will revise the plan"; `PlanAnnotationToolbar.tsx:403` "Waiting for Claude to revise...". `PlanPanel.tsx:121` names "Claude Code" (product name, stays).
- Hint bars today: `FilesChip.tsx:331-333`, `HistoryChip.tsx:194-195`, `BranchesChip.tsx:462-463` (sticky, 9px, `gap-1`, no wrap); `StashesChip.tsx:419-423` (inline under the Save button, only when `stashes.length > 0`); `PrsChip.tsx:315-317` (list footer, not sticky, no flex, 9px) and a duplicate header hint `PrsChip.tsx:258-260`.
- `Kbd` (`src/components/ui/kbd.tsx`): fixed `h-4 px-1 font-mono text-[9px]`, tones `subtle` / `onPrimary`, no size prop. `formatKey` upper-cases single letters, so `keys="m"` renders `M`.
- `confirm()` (`src/lib/confirm.ts`) returns `Promise<boolean>`; `body` is HTML and callers escape user text with `escapeHtml` (`src/lib/escapeHtml.ts`). The dialog renders a `DialogContent` (`src/components/ui/ConfirmHost.tsx:62`), so the PR viewer listener is inert while it is open.
- Global `Ctrl+Enter` is `fullscreen-terminal` (`src/stores/shortcuts.ts:704`), dispatched at window capture by `useKeyboardShortcuts.ts`; it is skipped only inside non-terminal text fields (`useKeyboardShortcuts.ts:341`).
- Pure key helpers live in `src/lib/` with vitest files beside them (`gitChipKeys.ts` + `gitChipKeys.test.ts`). Vitest runs with `pnpm test`.

## Execution order

1. Backend: `deliver_prompt_to_session` in `pty.rs`, register in `lib.rs`, remove `terminal_deliver`.
2. `Kbd` `size` prop, then `HintBar` component.
3. PR viewer: key helper (`src/lib/prViewerKeys.ts`), `M` + confirm, hint bar, picker hint bar, Apply via the new command, labels.
4. Conflicts: Ask via the new command, labels, hint bar at the view bottom.
5. Chips: Files, History, Branches, Stashes, PRs list on `HintBar`.
6. Label sweep: SpecPanel, PlanPanel, PlanAnnotationToolbar.
7. Docs: `docs/patterns.md` §4 (+ the bare-letter precedents line `:406` and `:96` add `M`), `docs/design.md` §3.4 (`size` prop + `HintBar`).

## Plan

- **`src-tauri/src/pty.rs`**: add `#[tauri::command] pub fn deliver_prompt_to_session(app: AppHandle, session_id: String, text: String) -> Result<(), String>` that returns `paste_and_submit_settled(&app, &session_id, &text)`. Delete `terminal_deliver`. `lib.rs:533`: swap the registration.
- **`src/components/ui/kbd.tsx`**: `size?: "sm" | "md"` (default `sm`); `sm` → `text-[9px]`, `md` → `text-[10px]`. Keep `h-4`.
- **`src/components/ui/hint-bar.tsx`** (new): `HintBar({ children, className })` renders a `div` with the D2 classes; `Hint({ keys, children })` renders `<Kbd size="md" keys=…/>` chips (one per key, `keys` may be an array) plus the label. Callers compose `<HintBar><Hint keys={["j","k"]}>hunk</Hint>…</HintBar>`.
- **`src/lib/prViewerKeys.ts`** (new): `prViewerKeyAction(e: { code; ctrlKey; metaKey; altKey; shiftKey }, ctx: { pickerOpen; hunkCount; fileCount }) → "picker-close" | "picker-next" | "picker-prev" | "picker-pick" | "hunk-next" | "hunk-prev" | "toggle-collapse" | "annotate" | "merge" | "file-prev" | "file-next" | null`. `KeyM` with no modifier → `"merge"`; `Enter` with Ctrl or Meta → `null`. `PrViewer.tsx` handler maps the action to the existing calls. State checks that need component state (`annotating`, dialog open, in-field) stay in the component.
- **`PrViewer.tsx`**: `"merge"` → `requestMergeConfirm()`: returns unless `state === "OPEN" && !merging`; `await confirm({ title: "Merge PR #N?", body: "Merge into <b>base</b>." + (annotations ? " N annotations are unresolved." : ""), confirmLabel: "Merge", destructive: true })` with `escapeHtml(baseRefName)`; on true → `performMerge()`. Merge button chip `keys="m"`. Add `HintBar` at the bottom of the diff area (above the footer) with `j/k` hunk, `a` annotate, `Ctrl+←/→` file, `M` merge. Picker: replace the header hint with a `HintBar` at the picker bottom (`j/k` move, `↵` pick, `Esc` close). `applyWithClaude` → `applyWithAgent`, invoke `deliver_prompt_to_session`, toast title "Apply with Agent", label "Apply with Agent (N)".
- **`ConflictsPanel.tsx`**: `askAgent` invokes `deliver_prompt_to_session` instead of `writeToSession`; toast "Sent to agent"; `onAskAgent`; labels "Intent for the agent (optional)" and "Ask Agent"; hint line → `HintBar` at the bottom of the view with chips (`↑↓`/`j k` region, `o`/`t`/`b` region, `Ctrl+Shift+O`/`T` all, `s` sync, `Ctrl+Shift+K` picker, `Ctrl+Shift+↵` save, `Ctrl+Shift+R` ask agent).
- **Chips**: replace each bar's `div` with `HintBar`, keeping its hints. Stashes: move its hints into a `HintBar` at the bottom of the view; it still renders only when `stashes.length > 0`. PRs list: footer → `HintBar`; drop the duplicate header hint (`PrsChip.tsx:258-260`).
- **Labels**: SpecPanel `handleSendToAgent` + "Send to Agent"; PlanPanel "The agent will revise the plan"; PlanAnnotationToolbar "Waiting for the agent to revise...".

## Per-phase risk

- [Removing `terminal_deliver` breaks a caller missed by the grep] → grep `terminal_deliver` across `src/` and `src-tauri/` after the change; `tsc` does not check `invoke` names, so the grep is the gate.
- [The PR viewer effect captures stale `merging`/`state`/`annotations`] → add them to the effect deps (`:695`) or read them through a ref, and cover with the walk.
- [A sticky `HintBar` inside a non-scrolling flex column does not stick] → place it as the last child of the scroll container, or as a `shrink-0` sibling after it; check each view in the walk.
- [`M` pressed twice quickly opens two confirms] → the second press lands while the dialog is open, and the listener returns on `[data-slot="dialog-content"]`.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh main git-panel-agent-actions`.
- Grep: no `terminal_deliver` in `src/` or `src-tauri/src/`; no "Claude" in the agent-action strings listed in the proposal.
- Manual walk on the installed `.deb` (tasks group Verification).


===== tasks.md =====
## 1. Prompt delivery that submits

- [ ] 1.1 Test (Red): covered by existing Rust tests, no new Red. `deliver_prompt_to_session` is a one-line wrapper over `paste_and_submit_settled`, whose parts are tested: `bracketed_paste_wraps_multiline_body_without_submit` (`pty.rs`), `paste_to_session_rejects_aux_shells` (`pty.rs`). No harness builds a `tauri::AppHandle` in tests. The submit timing is checked in walk 6.5.
- [ ] 1.2 Add the `deliver_prompt_to_session(session_id, text)` command in `src-tauri/src/pty.rs`, calling `paste_and_submit_settled`; delete `terminal_deliver`; swap the registration in `src-tauri/src/lib.rs`.
- [ ] 1.3 `src/components/git/PrViewer.tsx`: `applyWithClaude` → `applyWithAgent`, invoking `deliver_prompt_to_session`; toast title "Apply with Agent"; button label "Apply with Agent (N)".
- [ ] 1.4 `src/components/git/ConflictsPanel.tsx`: `askClaude` → `askAgent` (and `onAskClaude` → `onAskAgent`), invoking `deliver_prompt_to_session` instead of `writeToSession(prompt + "\r")`; toast "Sent to agent".

## 2. Merge with M

- [ ] 2.1 Test (Red): `src/lib/prViewerKeys.test.ts` for `prViewerKeyAction`: `KeyM` without modifiers → `"merge"`; `Shift+M`, `Ctrl+M` → `null`; `Ctrl+Enter` and `Meta+Enter` → `null`; `KeyA` → `"annotate"` with hunks and `null` without; `j`/`k`/arrows → hunk next/prev; `Ctrl+←/→` → file prev/next only with 2+ files; picker open: `Escape` → `"picker-close"`, `j`/`k` → picker next/prev, `Enter` → `"picker-pick"`, `KeyM` → `null`.
- [ ] 2.2 Add `src/lib/prViewerKeys.ts` with `prViewerKeyAction` and route the `PrViewer.tsx` keydown handler through it, keeping the annotation, dialog and in-field guards in the component.
- [ ] 2.3 `"merge"` opens `confirm()` ("Merge PR #N?", target branch escaped with `escapeHtml`, unresolved annotation count when any, confirm label "Merge") only when the PR is open and no merge runs; confirming calls `performMerge()`. The Merge button chip becomes `keys="m"`.

## 3. Shared hint bar

- [ ] 3.1 Screenshot check: each git panel view against the `Git panel hint bars share one standard` requirement (walk 6.2).
- [ ] 3.2 `src/components/ui/kbd.tsx`: add `size?: "sm" | "md"` (`sm` = 9px default, `md` = 10px).
- [ ] 3.3 Add `src/components/ui/hint-bar.tsx` (`HintBar` + `Hint`) with the classes from design D2.
- [ ] 3.4 PR viewer: a `HintBar` below the diff with `j/k` hunk, `a` annotate, `Ctrl+←/→` file, `M` merge; the file picker hint moves to a `HintBar` at the picker bottom.
- [ ] 3.5 `FilesChip`, `HistoryChip`, `BranchesChip`, `StashesChip` and the `PrsChip` list footer use `HintBar`; drop the duplicate PRs header hint (`PrsChip.tsx:258-260`); Stashes hints move to the bottom of the view.
- [ ] 3.6 `ConflictsPanel.tsx`: the hint line becomes a `HintBar` at the bottom of the view, with chips and "ask agent".

## 4. Agent labels

- [ ] 4.1 `ConflictsPanel.tsx`: "Intent for the agent (optional)", "Ask Agent".
- [ ] 4.2 `src/components/spec/SpecPanel.tsx`: `handleSendToClaude` → `handleSendToAgent`, label "Send to Agent".
- [ ] 4.3 `src/components/plan/PlanPanel.tsx`: "The agent will revise the plan"; `src/components/plan/PlanAnnotationToolbar.tsx`: "Waiting for the agent to revise...".
- [ ] 4.4 PR viewer Apply tooltips keep their text, with "Claude" removed where it appears.

## 5. Docs

- [ ] 5.1 `docs/patterns.md` §4: hint bars use `HintBar` (sticky bottom, 10px labels, 10px `Kbd` chips, wraps); placeholders keep plain glyph text. Add `M` to the PrViewer bare-letter precedents (`patterns.md:96`, `:406`).
- [ ] 5.2 `docs/design.md` §3.4: document the `Kbd` `size` prop and `HintBar`.

## 6. Verification

- [ ] 6.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- [ ] 6.2 `~/.claude/skills/work/scripts/work-gates.sh main git-panel-agent-actions` passes; grep finds no `terminal_deliver` in `src/` or `src-tauri/src/`.
- [ ] 6.3 Manual walk: in the PR viewer of an open PR, `M` opens "Merge PR #N?"; Cancel merges nothing; `Ctrl+Enter` toggles the fullscreen terminal and opens no merge confirmation.
- [ ] 6.4 Manual walk: every git panel view (Files, History, Branches, Stashes, PRs list, PR viewer, PR file picker, Conflicts) shows the same bottom hint bar with 10px chips; the PR viewer bar lists `j/k`, `a`, `Ctrl+←/→` and `M`; a narrow panel wraps the Branches bar.
- [ ] 6.5 Manual walk: annotate a PR hunk and press "Apply with Agent": the agent receives the multi-line prompt as one turn and starts without an Enter. In a conflict, "Ask Agent" with an intent does the same.
- [ ] 6.6 Manual walk: the SpecPanel button reads "Send to Agent"; a plan Revise shows "The agent will revise the plan".


===== handoff/.work-modules.json =====
{
  "tier": "S",
  "ceremony": "quick-fix",
  "risk_tier": "low",
  "spec_target": "git-panel-v2, terminal-wezterm, conflict-resolution, spec-annotations",
  "tracker": {
    "system": "linear",
    "id": "FRI-609",
    "pending": []
  },
  "validation": "pending",
  "planning_only": false,
  "iprev": {
    "rounds": 0,
    "verdict": null
  },
  "interview_answers": {
    "merge_key": "M with a confirmation dialog in the PR viewer; Ctrl+Enter only toggles fullscreen (user, 2026-11-24)",
    "hint_bars": "Kbd chips at 10px in a sticky bottom bar with background; update docs/patterns.md section 4 to bless it (user, 2026-11-24)",
    "labels": "Claude -> Agent on agent-facing actions; product names (Claude Code) and the Claude status provider stay (user, 2026-11-24)",
    "apply_submit": "Apply with Agent submits immediately via paste_and_submit_settled (user, 2026-11-24)",
    "worktree_for_pr": "out of scope, FRI-610"
  },
  "events": []
}

===== specs/conflict-resolution/spec.md =====
## MODIFIED Requirements

### Requirement: Conflict tab toolbar actions
The conflict tab SHALL expose four toolbar actions: Accept Ours, Accept Theirs, Ask Agent, Save Resolution. The intent field above Ask Agent is labelled "Intent for the agent (optional)".

#### Scenario: Accept ours copies ours to merged
- **WHEN** user clicks "Accept Ours"
- **THEN** the merged panel contents are replaced with the ours panel contents

#### Scenario: Accept theirs copies theirs to merged
- **WHEN** user clicks "Accept Theirs"
- **THEN** the merged panel contents are replaced with the theirs panel contents

#### Scenario: Ask Claude to resolve injects context and switches focus
- **WHEN** user clicks "Ask Agent" (or presses `Ctrl+Shift+R`) with an optional intent
- **THEN** the backend builds the conflict prompt from `{ path, ours, theirs, merged, intent }` (`build_conflict_prompt`)
- **AND** `deliver_prompt_to_session` pastes it into the session as one bracketed paste and submits it, so a multi-line prompt arrives as one turn
- **AND** focus switches to the terminal and a toast reads "Sent to agent"

#### Scenario: Save resolution writes, stages, closes, re-checks
- **WHEN** user clicks "Save Resolution"
- **THEN** the merged contents are written to the file on disk, the file is staged, the conflict tab is closed, and the git panel's conflict list is refreshed; if no conflicts remain, the conflict list section disappears


===== specs/git-panel-v2/spec.md =====
## ADDED Requirements

### Requirement: PR viewer merges with M after a confirmation
In the PR viewer, `M` (no modifier) SHALL open a confirmation dialog for merging the displayed PR, and SHALL merge only when the user confirms. The PR viewer SHALL NOT handle `Ctrl+Enter`; `Ctrl+Enter` keeps its global meaning (toggle the fullscreen terminal). The Merge button SHALL show `M` as its shortcut chip. `M` SHALL do nothing while the PR is not open, while a merge runs, while a dialog is open, in annotation mode, or while focus is in a text field.

#### Scenario: M asks before merging
- **GIVEN** the PR viewer shows open PR #7 into `main` with no annotations
- **WHEN** the user presses `M`
- **THEN** a confirmation dialog titled "Merge PR #7?" names `main` as the target
- **AND** confirming runs the merge; cancelling merges nothing

#### Scenario: M with unresolved annotations
- **GIVEN** the PR viewer shows open PR #7 with 3 annotations
- **WHEN** the user presses `M`
- **THEN** the confirmation body states that 3 annotations are unresolved
- **AND** confirming merges the PR

#### Scenario: Ctrl+Enter no longer merges
- **GIVEN** the PR viewer is active and focus is not in a text field
- **WHEN** the user presses `Ctrl+Enter`
- **THEN** no merge starts and no merge confirmation opens
- **AND** the fullscreen terminal toggles

#### Scenario: M is inert when the PR is closed
- **GIVEN** the PR viewer shows a merged or closed PR
- **WHEN** the user presses `M`
- **THEN** no confirmation opens

### Requirement: Git panel hint bars share one standard
Every keyboard hint bar in the git panel SHALL use one shared hint bar: a row pinned to the bottom of its view (sticky, with an opaque card background and a top border) that wraps when narrow, with 10px labels and 10px `Kbd` chips. This SHALL apply to the Files, History, Branches, Stashes, PRs list, Conflicts and PR viewer views and to the PR viewer file picker. The PR viewer hint bar SHALL list `j/k` (hunks), `a` (annotate the hunk), `Ctrl+←/→` (files) and `M` (merge).

#### Scenario: PR viewer shows its keys
- **GIVEN** the PR viewer shows an open PR with at least one hunk
- **THEN** its hint bar shows chips for `j`/`k`, `a`, `Ctrl+←`/`Ctrl+→` and `M` with their labels

#### Scenario: PRs list footer matches the other views
- **WHEN** the PRs list renders its footer
- **THEN** the footer is the shared hint bar, with 10px text and chips, pinned to the bottom of the list

#### Scenario: Narrow panel wraps the hint bar
- **GIVEN** the Branches view hint bar holds 7 hints
- **WHEN** the panel is narrower than the hints
- **THEN** the hints wrap onto a second row instead of being clipped

## MODIFIED Requirements

### Requirement: PRs chip is a picker → embedded viewer
The `PRs` chip SHALL render a file-picker-style PR list (j/k/Enter to select). Selecting a PR replaces the chip body with the `PrViewer` embedded in place plus a "All PRs" back-strip. `Backspace` returns to the picker.

#### Scenario: Backspace returns to picker
- **WHEN** the user is viewing a PR (`selected !== null`)
- **AND** presses `Backspace` outside an input/textarea
- **THEN** the chip returns to picker mode

#### Scenario: Apply with Claude tooltip on disabled
- **WHEN** the Apply-with-Agent button is disabled (no owning session, or owning session not active)
- **AND** the user hovers it
- **THEN** a Tooltip surfaces the specific gating reason (no matching branch / wrong active session)
- **AND** the button label includes the annotation count: `Apply with Agent (N)`

#### Scenario: Apply with Agent submits the prompt
- **GIVEN** the PR viewer has 2 annotations and the owning session is active
- **WHEN** the user clicks "Apply with Agent (2)" (or presses `Ctrl+Shift+R`)
- **THEN** `deliver_prompt_to_session` receives the structured prompt for that session
- **AND** the agent receives it as one turn, with no Enter needed from the user
- **AND** a toast titled "Apply with Agent" reports 2 annotations sent


===== specs/spec-annotations/spec.md =====
## MODIFIED Requirements

### Requirement: Send annotations to Claude via chat injection

The `SpecPanel` SHALL provide a "Send to Agent" action in the annotation toolbar that serializes all annotations for the currently viewed spec (or other artifact) and queues them into `HookState.pending_annotations` via the `set_pending_annotations` Tauri command. The serialized payload SHALL include the artifact's repo-relative path so Claude can read the file directly. Delivery SHALL happen via the existing `inject-edits` (`UserPromptSubmit`) hook — NOT via the plan-review FIFO.

#### Scenario: Send to Claude queues feedback

- **WHEN** user has one or more annotations on a spec and clicks "Send to Agent" (or presses `Ctrl+Shift+R`)
- **THEN** the system SHALL call `serializeSpecAnnotations(annotations, { changeName, artifactPath, isMaster })`
- **AND** the serialized result SHALL be passed to `set_pending_annotations`
- **AND** the local annotations for that scope SHALL be cleared (in-memory atom + SQLite for that `spec_key`)
- **AND** annotation mode SHALL exit
- **AND** a toast SHALL confirm "Feedback queued — Will be sent in your next prompt"

#### Scenario: Master spec vs change-scoped spec context

- **WHEN** the spec being annotated is under `openspec/specs/<capability>/spec.md` (master spec)
- **THEN** the serialized payload header SHALL read `Review the OpenSpec capability spec and address my annotations.`
- **AND** the `Change:` field SHALL read `(master specs)`
- **AND** the `File:` field SHALL contain the master-spec path
- **WHEN** the spec being annotated is under `openspec/changes/<change>/...`
- **THEN** the header SHALL read `Review the OpenSpec change artifact and address my annotations.`
- **AND** the `Change:` field SHALL contain the change slug
- **AND** the `File:` field SHALL contain the change-scoped path

#### Scenario: Next user prompt injects feedback

- **WHEN** the user sends their next message in the active session after queuing spec feedback
- **THEN** the `inject-edits` hook SHALL read `pending_annotations` and append the serialized feedback to the prompt submitted to Claude
- **AND** `pending_annotations` SHALL be cleared after injection


===== specs/terminal-wezterm/spec.md =====
## MODIFIED Requirements

### Requirement: Selection and clipboard
The terminal SHALL support mouse-drag selection, keyboard copy, and paste. A user paste SHALL be wrapped in bracketed-paste markers only while the running app has enabled bracketed paste (DECSET 2004). Otherwise the text SHALL be written without markers: every newline (`\r\n`, `\n` or `\r`) written as `\r`, then embedded bracketed-paste markers removed. A delivery to an agent session (`paste_to_session`, and the `deliver_prompt_to_session` command the git panel's "Apply with Agent" and "Ask Agent" use) SHALL stay wrapped whatever the mode, so a multi-line body arrives as one paste. `deliver_prompt_to_session` SHALL then write a single CR as a separate write after a settle delay, so the agent submits the whole paste as one turn.

#### Scenario: Drag to select
- **WHEN** the user mouse-downs on a cell and drags to another cell
- **THEN** the range SHALL be visually highlighted
- **AND** on mouseup the selection SHALL remain highlighted until a new selection is started or input clears it

#### Scenario: Copy selection to clipboard
- **WHEN** the user presses Ctrl+Shift+C (or the configured copy shortcut) with an active selection
- **THEN** the selected cells SHALL be serialized to text and written to the system clipboard

#### Scenario: Bracketed paste
- **GIVEN** the app wrote `ESC[?2004h`
- **WHEN** the user presses Ctrl+Shift+V with clipboard text `a` + LF + `b`
- **THEN** the PTY SHALL receive `ESC[200~a` + LF + `bESC[201~`

#### Scenario: Plain paste without DECSET 2004
- **GIVEN** the app has not enabled bracketed paste, or wrote `ESC[?2004l`
- **WHEN** the user pastes `a` + CR + LF + `b` + LF + `c` + `ESC[201~` + `d`
- **THEN** the PTY SHALL receive `a` + CR + `b` + CR + `cd`

#### Scenario: Agent deliveries stay bracketed
- **GIVEN** the agent session's app has not enabled bracketed paste
- **WHEN** `paste_to_session` or `deliver_prompt_to_session` delivers `x` + LF + `y`
- **THEN** the PTY SHALL receive `ESC[200~x` + LF + `yESC[201~`

#### Scenario: Prompt delivery submits after the paste
- **GIVEN** an agent session with a live PTY
- **WHEN** `deliver_prompt_to_session` delivers `x` + LF + `y`
- **THEN** the PTY SHALL first receive `ESC[200~x` + LF + `yESC[201~`
- **AND** about 250 ms later it SHALL receive one separate write holding only CR

#### Scenario: Prompt delivery to a shell tab is refused
- **WHEN** `deliver_prompt_to_session` targets an auxiliary shell id (one containing `::`)
- **THEN** it SHALL return an error and write nothing to any PTY

---