## Context

cluihud runs multiple Claude Code sessions in parallel, each in its own PTY with a unique `CLUIHUD_SESSION_ID`. The app already intercepts hook events via a Unix socket, watches files via `notify`, and injects content into prompts via `inject_edits()` on `UserPromptSubmit`. A manual cross-session protocol exists (`cross-session-protocol.md`) using a shared markdown file with CAMBIO/CONSENSO markers — validated in a real debugging session (`cross-session.md`) where two sessions collaborated to fix a hook routing bug.

Key existing infrastructure:
- `HookState` (`~/.claude/cluihud-state.json`): IPC state between GUI and CLI hooks, take-on-read pattern
- `inject_edits()` in `cli.rs`: Mutates `UserPromptSubmit` payload with pending plan edits + annotations
- `modeMapAtom`: Tracks per-session mode (idle/active/tool name) — used for state-aware injection
- File watcher (`notify`): Already watches plan files, can extend to channel files
- PTY handles: App holds `CommandBuilder`/writer for each session — can write to stdin

## Goals / Non-Goals

**Goals:**
- Automate cross-session context sharing with cluihud as the router
- Two modes: Channel (autonomous bidirectional) and Quick Share (one-way push)
- State-aware injection: only inject when target session is idle
- Channel files serve as both message bus and readable audit log
- Minimal disruption to existing hook pipeline

**Non-Goals:**
- Multi-party channels (3+ sessions) — MVP is strictly 2 participants
- Auto-detection of relevant context from transcripts
- Channel persistence across app restarts
- Real-time streaming between sessions (turn-based is sufficient)
- Replacing or modifying the Claude Code hook protocol itself

## Decisions

### 1. Session-scoped HookState

**Decision**: Migrate from single `~/.claude/cluihud-state.json` to per-session `~/.claude/cluihud-state-{session_id}.json`.

**Why**: Quick Share injects context via `inject_edits()`, which reads HookState. With a single file, concurrent sessions would race on read/write. Per-session files eliminate contention.

**Alternatives considered**:
- *Mutex/lock on shared file*: Adds complexity, still serial. Rejected.
- *In-memory state via Tauri event*: Would bypass the existing CLI → file → CLI pipeline. The CLI binary (`cluihud hook inject-edits`) runs as a subprocess — it can't receive Tauri events, only read files. Rejected.
- *SQLite row per session*: Over-engineered for key-value state with take-on-read semantics. Rejected.

**Migration**: `HookState::state_path()` becomes `HookState::state_path(session_id)`. The `inject_edits()` CLI reads `CLUIHUD_SESSION_ID` from env (already available) to resolve the correct file. Old global file is ignored after migration.

### 2. Channel file as message bus

**Decision**: Use `.claude/crossmsg-{channel_id}.md` files in the project root as the communication medium.

**Why**: Claude Code already knows how to read and write files. No new tool or protocol needed — just instruct it to append to a markdown file. The file is simultaneously a message bus and human-readable audit log.

**Format**:
```markdown
# Channel: {topic}
Participants: {session_a_name}, {session_b_name}
Created: {ISO timestamp}

---
## {Session Name} — {ISO timestamp}

{Content}

**CAMBIO**

---
## {Session Name} — {ISO timestamp}

{Content}

**CONSENSO**
```

**Alternatives considered**:
- *Unix socket between sessions*: Claude Code can't write to sockets. Rejected.
- *Tauri events*: Same subprocess problem as above — CLI hooks can't participate. Rejected.
- *SQLite messages table*: Claude can't query SQLite. The beauty of a file is Claude reads it natively. Rejected.

### 3. PTY stdin injection for autonomous routing

**Decision**: When a channel message is detected, cluihud writes a prompt directly to the target session's PTY stdin.

**Injection prompt**:
```
Read the cross-session channel at {path}. Session "{sender_name}" sent a new message. Read the file, process the latest message, and write your response to the same file. End with **CAMBIO** when done, or **CONSENSO** if consensus is reached.\r
```

**Why**: This is the only way to trigger Claude Code to act without user input. The `\r` simulates Enter in the terminal.

**State guard**: Only inject when `modeMapAtom[session_id] === "idle"`. If working, queue the injection and subscribe to the `Stop` hook event for that session.

**Alternatives considered**:
- *inject_edits() on next UserPromptSubmit*: Only fires when user/Claude sends a prompt — in an idle session, this never happens. Not suitable for autonomous routing. Rejected for channels (but used for Quick Share).
- *CLAUDE.md instruction*: Tell Claude to poll the file. Wastes context window, unreliable timing. Rejected.

### 4. Dual-mode: Channel vs Quick Share

**Decision**: Two distinct mechanisms sharing the same infrastructure.

| Aspect | Channel | Quick Share |
|--------|---------|-------------|
| Direction | Bidirectional, autonomous | One-way push |
| Trigger | Explicit channel creation | Shortcut (Ctrl+Shift+S) |
| Delivery | PTY stdin injection | `inject_edits()` on next prompt |
| Detection | File watcher on crossmsg file | HookState pending_context field |
| Lifecycle | Create → CAMBIO loop → CONSENSO | Fire and forget |
| File | `.claude/crossmsg-{id}.md` | No file (HookState only) |

**Why separate**: Quick Share is simpler (no file, no watcher, no PTY write) and covers the 80% case of "send this context to that session". Channels are for the 20% case of collaborative debugging/planning.

### 5. File watcher integration

**Decision**: Extend the existing `notify` file watcher to watch `.claude/crossmsg-*.md` files.

**Detection logic**:
1. On file modify event, read the file
2. Parse last `## {name} — {timestamp}` block
3. Match sender name to a session ID
4. If last block ends with CAMBIO, route to the other participant
5. If CONSENSO, mark channel as closed

**Why reuse notify**: Already in the dependency tree, already used for plan file watching. Same pattern, different glob.

### 6. Queue system for working sessions

**Decision**: In-memory queue in the Tauri backend. One pending injection per session.

**Flow**:
1. Channel message detected → check target session mode
2. If idle → inject immediately via PTY stdin
3. If not idle → store `PendingInjection { channel_id, prompt }` in a `HashMap<SessionId, PendingInjection>`
4. On `Stop` event for that session → pop and inject
5. If a new message arrives while one is already queued, replace it (latest message wins — Claude will read the full file anyway)

**Why in-memory**: Channels are ephemeral. No need to persist the queue across restarts.

## Risks / Trade-offs

**[Risk] Claude ignores the injected prompt or doesn't follow the format** → Mitigation: The injection prompt is explicit and imperative. If Claude doesn't write CAMBIO/CONSENSO, the channel stalls. User can manually close the channel from UI. Add a timeout (configurable, default 5min) that notifies the user if no response is detected.

**[Risk] Race condition on channel file** → Mitigation: Only one session writes at a time (turn-based). The state guard ensures the target session is idle before injection. File watcher debounce (500ms) prevents partial-write detection.

**[Risk] PTY stdin injection corrupts terminal state** → Mitigation: Only inject when session is idle (waiting for user input at the prompt). The `\r` character submits the prompt cleanly. If the session is in a weird state (e.g., vim open), the mode won't be "idle" so injection won't fire.

**[Risk] Context window bloat** → Mitigation: Each channel read consumes context. For long conversations, Claude's own context compaction handles this. The channel file grows linearly — for very long exchanges (10+ turns), the file itself becomes large. Mitigation: document a soft limit of ~10 turns, suggest closing and opening a new channel.

**[Risk] HookState migration breaks existing plan edit flow** → Mitigation: `inject_edits()` already reads `CLUIHUD_SESSION_ID` from env. Changing the file path is transparent to the rest of the pipeline. Add fallback: if session-scoped file doesn't exist, try the global file (backwards compat during rollout).

**[Trade-off] Quick Share uses inject_edits (passive) vs Channel uses PTY stdin (active)** → Accepted. Quick Share doesn't need immediacy — it's "next time you prompt, here's some context". Channels need immediacy for autonomous back-and-forth. Two injection mechanisms is justified by the different timing requirements.
