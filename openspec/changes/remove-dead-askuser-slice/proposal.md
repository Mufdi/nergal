## Why

A fully dead vertical slice survives from the abandoned ask-user dialog flow, and its module doc misdescribes the current architecture to every future reader. Ask-user has been notifier-only since v0.1.2 (CC's TUI owns the prompt; the hook fires a socket message and exits). Verified dead today:

- `src/components/session/AskUserModal.tsx:20-22` — `AskUserModal` renders `return null` (mounted at `src/App.tsx:162` for nothing); `AskUserModalLegacy` (24-337) has zero import sites.
- `src/stores/askUser.ts:19` — `askUserAtom` is read only by the dead modal and never `set` anywhere.
- `src-tauri/src/commands.rs:378` — `submit_ask_answer` (registered in `lib.rs:416`) has its only caller in the dead legacy component.
- `src-tauri/src/agents/mod.rs:643` — `AgentAdapter::submit_ask_answer` trait method (default `NotSupported`); only CC overrides it (`agents/claude_code/adapter.rs:300`), reading `pending_ask_fifos` that nothing populates in production (the non-blocking `nergal hook ask-user` creates no FIFO).
- `agents/claude_code/adapter.rs:5-20` — module doc still describes the mid-migration plan ("commit 4", "production call path") as if in flight.

**Requires user confirmation before implementation**: project memory records the legacy modal was deliberately "kept in case we revive the dialog flow". This change proposes committing to the notifier-only architecture and deleting the escape hatch (git history preserves it) — do not start implementation until that intent is confirmed.

## What Changes

- **Frontend**: delete `AskUserModalLegacy` + the null `AskUserModal` + its `App.tsx` mount (4, 162); delete `askUserAtom` and its types from `askUser.ts`. **Keep** `pendingAsksAtom`/`pendingAttentionAtom` (live — the tab-blink attention flow in `stores/hooks.ts:98-133`).
- **Backend**: delete `commands::submit_ask_answer` + its `invoke_handler` entry (`lib.rs:416`); delete the `AgentAdapter::submit_ask_answer` trait method (default + CC override) and the now-orphaned `pending_ask_fifos` map + its registration path (verify zero call sites first); rewrite `adapter.rs:5-20` module doc to describe the current architecture (plan FIFOs live, ask FIFOs gone, ask-user = non-blocking notifier).
- **Keep** the `ASK_USER_BLOCKING` capability flag: it still gates the attention-UX rendering and adapter capability declarations (design.md D2).

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `agent-adapter`: ask-user is a non-blocking notifier — the adapter interface carries no blocking ask-user answer round-trip.

## Impact

- **Deleted**: `AskUserModalLegacy` (~315 lines), `askUserAtom` plumbing, `submit_ask_answer` command + trait method + CC impl + `pending_ask_fifos`.
- **Edited**: `App.tsx` (unmount), `askUser.ts` (slim to the live atoms), `adapter.rs` module doc, `lib.rs` handler list.
- **Risk**: LOW mechanically (compiler catches every missed reference) — the real risk is intent (revival option foreclosed), gated on user confirmation above.
- **Out of scope**: the plan-review FIFO flow (alive and load-bearing); the attention/tint UX; any hook CLI change (`nergal hook ask-user` stays as the notifier).
