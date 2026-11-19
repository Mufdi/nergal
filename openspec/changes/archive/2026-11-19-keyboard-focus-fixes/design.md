## Context

Three causes behind four symptoms, all on the keyboard path. See proposal.md. iprev round 1 (sonnet) rejected the first plan; this design records the decisions it forced.

## Goals / Non-Goals

**Goals:** fix the replayed signals, the lost yielded keys, and the halo after the first `Ctrl+<number>`, without widening the change.

**Non-Goals:** changing the terminal's fixed pass-through list for keys that no app-scope entry yields; a global DOM-focus to zone sync; mount-based Sidebar tests (Vitest runs without a DOM, and adding jsdom is a dependency decision for another change).

## Decisions

### D1. Consume-and-clear signals, not a watermark
- **Choice:** the consumer clears the signal atom when it acts (`consumeCounterSignal`, `consumeJumpSignal` in `src/stores/shortcutSignals.ts`). This matches `triggerResumeSessionAtom` (`Sidebar.tsx:408`).
- **Alternative:** a module-level "last consumed" watermark, as `lastNewSessionConsumed` does today. Rejected: hidden state that tests cannot reset, and the jump signal's `tick` is `Date.now()`, not a counter.
- Preconditions live in the helpers (`consumeCounterSignal(store, atom, canAct)`, `consumeJumpSignal(store, workspaceCount)`), so "wait while no workspaces are loaded" and "drop an out-of-range jump" are unit-tested. Each Sidebar effect becomes one call.

### D2. Targeted zone hand-off, not a global `focusin` sync
- **Choice:** the zone moves where the session actually becomes active, together with DOM focus.
  - Live branch of `switchToSession`: set `focusZoneAtom` to `terminal`. For the session that is already active, also call `terminalService.focusActive()`, because no effect will. For another session, `Workspace.tsx:161-165` moves DOM focus on later switches, and a terminal that mounts for the first time focuses itself (`TerminalManager.tsx:89,120`).
  - Resume branch: the `triggerResumeSessionAtom` consumer (`Sidebar.tsx:402-413`) sets the zone next to `setActiveSessionId`, only when it finds the session. The resumed terminal takes DOM focus when it mounts.
- **Alternative:** a `document` `focusin` listener that maps DOM focus to a zone. Rejected: `focusIfPanelZone` (BUG-09) relies on the zone staying `terminal` while restored panel content mounts, and five callers focus panels programmatically. A global sync would let any of them steal the zone.

### D3. Yield by event target, mark, and return
- **Choice:** a pure `shouldYieldToAgent({ ownership, scope, targetZone })` decides, with `targetZone = zoneOf(e.target)`. There is no fallback to `focusZoneAtom`: a target in no zone (focus orphaned to `<body>`) is not yielded, so Nergal keeps the key. When it yields, the dispatcher calls `yieldToAgent(e)` and returns. `shouldPassThrough` (moved to `passThrough.ts`) returns `false` first for a marked keydown.
- `return` instead of `continue` changes nothing reachable: the registry test "has no duplicate code+modifier combo" (`shortcuts.test.ts`) guarantees no later entry shares the combo.
- **Alternative:** remove the terminal's fixed list. Rejected for this change: some entries are webview defaults (copy), and their audit is separate work.
- The mark is a `WeakSet` entry, so it dies with the event.

### D4. Jump signals with an index out of range are dropped
- Once workspaces are loaded, a jump to an index with no workspace clears the signal and does nothing, so it cannot fire later when that workspace appears. With no workspaces loaded yet, the signal waits.

## Risks / Trade-offs

- [The Sidebar effects have no mount test.] → Their logic is one helper call each, and the helpers carry the preconditions under test; walks 4.2, 4.3 and 4.5 cover the remount.
- [iprev round 3 low items left open: `focusZoneSync.ts` keeps its name although it only exports `zoneOf`; the three fixes land in one commit; the `focusActive` call is not asserted.] → Accepted at the round cap for a low-risk change. Each fix is still a separate task group, and walk 4.5 checks DOM focus.
- [A press made while the sidebar is collapsed acts on the next expand, possibly much later.] → Kept as today's behavior and pinned by a scenario; dropping such presses is a separate UX decision.
- [In agent mode `Ctrl+B` now reaches Claude Code instead of doing nothing.] → That is the documented meaning of `keyboard_ownership: "agent"`; `leader shift+b` toggles the sidebar in both modes.
