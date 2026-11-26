Linear: FRI-623

## Why

Nergal guesses each session's state from hooks, and the guess has gaps. After `Stop`, a session with background commands, monitors or MCP tasks still works, but Nergal shows it idle. The folder-trust prompt waits for the user, and no hook reports it. Claude Code 2.1.295 and later compute that state themselves and write it to the PTY as OSC 7501 (Program Status Protocol), but only when the terminal answers the protocol's query.

## What Changes

- The terminal emulator answers Claude Code's OSC 7501 query (`ESC ] 7501 ; ?`), so Claude Code turns the protocol on.
- The emulator reads OSC 7501 status reports from the PTY output and never draws them.
- Each agent session keeps the last root status Claude Code reported: `working`, `blocked`, `done`, `idle` or `error`.
- Session tabs, sidebar rows and the collapsed sidebar take the session state from that status. Hooks still supply detail: the running tool, plan review and AskUserQuestion.
- Two new indicator states: `done` (the task finished) and `error` (the last turn failed). Today both look like `idle`.
- The MCP session directory (`list_sessions`, `get_session`) takes `mode` and `waiting_for` from the same status, with the `mode` enum unchanged.
- Cross-session delivery follows the reported status: it never pastes into a dialog (`blocked`) or a running turn, and a session that starts accepting a prompt gets its queued messages.
- A `blocked` status also sets the pending tint on the tab and the sidebar row. It sends no desktop notification, because the hooks already notify for permission prompts and questions.
- Sessions that never send OSC 7501 (older Claude Code, frozen agents) keep today's hook-derived state.
- Out of scope: child entries (subagents and background tasks reported with an `id`), and progress values.

## Capabilities

### New Capabilities

- `program-status-protocol`: how Nergal answers the OSC 7501 query, parses status reports, and keeps one root status per agent session.

### Modified Capabilities

- `terminal-wezterm`: the emulator answers the OSC 7501 query and consumes status reports without drawing them.
- `session-state-indicators`: the indicator resolves its state from the reported status when there is one, and gains the `done` and `error` states.
- `session-directory`: the descriptor's `mode` and `waiting_for` follow the reported status when there is one.
- `cross-session-messaging`: every PTY delivery path uses one busy rule that follows the reported status, and a busy to not-busy change drains the queue.

## Impact

- Rust: `src-tauri/src/terminal/session.rs` (byte pre-scan next to the DEC 2026 tracking), `src-tauri/src/pty.rs` (one write queue for emulator answers, reader thread forwards reports), `src-tauri/src/mcp/messaging.rs` (busy check, drain on leaving `blocked`), `src-tauri/src/agents/state.rs` (status per session), `src-tauri/src/mcp/directory.rs` (mode mapping).
- New Tauri event carrying a session's reported status to React.
- React: `src/stores/` (status atom and listener), `src/components/session/SessionIndicator.tsx` (resolution and the two new states), `TopBar.tsx` and `SessionRow.tsx` (pending tint), `src/styles/globals.css` only if a new state needs a class.
- No new dependency, no schema change, no change to the shortcuts.
- Claude Code without OSC 7501, Codex, OpenCode and Pi: behavior unchanged.
