# Implementation guide — opencode-adapter

> Companion to `tasks.md`. Read after `agent-adapter-foundation/implementation.md` — this change builds on the trait, registry, and capability gating defined there.

## Pre-flight

```bash
git checkout main && git pull
# Confirm foundation merged
grep -q "pub trait AgentAdapter" src-tauri/src/agents/mod.rs && echo "foundation merged ✓" || echo "MISSING — merge agent-adapter-foundation first"
git checkout -b opencode-adapter

# Install OpenCode for the spike
curl -fsSL https://opencode.ai/install | bash
opencode --version
opencode auth login   # configure your provider (Anthropic / OpenAI / etc.) — BYO credentials
```

## Spike phase (commit 0 — exploratory, may not merge)

The OpenCode SSE event schema is **not fully documented**. Capture it empirically before writing the parser.

```bash
# Terminal 1: start the server
opencode serve --port 14096   # use a high port to avoid collisions

# Terminal 2: tee SSE
mkdir -p docs/agents
curl -N http://127.0.0.1:14096/event | tee docs/agents/opencode-sse-raw.log

# Terminal 3: in another shell, trigger a small OpenCode session that exercises:
#   - a Bash tool call
#   - an Edit tool call
#   - a permission prompt (try a destructive operation to force one)
#   - a session.idle
opencode  # interactive session, ask it to "list files and edit a small README"
```

Read `opencode-sse-raw.log` and document each event type observed in `docs/agents/opencode-sse-schema.md`. Specifically capture:

- `tool.execute.before` — payload shape
- `tool.execute.after` — payload shape
- `permission.asked` — payload shape (does it include the permission_id explicitly? what's the response endpoint URL?)
- `permission.replied` — confirmation event
- `message.part.updated` — does it carry `usage` (cost/tokens)?
- `session.idle` — analog of CC Stop

The schema doc drives the parser implementation. If `usage` field is absent, declare `RAW_COST_PER_MESSAGE` off.

## Commit sequence

| # | Title | Purpose |
|---|---|---|
| 1 | `feat(opencode): adapter scaffold + supervisor skeleton` | Tasks 2.1–3.6. Process supervisor + adapter struct. |
| 2 | `feat(opencode): SSE client + transcript event mapping` | Tasks 4.1–4.5. Reads `/event`, translates to TranscriptEvent. |
| 3 | `feat(opencode): permission client (REST POST)` | Tasks 5.1–5.3. submit_ask_answer wired. |
| 4 | `feat(opencode): integrate adapter into registry + trait impl complete` | Tasks 6.1–6.7. |
| 5 | `feat(workspace): chat panel render routing for OpenCode sessions` | Tasks 7.1–7.3, 9.1–9.2. |
| 6 | `feat(chat): OpenCodeChat component (messages, tool cards, permission inline)` | Task 8.1–8.4. |
| 7 | `feat(settings): OpenCode settings panel (BYO credentials)` | Task 10.1–10.3. |
| 8 | `feat(opencode): orphan opencode-serve cleanup at startup` | Task 11.1–11.2. |
| 9 | `test(opencode): SSE parser + supervisor + e2e` | Task 12.1–12.4. |
| 10 | `docs(opencode): user-facing docs + CLAUDE.md update` | Task 13.1–13.3. |

## Commit 1 — Adapter scaffold + supervisor

### `src-tauri/src/agents/opencode/mod.rs`

```rust
pub mod server_supervisor;
pub mod sse_client;
pub mod permission_client;

pub struct OpenCodeAdapter {
    binary_path: parking_lot::RwLock<Option<PathBuf>>,
    config_paths: Vec<PathBuf>,  // ~/.config/opencode, ~/.local/share/opencode
    supervisor: Arc<server_supervisor::ServerSupervisor>,
    sse_clients: Arc<DashMap<String, sse_client::SseClient>>,
    pending_permissions: Arc<DashMap<String, permission_client::PendingPermission>>,
}

impl OpenCodeAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            binary_path: parking_lot::RwLock::new(which::which("opencode").ok()),
            config_paths: vec![home.join(".config/opencode"), home.join(".local/share/opencode")],
            supervisor: Arc::new(server_supervisor::ServerSupervisor::new()),
            sse_clients: Arc::new(DashMap::new()),
            pending_permissions: Arc::new(DashMap::new()),
        }
    }
}
```

### `src-tauri/src/agents/opencode/server_supervisor.rs`

The supervisor manages one `opencode serve` child per cluihud session. Use `tokio::process::Command`.

```rust
pub struct ServerInstance {
    pub child: tokio::process::Child,
    pub port: u16,
    pub started_at: std::time::Instant,
    pub pid_file: PathBuf,
}

pub struct ServerSupervisor {
    instances: DashMap<String, ServerInstance>,
}

impl ServerSupervisor {
    pub async fn start(&self, session_id: &str, binary: &Path) -> Result<u16> {
        let mut cmd = tokio::process::Command::new(binary);
        cmd.arg("serve").arg("--port").arg("0");  // request ephemeral port; if unsupported, range fallback
        cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn()?;

        // Read stdout for "listening on http://127.0.0.1:<port>"
        let stdout = child.stdout.take().expect("piped");
        let port = parse_port_from_stdout(stdout).await?;

        // Write PID file for orphan cleanup
        let pid_dir = dirs::state_dir().unwrap_or_default().join("cluihud/opencode-pids");
        tokio::fs::create_dir_all(&pid_dir).await?;
        let pid_file = pid_dir.join(format!("{}.pid", session_id));
        let parent_pid = std::process::id();
        let child_pid = child.id().unwrap_or(0);
        tokio::fs::write(&pid_file, format!("parent={}\nchild={}\n", parent_pid, child_pid)).await?;

        self.instances.insert(session_id.into(), ServerInstance {
            child, port, started_at: std::time::Instant::now(), pid_file,
        });
        Ok(port)
    }

    pub async fn stop(&self, session_id: &str) -> Result<()> {
        if let Some((_, mut inst)) = self.instances.remove(session_id) {
            // SIGTERM, then SIGKILL after 5s
            let _ = inst.child.start_kill();
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), inst.child.wait()).await;
            let _ = tokio::fs::remove_file(&inst.pid_file).await;
        }
        Ok(())
    }

    /// Run at app startup: PID files left by previous crashed cluihud must be cleaned.
    pub async fn cleanup_orphans() {
        let pid_dir = dirs::state_dir().unwrap_or_default().join("cluihud/opencode-pids");
        let mut entries = match tokio::fs::read_dir(&pid_dir).await { Ok(e) => e, Err(_) => return };
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(content) = tokio::fs::read_to_string(entry.path()).await {
                let parent_pid: u32 = content.lines().find(|l| l.starts_with("parent="))
                    .and_then(|l| l.strip_prefix("parent=")).and_then(|s| s.parse().ok()).unwrap_or(0);
                let child_pid: u32 = content.lines().find(|l| l.starts_with("child="))
                    .and_then(|l| l.strip_prefix("child=")).and_then(|s| s.parse().ok()).unwrap_or(0);
                // If parent_pid no longer alive, kill child
                if !proc_alive(parent_pid) && child_pid > 0 {
                    let _ = tokio::process::Command::new("kill").arg("-9").arg(child_pid.to_string()).status().await;
                    let _ = tokio::fs::remove_file(entry.path()).await;
                }
            }
        }
    }
}

fn proc_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{}", pid)).exists()  // Linux-only; cluihud is Linux-first
}
```

Hook `cleanup_orphans()` at app startup (in `lib.rs::app_setup`).

## Commit 2 — SSE client

### `src-tauri/src/agents/opencode/sse_client.rs`

```rust
use eventsource_stream::Eventsource;
use futures_util::StreamExt;

pub struct SseClient {
    handle: tokio::task::JoinHandle<()>,
    cancel: tokio::sync::oneshot::Sender<()>,
}

impl SseClient {
    pub async fn connect_and_run(
        url: String,
        session_id: String,
        sink: EventSink,
        pending: Arc<DashMap<String, PendingPermission>>,
    ) -> Result<Self> {
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            let client = reqwest::Client::new();
            let response = client.get(&url).send().await.expect("sse connect failed");
            let mut stream = response.bytes_stream().eventsource();
            loop {
                tokio::select! {
                    _ = &mut cancel_rx => break,
                    Some(event) = stream.next() => {
                        if let Ok(ev) = event {
                            if let Some(translated) = translate_sse_event(&ev, &session_id, &pending) {
                                let _ = sink.send(translated);
                            }
                        }
                    }
                }
            }
        });
        Ok(Self { handle, cancel: cancel_tx })
    }
}

fn translate_sse_event(
    ev: &eventsource_stream::Event,
    session_id: &str,
    pending: &DashMap<String, PendingPermission>,
) -> Option<FrontendHookEvent> {
    let payload: serde_json::Value = serde_json::from_str(&ev.data).ok()?;
    match ev.event.as_str() {
        "tool.execute.before" => {
            let name = payload.get("tool_name")?.as_str()?;
            let input = payload.get("input").cloned().unwrap_or_default();
            Some(FrontendHookEvent::ToolUse { session_id: session_id.into(), name: name.into(), input })
        }
        "tool.execute.after" => {
            let id = payload.get("tool_use_id")?.as_str()?;
            let output = payload.get("output").cloned().unwrap_or_default();
            Some(FrontendHookEvent::ToolResult { session_id: session_id.into(), tool_use_id: id.into(), output })
        }
        "permission.asked" => {
            let pid = payload.get("permission_id")?.as_str()?;
            let prompt = payload.get("prompt")?.as_str()?.to_string();
            // Store pending so submit_ask_answer can resolve the URL later
            pending.insert(session_id.into(), PendingPermission {
                permission_id: pid.into(),
                opencode_session_id: payload.get("session_id")?.as_str()?.into(),
                port: 0, // filled in by the adapter when registering
            });
            Some(FrontendHookEvent::AskUser { session_id: session_id.into(), prompt, decision_path: format!("opencode://{}/{}", session_id, pid) })
        }
        "session.idle" => Some(FrontendHookEvent::Stop { session_id: session_id.into() }),
        // Cost extraction depends on spike findings; if message.part.updated carries usage:
        "message.part.updated" => {
            let usage = payload.pointer("/message/usage")?;
            let raw = RawCost {
                model_id: payload.pointer("/message/model").and_then(|v| v.as_str().map(String::from)),
                input_tokens: usage.get("input_tokens")?.as_u64()?,
                output_tokens: usage.get("output_tokens")?.as_u64()?,
                cache_read_tokens: usage.get("cache_read_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                cache_write_tokens: usage.get("cache_write_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            };
            Some(FrontendHookEvent::Cost { session_id: session_id.into(), raw })
        }
        _ => None,  // unknown event types: ignore, don't error
    }
}
```

Adjust event names + payload paths according to your spike findings.

## Commit 3 — Permission client

```rust
pub struct PendingPermission {
    pub permission_id: String,
    pub opencode_session_id: String,
    pub port: u16,
}

pub async fn submit_response(
    pending: &PendingPermission,
    answer: &serde_json::Value,
) -> Result<()> {
    let url = format!(
        "http://127.0.0.1:{}/session/{}/permissions/{}",
        pending.port, pending.opencode_session_id, pending.permission_id
    );
    reqwest::Client::new()
        .post(&url)
        .json(answer)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
```

In the adapter:

```rust
async fn submit_ask_answer(&self, session_id: &str, answers: serde_json::Value) -> Result<(), AdapterError> {
    let pending = self.pending_permissions.get(session_id)
        .ok_or(AdapterError::Transport(anyhow!("no pending permission for session")))?;
    permission_client::submit_response(&pending, &answers).await
        .map_err(|e| AdapterError::Transport(e))?;
    self.pending_permissions.remove(session_id);
    Ok(())
}
```

## Commit 5 — Workspace render routing

### `src/components/layout/Workspace.tsx`

```typescript
const agentId = useAtomValue(agentAdapterAtom);

return (
  <div className="workspace">
    {/* sidebar, etc. */}
    <main>
      {agentId === "opencode"
        ? <OpenCodeChat sessionId={activeSessionId} />
        : <TerminalManager sessionId={activeSessionId} />}
    </main>
  </div>
);
```

## Commit 6 — Chat component

### `src/components/chat/OpenCodeChat.tsx`

Skeleton:

```typescript
export function OpenCodeChat({ sessionId }: { sessionId: string }) {
  const events = useAtomValue(agentEventStreamAtomFamily(sessionId));

  return (
    <div className="flex flex-col gap-2 p-4 overflow-y-auto h-full">
      {events.map((ev, i) => {
        switch (ev.kind) {
          case "message":   return <MessageBubble key={i} role={ev.role} content={ev.content} />;
          case "tool_use":  return <ToolUseCard   key={i} name={ev.name} input={ev.input} />;
          case "tool_result": return <ToolResultCard key={i} output={ev.output} />;
          case "ask_user":  return <PermissionPromptInline key={i} prompt={ev.prompt} sessionId={sessionId} />;
          default: return null;
        }
      })}
    </div>
  );
}
```

The components themselves are simple — keep them lightweight. Use existing shadcn/ui primitives (Card, Badge).

## Verification

```bash
# Backend
cargo test agents::opencode:: 2>&1 | tail -10
cargo clippy -- -D warnings

# Manual: real OpenCode session
opencode --version
# Open cluihud, create new session with OpenCode, run a small task end-to-end
# Verify: chat renders, tool cards expand, permission inline modal works, no terminal canvas visible
```

## Common pitfalls

- **`opencode serve --port 0`**: if not supported by the OpenCode version, fall back to scanning ports 49152–65535.
- **SSE reconnection**: if `opencode serve` crashes or the connection drops, reconnect with exponential backoff. Don't tear down the cluihud session.
- **Permission ID expiry**: OpenCode permissions might time out server-side. Handle 404 from POST gracefully.
- **PID file race on cleanup**: the orphan cleaner reads PID files at startup, but a concurrent supervisor might be writing one. Use atomic writes (`tokio::fs::write` is atomic on rename, but here we're not renaming — write to temp + rename if you see flakiness).
- **Process orphan on macOS/BSD**: `/proc/<pid>` doesn't exist outside Linux. Cluihud is Linux-first per CLAUDE.md, so this is fine, but document for future portability.
