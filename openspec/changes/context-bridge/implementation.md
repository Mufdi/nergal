# Implementation Plan: Context Bridge

## Execution Order

1. HookState migration (backend) — foundation, all other changes depend on this
2. Quick Share backend + CLI extension
3. Quick Share frontend
4. Channel backend (file management + watcher)
5. Channel PTY injection + queue
6. Channel frontend
7. Integration testing

## 1. HookState Session-Scoping

### Files to modify
- `src-tauri/src/hooks/state.rs` — Core change

### Current state
`HookState` uses a single global file at `~/.claude/cluihud-state.json`. All methods are `Self`-based with no session awareness.

```rust
// state.rs:18 — current
fn state_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("home directory must exist")?;
    Ok(home.join(".claude").join("cluihud-state.json"))
}
```

### Changes

**`state_path()` → `state_path(session_id: &str)`**:
```rust
fn state_path(session_id: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("home directory must exist")?;
    let scoped = home.join(".claude").join(format!("cluihud-state-{session_id}.json"));
    if scoped.exists() {
        return Ok(scoped);
    }
    // Fallback: global file (backwards compat)
    let global = home.join(".claude").join("cluihud-state.json");
    if global.exists() {
        return Ok(global);
    }
    // Default to scoped path for new writes
    Ok(scoped)
}
```

For writes, always use scoped path (no fallback):
```rust
fn write_path(session_id: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("home directory must exist")?;
    Ok(home.join(".claude").join(format!("cluihud-state-{session_id}.json")))
}
```

**Add `pending_context` field**:
```rust
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HookState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_plan_edit: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_annotations: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_context: Option<ContextInjection>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContextInjection {
    pub from_session: String,
    pub summary: String,
    pub timestamp: u64,
}
```

**Update all public methods** (`read`, `write`, `take_pending_edit`, `set_pending_annotations`, `take_pending_annotations`) to accept `session_id: &str`.

### Callers to update
- `src-tauri/src/hooks/cli.rs:48` — `inject_edits()`: reads `CLUIHUD_SESSION_ID` from env, passes to `HookState::read(session_id)`
- `src-tauri/src/hooks/cli.rs:109` — `plan_review()`: not affected (doesn't use HookState)
- `src-tauri/src/commands.rs` — search for `HookState::` calls: `save_plan_edits` and `submit_plan_decision` commands write to HookState. These receive `session_id` from the frontend already — pass it through.

## 2. Quick Share Backend + CLI Extension

### Files to modify
- `src-tauri/src/hooks/cli.rs` — Extend `inject_edits()`
- `src-tauri/src/commands.rs` — New Tauri command

### `inject_edits()` extension (cli.rs:48)

After the existing plan_edit + annotations injection (line 67-77), add:

```rust
let context = state.pending_context.take();
// ... (write state back if context was Some)

if let Some(ctx) = context {
    let ago = format_relative_time(ctx.timestamp);
    parts.push(format!(
        "[Shared context from session \"{}\" ({} ago)]:\n{}",
        ctx.from_session, ago, ctx.summary
    ));
}
```

The `format_relative_time` helper: simple elapsed time formatter (seconds → "Xs", minutes → "Xm", etc.).

### New Tauri command

In `src-tauri/src/commands.rs`:

```rust
#[tauri::command]
pub fn quick_share(
    to_session_id: String,
    from_session_name: String,
    text: String,
) -> Result<(), String> {
    let injection = ContextInjection {
        from_session: from_session_name,
        summary: text,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    let mut state = HookState::read(&to_session_id).map_err(|e| e.to_string())?;
    state.pending_context = Some(injection);
    state.write(&to_session_id).map_err(|e| e.to_string())?;
    Ok(())
}
```

Register in `src-tauri/src/lib.rs` invoke_handler (line ~55).

### Emit event when context consumed

In `inject_edits()`, after consuming `pending_context`, we can't emit Tauri events (we're in CLI subprocess). Instead, the frontend polls `pendingContextMapAtom` or listens to `hook:event` for `user_prompt_submit` to clear the badge.

Alternative: `inject_edits()` outputs a marker in the modified JSON that the frontend can detect. Simpler: frontend clears the badge on `user_prompt_submit` event for the target session (the prompt was submitted, so inject_edits ran).

## 3. Quick Share Frontend

### Files to create
- `src/stores/quickShare.ts`
- `src/components/quickshare/QuickShareComposer.tsx`

### Files to modify
- `src/components/session/SessionRow.tsx` — pending badge
- `src/lib/shortcuts.ts` — register Ctrl+Shift+S
- `src/stores/hooks.ts` — clear pending on `user_prompt_submit`

### Store: `quickShare.ts`

```typescript
import { atom } from "jotai";

export interface PendingShare {
  fromSession: string;
  text: string;
  timestamp: number;
}

// Track pending shares per target session (for badge display)
export const pendingShareMapAtom = atom<Record<string, PendingShare>>({});

// Quick share composer open state
export const quickShareOpenAtom = atom(false);
```

### Composer component

Modal overlay with:
- `<textarea>` for context text
- Session picker (list active sessions from `workspacesAtom`, exclude current via `activeSessionIdAtom`)
- Send button → calls `invoke("quick_share", { toSessionId, fromSessionName, text })`
- On send: update `pendingShareMapAtom` for badge, close composer, show toast

### Badge in SessionRow

In `SessionRow.tsx`, after the session name `<span>` (line 109):
```tsx
{pendingShare && (
  <span className="size-1.5 rounded-full bg-blue-400" title="Context pending" />
)}
```

### Clear on prompt submit

In `src/stores/hooks.ts`, inside the `user_prompt_submit` case (line 75):
```typescript
case "user_prompt_submit": {
  // Clear pending quick share badge for this session
  set(pendingShareMapAtom, (prev) => {
    const next = { ...prev };
    delete next[sid];
    return next;
  });
  // ... existing activity logging
}
```

## 4. Channel Backend — File Management + Watcher

### Files to create
- `src-tauri/src/channels/mod.rs` — Channel struct, state, Tauri commands
- `src-tauri/src/channels/watcher.rs` — File watcher for crossmsg files

### Files to modify
- `src-tauri/src/lib.rs` — Register module, commands, watcher startup

### Channel struct

```rust
pub struct Channel {
    pub id: String,
    pub topic: String,
    pub participants: [String; 2],  // cluihud session IDs
    pub participant_names: [String; 2],
    pub status: ChannelStatus,
    pub file_path: PathBuf,
    pub created_at: u64,
}

pub enum ChannelStatus { Active, Closed }
```

### Channel state (in-memory)

```rust
pub struct ChannelManager {
    channels: Mutex<HashMap<String, Channel>>,
    injection_queue: Mutex<HashMap<String, PendingInjection>>,
}

struct PendingInjection {
    channel_id: String,
    prompt: String,
}
```

Managed as Tauri state (like `PtyManager`). Added in `lib.rs` `.manage()`.

### Tauri commands

- `create_channel(session_a_id, session_b_id, topic) -> Channel`
  - Generate UUID channel ID
  - Resolve session names from `db.find_session()`
  - Create `.claude/crossmsg-{id}.md` with header
  - Register in ChannelManager
  - Return channel

- `close_channel(channel_id)`
  - Mark as Closed in ChannelManager
  - Remove from injection queue

- `list_channels() -> Vec<Channel>`

### Channel file watcher

Follow the pattern from `src-tauri/src/claude/plan.rs:21-38`:

```rust
pub struct ChannelWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl ChannelWatcher {
    pub fn new(watch_dir: &Path, app: AppHandle, channels: Arc<ChannelManager>) -> Result<Self> {
        let mut watcher = notify::recommended_watcher(move |res| {
            let Ok(event) = res else { return };
            if event.kind.is_modify() {
                for path in event.paths {
                    if path.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("crossmsg-") && n.ends_with(".md"))
                    {
                        // Parse last message, determine sender, emit event
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let _ = app.emit("channel:file_changed", /* parsed data */);
                        }
                    }
                }
            }
        })?;
        watcher.watch(watch_dir, notify::RecursiveMode::NonRecursive)?;
        Ok(Self { _watcher: watcher })
    }
}
```

Startup in `lib.rs` setup closure, after the plan watcher block (~line 141). Watch `.claude/` directory. The watcher is `Box::leak`'d like the existing watchers.

### Message parser

Function to parse the last message block from a crossmsg file:

```rust
struct ParsedMessage {
    sender_name: String,
    timestamp: String,
    content: String,
    terminator: Terminator, // Cambio | Consenso | None
}
```

Regex or simple string scanning for `## {name} — {timestamp}` headers and `**CAMBIO**`/`**CONSENSO**` markers.

## 5. Channel PTY Injection + Queue

### Files to modify
- `src-tauri/src/channels/mod.rs` — injection logic
- `src-tauri/src/pty.rs` — already has `write_to_session_pty` (line 196)

### Key insight: `write_to_session_pty` already exists

`pty.rs:196-218` provides exactly what we need:
```rust
pub fn write_to_session_pty(state: State<'_, PtyManager>, session_id: String, data: String)
```

This writes arbitrary bytes to a session's PTY stdin. For channel injection, we call this with:
```
"Read the cross-session channel at {path}. Session \"{name}\" sent a new message. Read it, process, and respond by writing your reply to the same file. End with **CAMBIO** when done, or **CONSENSO** if consensus is reached.\r"
```

The `\r` is critical — it submits the prompt in xterm.js.

### State-aware injection

The channel watcher (backend) detects a file change and emits `channel:file_changed`. But the mode check (`modeMapAtom`) lives in the **frontend**. Two options:

**Option A**: Frontend listens to `channel:file_changed`, checks modeMap, and calls a Tauri command to inject or queue.
**Option B**: Backend maintains its own mode state (mirrors modeMap from hook events).

**Recommended: Option A** — simpler, avoids duplicating mode state in backend. The frontend already has `modeMapAtom` and receives all hook events. Flow:

1. Backend emits `channel:file_changed` with `{ channelId, senderSessionId, targetSessionId, terminator }`
2. Frontend `stores/channels.ts` listener:
   - If terminator is CONSENSO → update channel status to closed, show toast
   - If terminator is CAMBIO → check `modeMapAtom[targetSessionId]`
     - If "idle" → call `invoke("write_to_session_pty", { sessionId: targetSessionId, data: prompt })`
     - Else → store in `channelQueueAtom[targetSessionId]`
3. Frontend also listens to `hook:event` for `stop` events:
   - On stop for a session with queued injection → call `write_to_session_pty`, clear queue

This means the injection logic lives in the frontend store, not the backend. Clean separation: backend does file watching + parsing, frontend does state-aware routing.

### Queue atomics

```typescript
// stores/channels.ts
export const channelQueueAtom = atom<Record<string, { channelId: string; prompt: string }>>({});
```

In `stores/hooks.ts`, add to the `stop` case:
```typescript
case "stop": {
  // ... existing logic ...
  // Check channel queue
  const queue = get(channelQueueAtom);
  if (queue[sid]) {
    const { prompt } = queue[sid];
    invoke("write_to_session_pty", { sessionId: sid, data: prompt });
    set(channelQueueAtom, (prev) => { const next = {...prev}; delete next[sid]; return next; });
  }
}
```

## 6. Channel Frontend

### Files to create
- `src/stores/channels.ts` — Channel atoms + event listeners
- `src/components/channels/ChannelCreator.tsx` — Channel creation modal
- `src/components/channels/ChannelViewer.tsx` — Read-only markdown viewer for channel file

### Files to modify
- `src/components/session/SessionRow.tsx` — Channel participation badge
- `src/components/layout/Sidebar.tsx` — Channel indicator
- `src/stores/hooks.ts` — Queue drain on stop event
- Command palette entries (wherever command palette items are registered)

### Channel store

```typescript
export interface Channel {
  id: string;
  topic: string;
  participants: [string, string];
  participantNames: [string, string];
  status: "active" | "closed";
  filePath: string;
  messageCount: number;
}

export const channelsAtom = atom<Channel[]>([]);
export const channelQueueAtom = atom<Record<string, { channelId: string; prompt: string }>>({});
```

### Event listeners

Setup in a `setupChannelListeners(store)` function, called from the same place as `setupHookListeners` in the app initialization.

Listen to:
- `channel:file_changed` — route message or update state
- `channel:timeout` — show toast (if we add backend timeout)

### Channel viewer

Open as a tab in the right panel (same pattern as plan tabs):
```typescript
set(openTabAction, {
  tab: { id: `channel-${channelId}`, type: "channel", label: topic, data: { path: filePath } },
  isPinned: false,
});
```

Renderer: read file content, render markdown with session headers styled distinctly (like the plan viewer).

## Edge Cases

1. **Session killed mid-channel**: If a participant's PTY is killed, the channel stalls. The timeout (5min) notifies the user. They can manually close the channel.

2. **Both sessions idle simultaneously**: Only one should have a pending CAMBIO. The turn-based protocol ensures this — one writes CAMBIO, the other responds.

3. **Channel file deleted externally**: File watcher fires a remove event. Mark channel as closed, show toast.

4. **Quick share + channel on same session**: Both can coexist. Quick share is consumed on next `UserPromptSubmit` (before Claude acts). Channel injection is a PTY stdin write (triggers a new prompt). They don't conflict.

5. **HookState file permissions**: `~/.claude/` is user-owned. No permission issues expected. The CLI subprocess runs as the same user.
