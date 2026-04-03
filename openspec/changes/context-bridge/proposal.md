## Why

Multiple Claude Code sessions running in parallel (e.g., backend + frontend, main + worktree) frequently produce context relevant to each other. Today the user manually routes context between sessions using a shared markdown file and the word "CAMBIO" — a protocol that works but requires constant human intervention. cluihud already owns every PTY and receives all hook events, making it the natural automatic router.

## What Changes

- New bidirectional communication system between Claude Code sessions with cluihud as autonomous router
- Two modes: **Channel** (autonomous back-and-forth conversation until CONSENSO) and **Quick Share** (one-way context push)
- Markdown file as message bus (`.claude/crossmsg-{channel-id}.md`) — doubles as audit log
- Detection via `PostToolUse` hook + file watcher when Claude writes to channel files
- Prompt injection into target session's PTY stdin, with state-awareness (idle → immediate, working → queue until `Stop`)
- HookState becomes session-scoped to support per-session pending context
- `inject_edits()` extended to also inject pending quick-share context
- New UI: channel creation (command palette + sidebar), quick share composer, channel viewer, pending-context badges

## Capabilities

### New Capabilities
- `cross-session-channel`: Autonomous bidirectional communication channels between two Claude Code sessions. Covers channel lifecycle (create, active, consensus/close), message bus file format, file watcher detection, and PTY stdin injection with state-aware queuing.
- `cross-session-quick-share`: One-way context push from one session to another. Covers composer UI, HookState injection via `UserPromptSubmit`, and pending-context indicators.

### Modified Capabilities
<!-- No existing spec requirements are changing. The hook pipeline extension (inject_edits) is an implementation detail, not a spec-level behavior change. -->

## Impact

- **Backend**: `hooks/state.rs` (session-scoped HookState), `hooks/cli.rs` (inject_edits extension), new `channels/` module (file watcher, PTY writer, queue), new Tauri commands
- **Frontend**: New `stores/channels.ts`, new channel UI components, SessionRow badges, command palette entries, keyboard shortcuts
- **File system**: New `.claude/crossmsg-*.md` files in project directories
- **Existing flows**: `inject_edits()` gains a new injection source (quick share context) alongside plan edits and annotations
