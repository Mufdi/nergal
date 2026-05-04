# Implementation guide — agent-adapter-foundation

> Companion to `tasks.md`. Where tasks.md tells you **what** to do (checkboxes), this file tells you **how** — sequencing, code sketches, verification, and debug aids. Read this front-to-back before starting; reference it commit-by-commit during.

## Pre-flight checklist

Before opening the first commit:

```bash
# Confirm you're on a clean feature branch off main
cd cluihud
git checkout main && git pull
git checkout -b agent-adapter-foundation

# Snapshot the current state — used as zero-regression baseline
cargo build --release 2>&1 | tail -5     # baseline build clean
cd src-tauri && cargo test 2>&1 | tail -5  # baseline tests pass
cd .. && pnpm install && pnpm typecheck     # frontend baseline

# Capture current sites of CC coupling for grep-verification later
grep -rn "use crate::claude::" src-tauri/src/ > /tmp/cc-imports-before.txt
grep -rn "crate::tasks::transcript_parser" src-tauri/src/ >> /tmp/cc-imports-before.txt
wc -l /tmp/cc-imports-before.txt    # expect ~10-15 hits

# Manual UX baseline: take screenshots of the running app for post-refactor comparison
pnpm dev &  # take screenshots of: sidebar, plan panel, task panel, status bar (USD), ask-user modal, file-changed panel
# Save to /tmp/baseline-screenshots/
```

If any of the baseline checks fail, **stop**. Don't refactor on top of broken state.

## Commit sequence (single PR, sub-commits for review)

The whole change ships as one PR. Sub-commits exist for review legibility, not for partial deploys — the build must be green at every commit boundary.

| # | Commit title | Files touched | Purpose |
|---|---|---|---|
| 1 | `feat(agents): trait + types + cost aggregator scaffold` | `src-tauri/src/agents/{mod.rs, registry.rs, cost_aggregator.rs}`, `Cargo.toml` | Tasks 1.1–1.13. Pure addition, no consumers yet. |
| 2 | `refactor(agents): atomic move claude/* and tasks/transcript_parser.rs into agents/claude_code/` | `git mv` operations + every `use crate::claude::` → `use crate::agents::claude_code::` | Tasks 2.1–2.6. Build must remain green. |
| 3 | `feat(agents): ClaudeCodeAdapter implementing trait` | `src-tauri/src/agents/claude_code/mod.rs` (the wrapper) | Tasks 2.7–2.15. Adapter wraps moved logic. |
| 4 | `feat(hooks): wire dispatcher through agent registry with cache` | `src-tauri/src/hooks/server.rs`, `cli.rs` | Tasks 3.1–3.6. Cache + drop-on-miss + `--agent` flag. |
| 5 | `feat(pty): spawn through SpawnSpec from active adapter` | `src-tauri/src/pty.rs` | Tasks 4.1–4.3. |
| 6 | `feat(db): migration adding agent_id and agent_internal_session_id` | `src-tauri/src/db.rs`, migrations | Tasks 5.1–5.5. |
| 7 | `feat(config): agent_overrides + default_agent + canonicalization` | `src-tauri/src/config.rs` | Tasks 6.1–6.6. |
| 8 | `feat(socket): control message kind discriminator + rescan-agents CLI` | `src-tauri/src/hooks/server.rs`, `main.rs` | Tasks 1.11, 1.12. |
| 9 | `feat(frontend): agent store + capability gating sync` | `src/stores/agent.ts`, `hooks.ts`, `tasks.ts`, `plan.ts`, `askUser.ts` | Tasks 7.1–7.6. |
| 10 | `feat(ui): capability-gated components + agent badge + settings panel` | `src/components/{plan,tasks,session,layout,sidebar,settings}/*.tsx` | Tasks 8.1–8.6, 10.1–10.3. |
| 11 | `feat(session): session creation modal with agent picker` | `src/components/session/CreateSessionModal.tsx` (or equivalent) | Tasks 9.1–9.4. |
| 12 | `test(agents): integration tests for zero-regression CC flows` | `src-tauri/tests/`, fixtures | Tasks 11.1–11.15. |
| 13 | `docs(agents): architecture + module layout updates` | `CLAUDE.md`, `docs/agents/architecture.md` | Tasks 12.1–12.3. |

After commit 13, run the **full verification block** (see end of this file) before merging.

## Commit 1 — Trait + types + scaffold

### `src-tauri/Cargo.toml` deps

```toml
[dependencies]
# ... existing ...
bitflags = "2"
regex = "1"
dashmap = "6"
parking_lot = "0.12"
dunce = "1"
thiserror = "1"
# verify already present: serde, tokio (with rt-multi-thread + macros + sync), async-trait, anyhow
```

### `src-tauri/src/agents/mod.rs` skeleton

Use the trait sketch in `design.md` verbatim as a starting point. Critical points:

- `bitflags!` macro for `AgentCapability` uses v2 syntax (different from v1 — be careful when copying examples from the web).
- `impl Serialize for AgentCapability` emits `Vec<&'static str>` — write it manually, not via `#[derive(Serialize)]`:

```rust
impl serde::Serialize for AgentCapability {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut names: Vec<&'static str> = Vec::new();
        if self.contains(Self::PLAN_REVIEW)           { names.push("PLAN_REVIEW"); }
        if self.contains(Self::ASK_USER_BLOCKING)     { names.push("ASK_USER_BLOCKING"); }
        if self.contains(Self::TOOL_CALL_EVENTS)      { names.push("TOOL_CALL_EVENTS"); }
        if self.contains(Self::STRUCTURED_TRANSCRIPT) { names.push("STRUCTURED_TRANSCRIPT"); }
        if self.contains(Self::RAW_COST_PER_MESSAGE)  { names.push("RAW_COST_PER_MESSAGE"); }
        if self.contains(Self::TASK_LIST)             { names.push("TASK_LIST"); }
        if self.contains(Self::SESSION_RESUME)        { names.push("SESSION_RESUME"); }
        if self.contains(Self::ANNOTATIONS_INJECT)    { names.push("ANNOTATIONS_INJECT"); }
        names.serialize(ser)
    }
}
```

The `Deserialize` mirror parses the same shape back.

- `AgentId::new` regex MUST be `^[a-z][a-z0-9-]{0,31}$` (note `{0,31}`, not `{1,31}` — see Decision 3 spec).
- The known constructors (`AgentId::claude_code()` etc.) bypass `new()` for compile-time correctness; the test in 1.13 protects against a future rename violating the regex.

### `src-tauri/src/agents/registry.rs` skeleton

```rust
pub struct AgentRegistry {
    adapters: parking_lot::RwLock<HashMap<AgentId, Arc<dyn AgentAdapter>>>,
}

impl AgentRegistry {
    pub fn new() -> Self { Self { adapters: Default::default() } }

    pub fn register(&self, adapter: Arc<dyn AgentAdapter>) -> Result<(), AdapterError> {
        let id = adapter.id();
        let mut w = self.adapters.write();
        if w.contains_key(&id) { return Err(AdapterError::DuplicateAgentId(id)); }
        w.insert(id, adapter);
        Ok(())
    }

    pub fn get(&self, id: &AgentId) -> Option<Arc<dyn AgentAdapter>> {
        self.adapters.read().get(id).cloned()
    }

    pub async fn scan(&self) -> Vec<(AgentId, DetectionResult)> {
        // detect() is async on each adapter; sequential is fine (≤4 adapters)
        let adapters: Vec<_> = self.adapters.read().values().cloned().collect();
        let mut out = Vec::with_capacity(adapters.len());
        for a in adapters {
            let id = a.id();
            let det = a.detect().await;
            out.push((id, det));
        }
        out
    }

    pub fn priority_list() -> Vec<AgentId> {
        vec![AgentId::claude_code(), AgentId::codex(), AgentId::opencode(), AgentId::pi()]
    }
}

pub fn default_registrations(reg: &AgentRegistry) -> Result<(), AdapterError> {
    reg.register(Arc::new(crate::agents::claude_code::ClaudeCodeAdapter::new()))?;
    // OpenCode/Pi/Codex registrations land in their respective adapter changes
    Ok(())
}
```

### `src-tauri/src/agents/cost_aggregator.rs`

Implement exactly as design.md specifies. The `parking_lot::Mutex` over `SessionCostTotals` is intentional — `tokio::sync::Mutex` would force `await` on every `add()` call, polluting hot path. Locks are sub-microsecond uncontended.

### Verification after commit 1

```bash
cd src-tauri && cargo build && cargo test agents:: 2>&1 | tail -10
# Expect: builds clean, agent tests pass.
# Expect: no consumers yet — no warnings about unused items because mod.rs declares pub items.
```

## Commit 2 — Atomic move

This is the **risky** commit. The whole tree must build at the END of this commit, but every importer needs updating in the same atomic step.

### Sequence

```bash
# 1. Move files
git mv src-tauri/src/claude/transcript.rs src-tauri/src/agents/claude_code/transcript.rs
git mv src-tauri/src/claude/cost.rs       src-tauri/src/agents/claude_code/cost.rs
git mv src-tauri/src/claude/plan.rs       src-tauri/src/agents/claude_code/plan.rs
git mv src-tauri/src/claude/openspec.rs   src-tauri/src/openspec.rs
git mv src-tauri/src/tasks/transcript_parser.rs src-tauri/src/agents/claude_code/tasks.rs

# 2. Empty claude/ dir is now claude/mod.rs only — delete it (the mod will be removed in lib.rs)
rm src-tauri/src/claude/mod.rs
rmdir src-tauri/src/claude
# Same for tasks/ if transcript_parser.rs was its only file — verify first
ls src-tauri/src/tasks/   # if empty besides mod.rs, remove the dir; if other files exist, just delete the file
```

### Update every importer

```bash
cd src-tauri/src
# Mass-replace import paths
grep -rln "use crate::claude::"          . | xargs sed -i 's|use crate::claude::|use crate::agents::claude_code::|g'
grep -rln "crate::claude::"              . | xargs sed -i 's|crate::claude::|crate::agents::claude_code::|g'
grep -rln "use crate::tasks::transcript_parser" . | xargs sed -i 's|use crate::tasks::transcript_parser|use crate::agents::claude_code::tasks|g'
grep -rln "crate::tasks::transcript_parser" . | xargs sed -i 's|crate::tasks::transcript_parser|crate::agents::claude_code::tasks|g'

# Edge case: openspec moved to top-level, NOT into agents/
grep -rln "agents::claude_code::openspec" . | xargs sed -i 's|crate::agents::claude_code::openspec|crate::openspec|g'
```

### Update `lib.rs` mod block

```rust
// Remove
- mod claude;
// (tasks mod may stay if tasks/ has other files)

// Add
+ mod agents;
+ mod openspec;
```

### Inside `agents/claude_code/cost.rs` — apply the cost API change

The pre-foundation file has:

```rust
pub fn parse_cost_from_transcript(transcript_path: &Path) -> Result<CostSummary> {
    // reads whole file, accumulates
    let mut summary = CostSummary::default();
    for line in BufReader::new(File::open(transcript_path)?).lines() {
        // ...
        summary.input_tokens += usage.input_tokens;
        // ... and computes USD using hardcoded constants
    }
    Ok(summary)
}
```

Replace with:

```rust
pub fn parse_cost_line(line: &str) -> Option<RawCost> {
    let entry: serde_json::Value = serde_json::from_str(line).ok()?;
    let usage = entry.get("message")?.get("usage")?;
    Some(RawCost {
        model_id: entry.pointer("/message/model").and_then(|v| v.as_str().map(str::to_string)),
        input_tokens:       usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        output_tokens:      usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        cache_read_tokens:  usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        cache_write_tokens: usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
    })
}

// Private helper preserving the previous Sonnet 4 USD math (no UX regression for CC)
pub(crate) fn legacy_usd_for_sonnet4(t: &SessionCostTotals) -> f64 {
    const INPUT:       f64 = 3.0  / 1_000_000.0;
    const OUTPUT:      f64 = 15.0 / 1_000_000.0;
    const CACHE_READ:  f64 = 0.30 / 1_000_000.0;
    const CACHE_WRITE: f64 = 3.75 / 1_000_000.0;
    (t.input_tokens as f64) * INPUT
      + (t.output_tokens as f64) * OUTPUT
      + (t.cache_read_tokens as f64) * CACHE_READ
      + (t.cache_write_tokens as f64) * CACHE_WRITE
}
```

The old `parse_cost_from_transcript` is gone; its callers (the Stop hook handler) now feed lines into the aggregator instead.

### Inside `agents/claude_code/plan.rs` — respect `plansDirectory`

Add a small helper that reads `~/.claude/settings.json` and extracts `plansDirectory`:

```rust
fn user_configured_plans_dir() -> Option<PathBuf> {
    let path = dirs::home_dir()?.join(".claude/settings.json");
    let content = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("plansDirectory")?.as_str().map(PathBuf::from).map(|p| {
        // Resolve ~ expansion if user configured "~/foo" — but JSON typically stores absolute or "."-relative
        if p.starts_with("~/") {
            dirs::home_dir().map(|h| h.join(p.strip_prefix("~/").unwrap())).unwrap_or(p)
        } else { p }
    })
}

pub fn global_plans_dir() -> PathBuf {
    user_configured_plans_dir().unwrap_or_else(|| {
        dirs::home_dir().unwrap_or_default().join(".claude/plans")
    })
}
```

Use `global_plans_dir()` in the existing fallback location.

### Verification after commit 2

```bash
cd src-tauri && cargo build 2>&1 | tail -20
# Expect: clean build. If you see "unresolved import crate::claude::...", you missed an importer; grep for it and fix.

grep -rn "crate::claude" src-tauri/src/    # MUST be empty (zero hits)
grep -rn "tasks::transcript_parser" src-tauri/src/   # MUST be empty
ls src-tauri/src/claude 2>&1                # MUST say "No such file or directory"
```

## Commit 3 — ClaudeCodeAdapter

`src-tauri/src/agents/claude_code/mod.rs` declares the struct that implements the trait, mostly delegating to the moved sibling modules.

```rust
pub mod transcript;
pub mod cost;
pub mod plan;
pub mod tasks;

pub struct ClaudeCodeAdapter {
    capabilities: AgentCapabilities,
}

impl ClaudeCodeAdapter {
    pub fn new() -> Self {
        Self {
            capabilities: AgentCapabilities {
                flags: AgentCapability::all(),  // CC supports everything
                supported_models: vec![],
            },
        }
    }
}

#[async_trait::async_trait]
impl AgentAdapter for ClaudeCodeAdapter {
    fn id(&self) -> AgentId { AgentId::claude_code() }
    fn display_name(&self) -> &str { "Claude Code" }
    fn capabilities(&self) -> &AgentCapabilities { &self.capabilities }

    fn transport(&self) -> Transport {
        Transport::FileHooks {
            settings_path: dirs::home_dir().unwrap_or_default().join(".claude/settings.json"),
            hook_event_names: vec![
                "SessionStart", "SessionEnd",
                "PermissionRequest", "PreToolUse", "PostToolUse",
                "TaskCreated", "TaskCompleted",
                "CwdChanged", "FileChanged", "PermissionDenied",
                "Stop", "UserPromptSubmit",
            ],
        }
    }

    fn requires_cluihud_setup(&self) -> bool { true }

    async fn detect(&self) -> DetectionResult {
        let home = dirs::home_dir().unwrap_or_default();
        let config_dir = home.join(".claude");
        let binary_path = which::which("claude").ok();
        DetectionResult {
            installed: config_dir.exists() || binary_path.is_some(),
            binary_path,
            config_path: if config_dir.exists() { Some(config_dir) } else { None },
            version: None,  // populated by refresh_version()
            trusted_for_project: None,  // CC has no trust gate
        }
    }

    async fn refresh_version(&self) -> Option<String> {
        let out = tokio::process::Command::new("claude")
            .arg("--version")
            .output().await.ok()?;
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn spawn(&self, ctx: &SpawnContext) -> Result<SpawnSpec, AdapterError> {
        let binary = which::which("claude").map_err(|e| AdapterError::Transport(e.into()))?;
        let mut args = vec![];
        if let Some(rid) = ctx.resume_from {
            // CC accepts both --continue (latest) and --resume <id>; current pty.rs uses --continue
            args.push("--continue".into());  // honors current behavior; refine if resume_from is a specific id
        }
        let mut env = HashMap::new();
        env.insert("CLUIHUD_SESSION_ID".into(), ctx.session_id.into());
        Ok(SpawnSpec { binary, args, env })
    }

    fn parse_transcript_line(&self, line: &str) -> Option<TranscriptEvent> {
        // Emit Cost variant if line carries usage; emit ToolUse / ToolResult / Message as found
        if let Some(raw) = cost::parse_cost_line(line) {
            return Some(TranscriptEvent::Cost(raw));
        }
        // Add other variants as needed (ToolUse for tasks parser, Message for transcript display)
        None
    }

    async fn start_event_pump(&self, session_id: &str, sink: EventSink) -> Result<(), AdapterError> {
        // Start transcript watcher (existing logic in transcript.rs)
        transcript::start_watcher(session_id, sink.clone()).await?;
        // Start plan watcher (existing logic in plan.rs)
        plan::start_watcher(session_id, sink).await?;
        Ok(())
    }

    async fn stop_event_pump(&self, session_id: &str) -> Result<(), AdapterError> {
        transcript::stop_watcher(session_id).await;
        plan::stop_watcher(session_id).await;
        Ok(())
    }

    async fn submit_plan_decision(&self, session_id: &str, decision: PlanDecision) -> Result<(), AdapterError> {
        // Move existing logic from commands.rs:154-193 to here
        plan::write_decision_to_fifo(session_id, &decision).await
    }

    async fn submit_ask_answer(&self, session_id: &str, answers: serde_json::Value) -> Result<(), AdapterError> {
        // Move existing logic from commands.rs:140-150 to here
        let fifo_path = format!("/tmp/cluihud-ask-{}.fifo", std::process::id());
        let body = serde_json::json!({ "answers": answers });
        tokio::fs::write(&fifo_path, body.to_string()).await.map_err(AdapterError::Io)?;
        Ok(())
    }
}
```

### Verification after commit 3

```bash
cd src-tauri && cargo test agents::claude_code:: 2>&1 | tail -10
# Run the existing transcript/cost/plan tests (they should still pass post-move).
```

## Commit 4 — Hook dispatcher with cache

### `src-tauri/src/hooks/server.rs` — main changes

Add field to app state:

```rust
pub struct HookServerState {
    // ... existing fields ...
    pub agent_id_cache: Arc<DashMap<String, AgentId>>,  // session_id -> agent_id
    pub registry: Arc<AgentRegistry>,
}
```

In the dispatcher (around line 206 currently):

```rust
fn dispatch_event(state: &HookServerState, payload: &serde_json::Value) {
    let session_id = match payload.get("cluihud_session_id").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return,  // existing drop path: no session id
    };

    // Resolve agent_id
    let agent_id = state.agent_id_cache.get(session_id).map(|r| r.clone())
        .or_else(|| state.db.get_session(session_id).ok().flatten().map(|s| s.agent_id));

    let agent_id = match agent_id {
        Some(id) => id,
        None => {
            tracing::warn!(session_id, "orphan hook event; dropping");
            return;
        }
    };

    // Dispatch via adapter
    let adapter = match state.registry.get(&agent_id) {
        Some(a) => a,
        None => {
            tracing::error!(?agent_id, "registry has no adapter for resolved agent_id; dropping");
            return;
        }
    };

    // Existing dispatch logic, now per-adapter — adapter-specific parsing
    handle_event_for_adapter(state, &adapter, session_id, payload);
}
```

The `handle_event_for_adapter` function takes adapter-specific actions: parsing tool_input, mapping to FrontendHookEvent variants, etc. Most of this is moved from the existing `process_hook_event` body.

### Cache population timing

In `commands.rs::create_session_with_agent` (which you'll add or extend):

```rust
async fn create_session_with_agent(
    state: tauri::State<'_, AppState>,
    project_path: String,
    agent_id: AgentId,
    // ... other args
) -> Result<String, String> {
    // 1. Insert DB row
    let session_id = state.db.insert_session(&project_path, &agent_id).await?;

    // 2. Populate cache BEFORE PTY spawn (Decision 9 — closes the race)
    state.agent_id_cache.insert(session_id.clone(), agent_id.clone());

    // 3. Get adapter and spawn
    let adapter = state.registry.get(&agent_id).ok_or("adapter not found")?;
    let spawn_spec = adapter.spawn(&SpawnContext { /* ... */ })
        .map_err(|e| e.to_string())?;
    pty::spawn_with_spec(&session_id, spawn_spec).await?;

    // 4. Start event pump (adapter-owned watchers, etc.)
    let sink = state.event_sink.clone();
    adapter.start_event_pump(&session_id, sink).await
        .map_err(|e| e.to_string())?;

    Ok(session_id)
}
```

In `destroy_session`:

```rust
adapter.stop_event_pump(&session_id).await.ok();
state.agent_id_cache.remove(&session_id);
// ... existing teardown
```

### Verification after commit 4

```bash
cd src-tauri && cargo test hooks:: 2>&1 | tail -10
# Then manual: open a CC session, fire a tool event from inside Claude, observe TaskPanel update.
# Run tests 11.11 (orphan event) and 11.12 (cache miss recovery) early to validate the dispatcher.
```

## Commits 5–11 — see tasks.md sections 4–10

These follow standard refactor patterns. Key reminders:

- **Commit 5 (PTY)**: `pty.rs::spawn_session` becomes `pty::spawn_with_spec(session_id, spec: SpawnSpec)`. The hardcoded ` claude\n` lines (`pty.rs:253-257`) are deleted; the spec drives everything.
- **Commit 6 (DB)**: Migration runs at app startup. Test 11.7 must run against a populated pre-foundation fixture DB.
- **Commit 7 (Config)**: `dunce::canonicalize` returns `Result`; for non-existent paths fall back to `path.to_string_lossy().into_owned()`. Document this quirk.
- **Commit 9 (Frontend store)**: The synchronous capability population is the central piece — ensure `Session` struct from backend already includes `agent_capabilities: string[]` so the store doesn't need a separate invoke.
- **Commit 11 (Session creation modal)**: The picker default applies the priority logic on the backend (`commands::resolve_default_agent`) and the frontend just renders. Don't replicate the priority logic in TS.

## Commit 12 — Tests

The integration test fixtures live in `src-tauri/tests/fixtures/`:

- `cc_transcript.jsonl` — copy a real CC transcript with each interesting line type (message, tool_use, tool_result, with/without usage, with/without cache fields)
- `cc_settings_with_plans_dir.json` — fixture for plan watcher test 11.7
- `cc_db_pre_foundation.sqlite` — minimal SQLite with the old schema (no agent_id column) and 3 fixture rows; used by migration test 11.7

Test names map to task numbers — `tests/agents/regression_test.rs` with one `#[tokio::test]` per scenario.

For test 11.13 (manual UX walk), record:

- Plan flow: trigger ExitPlanMode in a real session, accept the plan, verify Claude proceeds.
- Ask-user flow: trigger AskUserQuestion, submit answer.
- Task list: TodoWrite tool, see tasks update.
- Cost USD: run a few prompts, verify status bar shows the same USD figure as pre-foundation (within rounding).
- Annotations: open a plan, add an annotation, save, verify it injects via UserPromptSubmit.
- Resume: kill cluihud mid-session, restart, hit resume on the session, verify it continues.

Take screenshots at each step; diff against `/tmp/baseline-screenshots/`.

## Verification block — pre-merge gate

Run end-to-end before requesting review:

```bash
# Backend — must all pass
cd cluihud/src-tauri
cargo fmt --check
cargo clippy -- -D warnings
cargo test 2>&1 | tail -15

# Frontend — must all pass
cd cluihud
npx tsc --noEmit
pnpm lint 2>&1 | tail -10

# Build the binary, install it (so hooks resolve to the new version)
cargo install --path src-tauri --force 2>&1 | tail -5
which cluihud && cluihud --version

# Run the app, manually exercise zero-regression flows (test 11.13)
pnpm dev
```

If anything fails, **do not merge**. Investigate.

## Rollback strategy

If post-merge a critical regression appears in CC:

1. **Don't revert the merge commit** (the file moves are mass — revert is huge). Instead, pin via:
   ```bash
   git revert --no-commit <merge_sha>
   git checkout <merge_sha~1> -- src-tauri/src/agents src-tauri/src/openspec
   ```
   ...and prepare a hotfix forward.
2. The DB migration is **not reversible** without restoring backup. Document this in `CHANGELOG.md` and instruct affected users to restore their `~/.local/share/cluihud/cluihud.db` from backup if they need to downgrade.
3. The `~/.claude/settings.json` hooks change (no behavioral change here, but tasks generalize the call sites) is reversible — `cluihud setup` post-foundation produces the same JSON shape.

## Common pitfalls — read before each commit

- **`bitflags = "2"` syntax**: v2 uses `bitflags! { pub struct Foo: u32 { const A = 1 << 0; ... } }`. Don't paste v1 examples (different macro form).
- **`async_trait::async_trait`**: every default async method body must use `Pin<Box<dyn Future + Send>>` under the hood; the macro handles it but compile errors mention `Send` bounds — usually means a captured `&self` reference isn't `Send`.
- **`dirs::home_dir()`**: returns `Option<PathBuf>`. Don't `unwrap()` in adapter code; surface as `AdapterError`.
- **DashMap deadlocks**: `agent_id_cache.get(k)` holds a read guard. Don't call `agent_id_cache.insert` while holding a `get` guard from the same map. Drop the guard explicitly: `let id = state.agent_id_cache.get(sid).map(|r| r.clone());` (clones out, drops guard).
- **PathBuf as JSON map key**: serde supports it, but uses `Display` which on Windows produces backslash paths. Stick to `String` keys (Decision 6) — already addressed in the plan.
- **`tokio::process::Command::output()` blocks tokio runtime if not awaited**: never call `.output()` synchronously in async context.

## Estimated effort

- Commits 1–4 (backend foundation): **2-3 days** focused work. Most risk in commit 2 (atomic move).
- Commits 5–8 (PTY, DB, config, socket): **1-2 days**.
- Commits 9–11 (frontend): **1-2 days**.
- Commit 12 (tests): **2 days** (writing integration tests for 13+ subsystems is non-trivial).
- Commit 13 (docs): **half day**.

**Total: 6-9 days** for a careful single-developer pass with verification at each step. Personal-use scale, no rush.

## Done means

- All 13 task sections checked off in `tasks.md`.
- All tests green.
- Manual UX walk passes screenshot diff.
- `openspec validate --changes` returns no errors.
- PR description references `.review-history.md` (the iterative-plan-review log) for traceability.
