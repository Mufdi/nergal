# Implementation guide — codex-adapter

> Companion to `tasks.md`. Read after `agent-adapter-foundation/implementation.md`. Codex is the easiest adapter — it reuses `Transport::FileHooks` infrastructure validated by CC. Most work is parsing the rollout JSONL and writing the hooks.json setup.

## Pre-flight

```bash
git checkout main && git pull
grep -q "pub trait AgentAdapter" src-tauri/src/agents/mod.rs && echo "foundation merged ✓"
git checkout -b codex-adapter

# Install Codex
npm install -g @openai/codex   # or brew install --cask codex
codex --version
codex login   # configure your provider (OpenAI / Azure OAI)
```

## Spike phase (commit 0)

Codex's hook payloads and rollout schema differ subtly from CC; capture before parsing.

```bash
# 1. Write a probe hooks.json that just tees stdin to a log
mkdir -p docs/agents
cat > ~/.codex/hooks.json <<'EOF'
{
  "hooks": {
    "PreToolUse":         [{ "matcher": "*", "hooks": [{ "type": "command", "command": "tee -a /tmp/codex-pretool.log" }] }],
    "PostToolUse":        [{ "matcher": "*", "hooks": [{ "type": "command", "command": "tee -a /tmp/codex-posttool.log" }] }],
    "PermissionRequest":  [{ "matcher": "*", "hooks": [{ "type": "command", "command": "tee -a /tmp/codex-perm.log" }] }],
    "UserPromptSubmit":   [{ "matcher": "*", "hooks": [{ "type": "command", "command": "tee -a /tmp/codex-prompt.log" }] }],
    "Stop":               [{ "matcher": "*", "hooks": [{ "type": "command", "command": "tee -a /tmp/codex-stop.log" }] }]
  }
}
EOF

# 2. Run a small Codex session
codex   # ask it to "list files and create a hello.txt"

# 3. Inspect captured payloads
for f in /tmp/codex-*.log; do echo "=== $f ==="; cat "$f"; echo; done

# 4. Locate rollout
ls -la ~/.codex/sessions/*/*/*/rollout-*.jsonl
cp ~/.codex/sessions/<latest>/rollout-*.jsonl src-tauri/tests/fixtures/codex/rollout.jsonl

# 5. Inspect rollout entry types
jq -c '. | {type, role, content_type: (.content[0].type // null)}' < src-tauri/tests/fixtures/codex/rollout.jsonl | sort -u
```

Document findings in `docs/agents/codex-schema.md`. Critical questions:

- Does PermissionRequest payload include a free-form question, or just allow/deny?
- Does the rollout carry `usage` per assistant message? Field names? (OpenAI typically `prompt_tokens` / `completion_tokens`, NOT `input_tokens`)
- Does Codex have a way to extract the rollout UUID at spawn time? (Filename: `rollout-<uuid>.jsonl`)
- Trust gate: where does Codex persist trust state? `~/.codex/trust.json`? Per-project file?

After the spike, **uninstall the probe hooks.json** before commit 1.

## Commit sequence

| # | Title | Purpose |
|---|---|---|
| 1 | `feat(codex): adapter scaffold + setup_agent('codex')` | Tasks 2.1–3.6. Writes hooks.json. |
| 2 | `feat(codex): rollout parser + UUID resolution` | Tasks 6.1–6.4, 8.1–8.4. |
| 3 | `feat(codex): trait impl complete` | Tasks 4.1–5.7. |
| 4 | `feat(codex): trust-gate banner in UI` | Task 9.1–9.2. |
| 5 | `feat(settings): Codex settings panel` | Task 11.1–11.2. |
| 6 | `test(codex): parser + setup + e2e PermissionRequest flow` | Task 12.1–12.4. |
| 7 | `docs(codex): user-facing docs + agent-agnostic milestone announcement` | Task 13.1–13.3. |

## Commit 1 — Adapter + setup

### `src-tauri/src/agents/codex/mod.rs`

```rust
pub mod setup;
pub mod transcript;
pub mod rollout_resolver;

pub struct CodexAdapter {
    binary_path: parking_lot::RwLock<Option<PathBuf>>,
    config_dir: PathBuf,    // ~/.codex
    sessions_dir: PathBuf,  // ~/.codex/sessions or $CODEX_HOME/sessions
}

impl CodexAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let config_dir = std::env::var("CODEX_HOME").map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".codex"));
        Self {
            binary_path: parking_lot::RwLock::new(which::which("codex").ok()),
            sessions_dir: config_dir.join("sessions"),
            config_dir,
        }
    }
}
```

### `src-tauri/src/agents/codex/setup.rs`

```rust
pub async fn run_codex_setup() -> Result<()> {
    let home = dirs::home_dir().ok_or(anyhow!("no home dir"))?;
    let path = home.join(".codex/hooks.json");
    tokio::fs::create_dir_all(path.parent().unwrap()).await?;

    // Read existing for merge
    let mut existing: serde_json::Value = match tokio::fs::read_to_string(&path).await {
        Ok(s) => serde_json::from_str(&s).unwrap_or(serde_json::json!({"hooks": {}})),
        Err(_) => serde_json::json!({"hooks": {}}),
    };

    // Cleanup obsolete cluihud entries (defensive — match by command pattern)
    if let Some(hooks) = existing.get_mut("hooks").and_then(|v| v.as_object_mut()) {
        for (_event, entries) in hooks.iter_mut() {
            if let Some(arr) = entries.as_array_mut() {
                arr.retain(|entry| !is_obsolete_cluihud_entry(entry));
            }
        }
    }

    // Add fresh cluihud entries
    let cluihud_entries = build_cluihud_entries();
    merge_into_hooks(&mut existing, cluihud_entries);

    // Atomic write
    let temp = path.with_extension("json.tmp");
    tokio::fs::write(&temp, serde_json::to_string_pretty(&existing)?).await?;
    tokio::fs::rename(&temp, &path).await?;
    Ok(())
}

fn build_cluihud_entries() -> serde_json::Value {
    serde_json::json!({
        "SessionStart":      [{ "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook send session-start --agent codex", "async": true }] }],
        "SessionEnd":        [{ "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook send session-end --agent codex",   "async": true }] }],
        "PreToolUse":        [{ "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook send pre-tool --agent codex",      "async": true }] }],
        "PostToolUse":       [{ "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook send tool-done --agent codex",     "async": true }] }],
        "PermissionRequest": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook ask-user --agent codex",            "sync": true, "timeout": 86400 }] }],
        "UserPromptSubmit":  [{ "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook inject-edits --agent codex",        "sync": true }] }],
        "Stop":              [{ "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook send stop --agent codex",          "async": true }] }],
    })
}

fn is_obsolete_cluihud_entry(entry: &serde_json::Value) -> bool {
    entry.pointer("/hooks/0/command")
        .and_then(|v| v.as_str())
        .map(|s| s.contains("cluihud hook"))
        .unwrap_or(false)
}
```

The merge logic preserves user-defined non-cluihud hooks: only entries matching `cluihud hook` are replaced.

## Commit 2 — Rollout parser + UUID resolution

### `src-tauri/src/agents/codex/rollout_resolver.rs`

```rust
/// After Codex spawn, find the rollout file with mtime closest to (and >=) spawn time.
pub async fn find_rollout_after_spawn(
    sessions_dir: &Path,
    spawn_time: std::time::SystemTime,
    timeout: std::time::Duration,
) -> Result<PathBuf> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if let Some(found) = scan_for_newest_rollout(sessions_dir, spawn_time).await {
            return Ok(found);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(anyhow!("no rollout file appeared in {:?} after spawn", sessions_dir))
}

async fn scan_for_newest_rollout(sessions_dir: &Path, after: std::time::SystemTime) -> Option<PathBuf> {
    // Codex layout: ~/.codex/sessions/YYYY/MM/DD/rollout-<uuid>.jsonl
    let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
    walk_dir(sessions_dir, &mut |path, mtime| {
        if path.file_name().and_then(|n| n.to_str())
            .map(|n| n.starts_with("rollout-") && n.ends_with(".jsonl")).unwrap_or(false)
            && mtime >= after
        {
            if newest.as_ref().map(|(_, t)| mtime > *t).unwrap_or(true) {
                newest = Some((path.to_path_buf(), mtime));
            }
        }
    }).await;
    newest.map(|(p, _)| p)
}

pub fn extract_uuid_from_filename(path: &Path) -> Option<String> {
    path.file_stem().and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix("rollout-").map(String::from))
}
```

### `src-tauri/src/agents/codex/transcript.rs`

```rust
/// Parse a Codex rollout line. Schema confirmed in spike (`docs/agents/codex-schema.md`).
pub fn parse_transcript_line(line: &str) -> Option<TranscriptEvent> {
    let entry: serde_json::Value = serde_json::from_str(line).ok()?;
    let typ = entry.get("type")?.as_str()?;

    match typ {
        "message" => {
            let role = entry.get("role")?.as_str()?.to_string();
            // OpenAI-style usage: prompt_tokens, completion_tokens, total_tokens
            // No cache_read/cache_write in standard OpenAI; default to 0
            if let Some(u) = entry.get("usage") {
                let raw = RawCost {
                    model_id: entry.get("model").and_then(|v| v.as_str().map(String::from)),
                    input_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    output_tokens: u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    cache_read_tokens: u.get("cache_read_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    cache_write_tokens: u.get("cache_write_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                };
                return Some(TranscriptEvent::Cost(raw));
            }
            let content = entry.get("content").cloned().unwrap_or_default().to_string();
            Some(TranscriptEvent::Message { role, content, model: entry.get("model").and_then(|v| v.as_str().map(String::from)) })
        }
        "tool_call" => {
            let name = entry.get("name")?.as_str()?.to_string();
            let input = entry.get("arguments").cloned().unwrap_or_default();
            Some(TranscriptEvent::ToolUse { name, input })
        }
        "tool_result" => {
            let id = entry.get("tool_call_id")?.as_str()?.to_string();
            let output = entry.get("output").cloned().unwrap_or_default();
            Some(TranscriptEvent::ToolResult { tool_use_id: id, output })
        }
        _ => None,  // unknown types: skip silently
    }
}
```

Adjust field names per your spike findings — OpenAI's exact naming may differ from this template.

## Commit 3 — Trait impl

```rust
#[async_trait::async_trait]
impl AgentAdapter for CodexAdapter {
    fn id(&self) -> AgentId { AgentId::codex() }
    fn display_name(&self) -> &str { "Codex" }
    fn capabilities(&self) -> &AgentCapabilities {
        // No PLAN_REVIEW, no ANNOTATIONS_INJECT until spike confirms otherwise
        static CAPS: AgentCapabilities = AgentCapabilities {
            flags: AgentCapability::ASK_USER_BLOCKING
                .union(AgentCapability::TOOL_CALL_EVENTS)
                .union(AgentCapability::STRUCTURED_TRANSCRIPT)
                .union(AgentCapability::SESSION_RESUME)
                .union(AgentCapability::RAW_COST_PER_MESSAGE),  // declared post-spike
            supported_models: vec![],
        };
        &CAPS
    }
    fn transport(&self) -> Transport {
        Transport::FileHooks {
            settings_path: self.config_dir.join("hooks.json"),
            hook_event_names: vec![
                "SessionStart", "SessionEnd",
                "PreToolUse", "PostToolUse", "PermissionRequest",
                "UserPromptSubmit", "Stop",
            ],
        }
    }
    fn requires_cluihud_setup(&self) -> bool { true }

    async fn detect(&self) -> DetectionResult {
        let bp = self.binary_path.read().clone();
        let trusted = check_codex_trust(&self.config_dir).await;
        DetectionResult {
            installed: self.config_dir.exists() || bp.is_some(),
            binary_path: bp,
            config_path: if self.config_dir.exists() { Some(self.config_dir.clone()) } else { None },
            version: None,
            trusted_for_project: Some(trusted),
        }
    }

    fn spawn(&self, ctx: &SpawnContext) -> Result<SpawnSpec, AdapterError> {
        let binary = self.binary_path.read().clone()
            .ok_or(AdapterError::Transport(anyhow!("codex binary not found")))?;
        let mut args = vec![];
        if let Some(uuid) = ctx.resume_from {
            args.push("resume".into());
            args.push(uuid.into());
        }
        let mut env = HashMap::new();
        env.insert("CLUIHUD_SESSION_ID".into(), ctx.session_id.into());
        Ok(SpawnSpec { binary, args, env })
    }

    fn parse_transcript_line(&self, line: &str) -> Option<TranscriptEvent> {
        transcript::parse_transcript_line(line)
    }

    async fn start_event_pump(&self, session_id: &str, _sink: EventSink) -> Result<(), AdapterError> {
        // Hook events arrive via shared Unix socket — no adapter-side pump needed.
        // BUT: we need to find the rollout file post-spawn to capture the UUID for resume.
        let spawn_time = std::time::SystemTime::now();
        let sessions_dir = self.sessions_dir.clone();
        let session_id_owned = session_id.to_string();
        tokio::spawn(async move {
            if let Ok(rollout_path) = rollout_resolver::find_rollout_after_spawn(
                &sessions_dir, spawn_time, std::time::Duration::from_secs(2),
            ).await {
                if let Some(uuid) = rollout_resolver::extract_uuid_from_filename(&rollout_path) {
                    let _ = persist_agent_internal_session_id(&session_id_owned, &uuid).await;
                }
            }
        });
        Ok(())
    }

    async fn submit_ask_answer(&self, session_id: &str, answers: serde_json::Value) -> Result<(), AdapterError> {
        // Same FIFO mechanism as CC — the Codex hook subprocess `cluihud hook ask-user` is blocking on the FIFO
        let pid = current_session_hook_pid(session_id).await
            .ok_or(AdapterError::Transport(anyhow!("no hook pid for session")))?;
        let fifo_path = format!("/tmp/cluihud-ask-{}.fifo", pid);
        let body = serde_json::json!({ "answers": answers });
        tokio::fs::write(&fifo_path, body.to_string()).await.map_err(AdapterError::Io)?;
        Ok(())
    }

    // submit_plan_decision uses default impl returning NotSupported(PLAN_REVIEW)
}

async fn check_codex_trust(_config_dir: &Path) -> bool {
    // Spike-confirmed heuristic: <project>/.codex/trusted file or ~/.codex/trust.json entry
    // Implementation depends on findings; default to false if uncertain
    false  // placeholder; refine per spike
}
```

## Commit 4 — Trust banner

In `src/components/sidebar/SessionRow.tsx` (or equivalent), when the active session is Codex and metadata indicates not trusted:

```typescript
{agentId === "codex" && !trustedForProject && (
  <Banner variant="warning">
    Codex requires trust for this project.
    <code>codex trust</code> from a terminal in this project, then{" "}
    <button onClick={rescanAgents}>Rescan</button>.
  </Banner>
)}
```

## Verification

```bash
# Backend
cargo test agents::codex:: 2>&1 | tail -10

# Setup test: write hooks.json, verify shape
cargo test agents::codex::setup:: 2>&1

# Manual e2e
codex --version
# In cluihud: create session with Codex, trigger a permission via destructive op
# Verify: AskUserModal renders, accept → Codex proceeds
# Verify: PlanPanel hidden (no PLAN_REVIEW capability)
# Verify: status bar shows tokens (no USD for Codex; pricing module future)
# Verify: kill cluihud, restart, resume the session — `codex resume <uuid>` invoked
```

## Common pitfalls

- **Codex schema versioning**: Codex `v1 → v2` may change hook payload shapes. Detect Codex version (`codex --version`) and gate parser logic if needed. Document the supported version range.
- **Trust gate auto-detection**: spike must confirm where Codex persists trust state. If you can't determine it programmatically, fall back to "always show banner with rescan option" — UX cost is low.
- **OpenAI cost field naming**: rollout schema uses OpenAI conventions (`prompt_tokens`, `completion_tokens`) — different from Anthropic's (`input_tokens`, `output_tokens`). Map carefully.
- **`hooks.json` merge corruption**: write to temp + rename atomically; if you write in-place and crash mid-write, the user's hooks.json is corrupt. Use `tokio::fs::write(temp).await; rename(temp, target).await`.
- **PermissionRequest payload variation**: Codex may emit different shapes for "ask user a question" vs "approve a destructive command". The AskUserModal handles both via `prompt + options`. Test both paths in spike.
- **`codex resume <uuid>`**: confirm the exact command shape during spike. Could be `codex resume`, `codex --resume <uuid>`, or `codex --session <uuid>` depending on version.
