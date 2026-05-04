# Implementation guide — pi-adapter

> Companion to `tasks.md`. Read after `agent-adapter-foundation/implementation.md`. Pi is the simplest adapter to implement (read-only observation, no blocking) but its session-file resolution is fiddly.

## Pre-flight

```bash
git checkout main && git pull
grep -q "pub trait AgentAdapter" src-tauri/src/agents/mod.rs && echo "foundation merged ✓"
git checkout -b pi-adapter

# Install Pi for the spike
npm install -g @mariozechner/pi-coding-agent
pi --version
# Configure credentials per Pi docs (env vars or ~/.pi/agent/config — Pi-specific, NOT cluihud)
```

## Spike phase (commit 0)

```bash
# 1. Run a small Pi session in a known cwd
mkdir -p /tmp/pi-test && cd /tmp/pi-test
pi   # interactive — ask it to "list files and create a hello.txt"
# 2. Locate the JSONL
find ~/.pi/agent/sessions -name "*.jsonl" -newer /tmp/pi-test 2>/dev/null
# 3. Copy fixture
mkdir -p src-tauri/tests/fixtures/pi
cp ~/.pi/agent/sessions/--tmp-pi-test--/*.jsonl src-tauri/tests/fixtures/pi/session.jsonl
# 4. Inspect entries
jq -c 'select(.type)' src-tauri/tests/fixtures/pi/session.jsonl | head -30
```

Document each `.type` value observed in `docs/agents/pi-jsonl-schema.md`. Confirm:

- `session` header on line 1 with `id` (UUID), `version`, `cwd`
- `agent` entries with `role: assistant` carrying `usage` field — capture exact field names
- `tool_call` and `tool_result` shapes
- Path encoding: confirm `~/.pi/agent/sessions/--<cwd-slashes-as-dashes>--/` matches your test cwd literally

## Commit sequence

| # | Title | Purpose |
|---|---|---|
| 1 | `feat(pi): adapter scaffold + session resolver` | Tasks 2.1–3.4. Encode cwd, find newest JSONL. |
| 2 | `feat(pi): JSONL tail watcher` | Tasks 4.1–4.6. Notify-based tail-f with offset. |
| 3 | `feat(pi): transcript parser` | Tasks 5.1–5.6. Map entry types to TranscriptEvent. |
| 4 | `feat(pi): adapter trait impl complete` | Tasks 6.1–6.7. |
| 5 | `feat(pi): UUID persistence for resume` | Tasks 8.1–8.4. |
| 6 | `feat(settings): Pi settings panel (read-only, BYO credentials)` | Task 10.1–10.2. |
| 7 | `test(pi): parser + tail watcher + e2e` | Task 11.1–11.5. |
| 8 | `docs(pi): user-facing docs` | Task 12.1–12.2. |

## Commit 1 — Adapter + session resolver

### `src-tauri/src/agents/pi/mod.rs`

```rust
pub mod jsonl_tail;
pub mod transcript;
pub mod session_resolver;

pub struct PiAdapter {
    binary_path: parking_lot::RwLock<Option<PathBuf>>,
    state_dir: PathBuf,  // ~/.pi/agent
    tails: Arc<DashMap<String, jsonl_tail::JsonlTailHandle>>,
}

impl PiAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            binary_path: parking_lot::RwLock::new(which::which("pi").ok()),
            state_dir: home.join(".pi/agent"),
            tails: Arc::new(DashMap::new()),
        }
    }
}
```

### `src-tauri/src/agents/pi/session_resolver.rs`

```rust
/// Pi encodes cwd as slashes-to-dashes wrapped in double-dashes.
/// /home/user/projects/foo  →  --home-user-projects-foo--
pub fn encode_cwd_to_pi_path(cwd: &Path) -> String {
    let mut s = String::from("--");
    let mut first = true;
    for component in cwd.components() {
        if let std::path::Component::Normal(c) = component {
            if !first { s.push('-'); }
            s.push_str(&c.to_string_lossy());
            first = false;
        }
    }
    s.push_str("--");
    s
}

/// After spawning Pi, poll the encoded sessions dir for the newest .jsonl file.
pub async fn wait_for_jsonl(
    sessions_dir: &Path,
    timeout: std::time::Duration,
) -> Result<PathBuf> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if let Ok(mut entries) = tokio::fs::read_dir(sessions_dir).await {
            let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.path().extension().map(|e| e == "jsonl").unwrap_or(false) {
                    if let Ok(meta) = entry.metadata().await {
                        if let Ok(mtime) = meta.modified() {
                            if latest.as_ref().map(|(_, t)| mtime > *t).unwrap_or(true) {
                                latest = Some((entry.path(), mtime));
                            }
                        }
                    }
                }
            }
            if let Some((path, _)) = latest {
                return Ok(path);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(anyhow!("no .jsonl file appeared in {:?} within {:?}", sessions_dir, timeout))
}

/// Read the first line of a JSONL session, parse the `session` header, return Pi's UUID.
pub async fn extract_pi_session_uuid(jsonl_path: &Path) -> Result<String> {
    let content = tokio::fs::read_to_string(jsonl_path).await?;
    let first_line = content.lines().next().ok_or(anyhow!("empty jsonl"))?;
    let entry: serde_json::Value = serde_json::from_str(first_line)?;
    if entry.get("type").and_then(|v| v.as_str()) != Some("session") {
        return Err(anyhow!("first line is not a session header"));
    }
    entry.get("id").and_then(|v| v.as_str()).map(String::from)
        .ok_or(anyhow!("session header missing id"))
}
```

Test these with multiple cwds — including paths with spaces, deep paths, and `.` segments. Document any encoding quirks discovered in the spike.

## Commit 2 — JSONL tail watcher

### `src-tauri/src/agents/pi/jsonl_tail.rs`

```rust
pub struct JsonlTailHandle {
    pub cancel: tokio::sync::oneshot::Sender<()>,
    pub join: tokio::task::JoinHandle<()>,
}

pub async fn start_tail<F>(
    path: PathBuf,
    session_id: String,
    sink: EventSink,
    parse_line: F,
) -> Result<JsonlTailHandle>
where
    F: Fn(&str) -> Option<TranscriptEvent> + Send + 'static,
{
    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel();

    let join = tokio::spawn(async move {
        let mut last_offset = 0u64;
        let mut file = match tokio::fs::OpenOptions::new().read(true).open(&path).await {
            Ok(f) => f, Err(e) => { tracing::error!("failed to open jsonl: {e}"); return; }
        };

        // Catch-up: read existing content
        last_offset = read_lines_from_offset(&mut file, &mut last_offset, &parse_line, &sink, &session_id).await;

        // Set up notify
        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Ok(event) = res { let _ = notify_tx.send(event); }
        }).expect("notify watcher");
        notify::Watcher::watch(&mut watcher, &path, notify::RecursiveMode::NonRecursive).ok();

        loop {
            tokio::select! {
                _ = &mut cancel_rx => break,
                Some(event) = notify_rx.recv() => {
                    if matches!(event.kind, notify::EventKind::Modify(_)) {
                        last_offset = read_lines_from_offset(&mut file, &mut last_offset, &parse_line, &sink, &session_id).await;
                    }
                }
            }
        }
    });

    Ok(JsonlTailHandle { cancel: cancel_tx, join })
}

async fn read_lines_from_offset<F>(
    file: &mut tokio::fs::File,
    last_offset: &mut u64,
    parse_line: &F,
    sink: &EventSink,
    session_id: &str,
) -> u64
where F: Fn(&str) -> Option<TranscriptEvent>
{
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let _ = file.seek(std::io::SeekFrom::Start(*last_offset)).await;
    let mut buf = String::new();
    let _ = file.read_to_string(&mut buf).await;
    let new_offset = *last_offset + buf.len() as u64;
    for line in buf.lines() {
        if let Some(ev) = parse_line(line) {
            // Translate to FrontendHookEvent and send to sink
            // ... (omitted for brevity — wraps in event::Cost / ToolUse / etc.)
        }
    }
    new_offset
}
```

Test 11.4 verifies offset tracking — write multiple lines with delays, assert each emits exactly once.

## Commit 3 — Transcript parser

### `src-tauri/src/agents/pi/transcript.rs`

```rust
#[derive(serde::Deserialize)]
#[serde(tag = "type")]
enum PiEntry {
    #[serde(rename = "session")]
    Session { id: String, version: u32, cwd: String, /* ... */ },
    #[serde(rename = "agent")]
    Agent {
        role: String,
        content: serde_json::Value,
        #[serde(default)]
        usage: Option<PiUsage>,
        #[serde(default)]
        model: Option<String>,
    },
    #[serde(rename = "tool_call")]
    ToolCall { id: String, name: String, arguments: serde_json::Value },
    #[serde(rename = "tool_result")]
    ToolResult { #[serde(rename = "toolCallId")] tool_call_id: String, output: serde_json::Value },
    #[serde(rename = "compaction")]
    Compaction { /* ... */ },
    // model_change, branch_summary, etc.
    #[serde(other)]
    Unknown,
}

#[derive(serde::Deserialize)]
struct PiUsage {
    input: u64,
    output: u64,
    #[serde(rename = "cacheRead")]
    cache_read: u64,
    #[serde(rename = "cacheWrite")]
    cache_write: u64,
    // discard cost.usd per Decision 6
}

pub fn parse_transcript_line(line: &str) -> Option<TranscriptEvent> {
    let entry: PiEntry = serde_json::from_str(line).ok()?;
    match entry {
        PiEntry::Agent { role, content, usage, model } if role == "assistant" => {
            if let Some(u) = usage {
                return Some(TranscriptEvent::Cost(RawCost {
                    model_id: model,
                    input_tokens: u.input,
                    output_tokens: u.output,
                    cache_read_tokens: u.cache_read,
                    cache_write_tokens: u.cache_write,
                }));
            }
            Some(TranscriptEvent::Message {
                role,
                content: content.to_string(),
                model,
            })
        }
        PiEntry::ToolCall { name, arguments, .. } => {
            Some(TranscriptEvent::ToolUse { name, input: arguments })
        }
        PiEntry::ToolResult { tool_call_id, output } => {
            Some(TranscriptEvent::ToolResult { tool_use_id: tool_call_id, output })
        }
        _ => None,
    }
}
```

## Commit 4 — Trait impl

```rust
#[async_trait::async_trait]
impl AgentAdapter for PiAdapter {
    fn id(&self) -> AgentId { AgentId::pi() }
    fn display_name(&self) -> &str { "Pi" }
    fn capabilities(&self) -> &AgentCapabilities {
        // Note: NO PLAN_REVIEW, NO ASK_USER_BLOCKING, NO TASK_LIST, NO ANNOTATIONS_INJECT
        static CAPS: AgentCapabilities = AgentCapabilities {
            flags: AgentCapability::TOOL_CALL_EVENTS
                .union(AgentCapability::STRUCTURED_TRANSCRIPT)
                .union(AgentCapability::RAW_COST_PER_MESSAGE)
                .union(AgentCapability::SESSION_RESUME),
            supported_models: vec![],
        };
        &CAPS
    }
    fn transport(&self) -> Transport {
        Transport::JsonlTail { sessions_dir: self.state_dir.join("sessions") }
    }
    fn requires_cluihud_setup(&self) -> bool { false }

    async fn detect(&self) -> DetectionResult {
        let bp = self.binary_path.read().clone();
        DetectionResult {
            installed: self.state_dir.exists() || bp.is_some(),
            binary_path: bp,
            config_path: if self.state_dir.exists() { Some(self.state_dir.clone()) } else { None },
            version: None,
            trusted_for_project: None,
        }
    }

    fn spawn(&self, ctx: &SpawnContext) -> Result<SpawnSpec, AdapterError> {
        let binary = self.binary_path.read().clone()
            .ok_or(AdapterError::Transport(anyhow!("pi binary not found")))?;
        let mut args = vec![];
        if let Some(uuid) = ctx.resume_from {
            args.push("--resume".into());
            args.push(uuid.into());
        }
        let mut env = HashMap::new();
        env.insert("CLUIHUD_SESSION_ID".into(), ctx.session_id.into());
        Ok(SpawnSpec { binary, args, env })
    }

    fn parse_transcript_line(&self, line: &str) -> Option<TranscriptEvent> {
        transcript::parse_transcript_line(line)
    }

    async fn start_event_pump(&self, session_id: &str, sink: EventSink) -> Result<(), AdapterError> {
        // Resolve session JSONL path. Need cwd — fetch from session row via DB.
        let cwd = fetch_session_cwd(session_id).await?;  // helper
        let encoded = session_resolver::encode_cwd_to_pi_path(&cwd);
        let sessions_dir = self.state_dir.join("sessions").join(&encoded);
        let jsonl = session_resolver::wait_for_jsonl(&sessions_dir, std::time::Duration::from_secs(2))
            .await.map_err(|e| AdapterError::Transport(e))?;

        // Persist Pi UUID for resume
        if let Ok(uuid) = session_resolver::extract_pi_session_uuid(&jsonl).await {
            persist_agent_internal_session_id(session_id, &uuid).await?;
        }

        let parse = |line: &str| transcript::parse_transcript_line(line);
        let handle = jsonl_tail::start_tail(jsonl, session_id.into(), sink, parse).await
            .map_err(|e| AdapterError::Transport(e))?;
        self.tails.insert(session_id.into(), handle);
        Ok(())
    }

    async fn stop_event_pump(&self, session_id: &str) -> Result<(), AdapterError> {
        if let Some((_, handle)) = self.tails.remove(session_id) {
            let _ = handle.cancel.send(());
            let _ = handle.join.await;
        }
        Ok(())
    }

    // submit_plan_decision and submit_ask_answer use the trait's default impls,
    // returning NotSupported.
}
```

## Verification

```bash
# Backend
cargo test agents::pi:: 2>&1 | tail -10
# Specifically verify the encoder round-trips

# Manual: real Pi session
pi --version
# In cluihud, create session with Pi, watch a tool call execute, observe activity drawer fill
# Verify: PlanPanel hidden, AskUserModal hidden (capability gating from foundation works)
# Verify: status bar shows token totals (no USD for Pi)
```

## Common pitfalls

- **Path encoding edge cases**: paths with non-ASCII or spaces in cwd may have a different encoding than the simple slashes-as-dashes convention. Verify against fixtures from real Pi runs.
- **JSONL race**: if Pi creates the file with delay, `wait_for_jsonl` can time out. 2s default; configurable if needed.
- **JSONL truncation**: Pi might rotate or truncate the file (rare but defensive). On `EventKind::Remove`, gracefully tear down the tail.
- **`AgentCapability` union in const**: Rust's bitflags v2 allows `.union()` in const context; v1 may not. Pinned `bitflags = "2"` in foundation.
- **Cost USD = 0 for Pi**: by design, `get_session_cost_usd` returns `None` for Pi sessions. The status bar correctly hides USD without code changes.
