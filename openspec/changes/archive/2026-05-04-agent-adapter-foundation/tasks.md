## 1. Trait + types + registry scaffold

- [ ] 1.1 Pin deps in `src-tauri/Cargo.toml`: `bitflags = "2"`, `regex = "1"`, `dashmap = "6"`, `parking_lot = "0.12"`, `dunce = "1"` (cross-platform path canonicalization), `thiserror = "1"`. Verify `serde`, `tokio`, `async-trait` already present.
- [ ] 1.2 Create `src-tauri/src/agents/mod.rs` with `AgentId` newtype + validated constructor (`AgentId::new(s) -> Result<Self, AdapterError>` validating `^[a-z][a-z0-9-]{0,31}$`), `AgentCapability` bitflags (PLAN_REVIEW, ASK_USER_BLOCKING, TOOL_CALL_EVENTS, STRUCTURED_TRANSCRIPT, RAW_COST_PER_MESSAGE, TASK_LIST, SESSION_RESUME, ANNOTATIONS_INJECT)
- [ ] 1.3 Define `Transport` enum (FileHooks, HttpSse with `auth: Option<AuthScheme>` slot, JsonlTail, RpcStdio) with associated payload structs; define `AuthScheme` enum (None, Bearer, Header)
- [ ] 1.4 Define `DetectionResult` (with `version: Option<String>` populated lazily, `trusted_for_project: Option<bool>`), `SpawnContext`, `SpawnSpec`, `TranscriptEvent`, `RawCost`, `PlanDecision`, `EventSink` (mpsc::UnboundedSender<FrontendHookEvent>) types
- [ ] 1.5 Define `AdapterError` enum with variants: `NotSupported(AgentCapability)`, `SessionLocked`, `InvalidAgentId(String)`, `DuplicateAgentId(AgentId)`, `Transport(anyhow::Error)`, `Io(std::io::Error)` — derive `thiserror::Error`
- [ ] 1.6 Define `AgentAdapter` async trait with default impls for `submit_plan_decision`, `submit_ask_answer`, `stop_event_pump`, `refresh_version` (returning NotSupported / Ok / None respectively); declare `requires_cluihud_setup() -> bool` as a required method
- [ ] 1.7 Implement custom `Serialize` for the bitflags struct emitting `Vec<&'static str>` (e.g. `["PLAN_REVIEW", "TASK_LIST"]`) and `Deserialize` parsing the same shape
- [ ] 1.8 Create `agents/registry.rs` with `AgentRegistry` struct: `register(adapter) -> Result` (rejects DuplicateAgentId), `available_agents()`, `get(AgentId)`, `scan() -> Vec<(AgentId, DetectionResult)>`, `priority_list()` returning `[CC, Codex, OpenCode, Pi]`
- [ ] 1.9 Create `agents/cost_aggregator.rs` with `SessionCostAggregator` (parking_lot::Mutex over `SessionCostTotals`), `add(&RawCost)`, `snapshot()`
- [ ] 1.10 Wire `AgentRegistry` into `app_setup()` in `lib.rs`; expose Tauri commands `list_available_agents() -> Vec<AgentMetadata>`, `get_session_agent(session_id) -> AgentMetadata`, `rescan_agents() -> Vec<AgentMetadata>`
- [ ] 1.11 Extend Unix socket wire schema to include a `kind` discriminator: `{"kind": "hook_event", ...}` (default for backward compat) vs `{"kind": "control", "op": "rescan_agents"}`. Dispatcher routes by kind; hook events follow existing path, control commands go to a new control handler. Document in `hooks/server.rs` doc-comment.
- [ ] 1.12 Add CLI subcommand `cluihud rescan-agents` to top-level `main.rs` clap parser; writes a control message `{"kind": "control", "op": "rescan_agents"}` to the Unix socket; the running app's control handler invokes `AgentRegistry::scan()` and returns updated metadata
- [ ] 1.13 Add `cargo test` covering: AgentId validation (accepts valid + rejects invalid like `""`, `"../etc"`, `"FOO"`); known constructors (`AgentId::claude_code()`, `opencode()`, `pi()`, `codex()`) round-trip through `AgentId::new(known.as_str()).is_ok()` so a future rename can't silently violate the regex; bitflags Serialize round-trip; registry registration + duplicate rejection; scan with mock adapter; generic test asserting every registered adapter's `spawn(ctx)` returns SpawnSpec.env containing `CLUIHUD_SESSION_ID`

## 2. Move claude/* into agents/claude_code/ (atomic refactor — single PR, no transitional re-exports)

> **Critical sequencing**: tasks 2.1–2.5 must land in a single commit (or the build breaks). The `lib.rs` `mod` block must update simultaneously with the file moves. Run `cargo check` after EACH commit to catch import drift early.

- [ ] 2.1 `git mv src-tauri/src/claude/transcript.rs src-tauri/src/agents/claude_code/transcript.rs` — preserve public API, no logic changes yet
- [ ] 2.2 `git mv src-tauri/src/claude/cost.rs src-tauri/src/agents/claude_code/cost.rs` — **DELETE** the hardcoded Sonnet 4 constants `INPUT_PRICE`, `OUTPUT_PRICE`, `CACHE_READ_PRICE`, `CACHE_WRITE_PRICE`. Replace `parse_cost_from_transcript(path) -> CostSummary` with `parse_cost_line(line) -> Option<RawCost>`. Add private `legacy_usd_for_sonnet4(raw: &RawCost) -> f64` reproducing the previous USD math (so the status bar's USD doesn't disappear for CC users — see Decision 6).
- [ ] 2.3 `git mv src-tauri/src/claude/plan.rs src-tauri/src/agents/claude_code/plan.rs` — preserve current `<cwd>/.claude/plans/` then `~/.claude/plans/` fallback. **Add**: read `~/.claude/settings.json` `plansDirectory` field (if set) and use it as the global fallback path instead of `~/.claude/plans/` (respects user customization)
- [ ] 2.4 `git mv src-tauri/src/claude/openspec.rs src-tauri/src/openspec.rs` (top-level, NOT under agents/ — it's agent-agnostic per Decision 8)
- [ ] 2.5 `git mv src-tauri/src/tasks/transcript_parser.rs src-tauri/src/agents/claude_code/tasks.rs` — preserve current behavior recognizing CC tool names (TodoWrite, TaskCreate, TaskUpdate)
- [ ] 2.6 In **same commit as 2.1–2.5**, update every importer:
    - `grep -rn "use crate::claude::" src-tauri/src/` to enumerate all sites; expected hits include `db.rs`, `commands.rs`, `plan_state.rs`, `lib.rs`, `pty.rs`
    - Replace each with `use crate::agents::claude_code::...`
    - `grep -rn "use crate::tasks::transcript_parser" src-tauri/src/` and replace with `use crate::agents::claude_code::tasks::...`
    - `grep -rn "crate::claude::openspec" src-tauri/src/` and replace with `crate::openspec`
    - Update `lib.rs` `mod claude;` removal and `mod openspec;` addition
    - Run `cargo check`; fix any leftover paths
- [ ] 2.7 Create `src-tauri/src/agents/claude_code/mod.rs` with `ClaudeCodeAdapter` struct implementing `AgentAdapter`
- [ ] 2.8 Implement `ClaudeCodeAdapter::detect()` scanning `~/.claude/` directory and `which claude` (sync filesystem checks only, no child spawn); return `version: None` initially
- [ ] 2.9 Implement `ClaudeCodeAdapter::refresh_version()` calling `claude --version` async; populates the registry's metadata cache when done
- [ ] 2.10 Implement `ClaudeCodeAdapter::spawn()` returning `(claude binary, args from ctx.resume_from, env including CLUIHUD_SESSION_ID)` matching current `pty.rs:253-257` behavior
- [ ] 2.11 Implement `ClaudeCodeAdapter::parse_transcript_line()` wrapping the moved logic (per-line emission, no full-file scan)
- [ ] 2.12 Implement `ClaudeCodeAdapter::start_event_pump(session_id, sink)`: starts the **transcript watcher** (over the session's transcript file) and the **plan watcher** (over `.claude/plans/`), feeding events into the sink. Hook events still arrive via the shared Unix socket (runtime-owned); the watchers are adapter-owned. Implement `stop_event_pump(session_id)` to terminate them cleanly.
- [ ] 2.13 Implement `ClaudeCodeAdapter::submit_plan_decision()` and `submit_ask_answer()` writing to FIFOs (existing logic moved verbatim from `commands.rs:140-193`)
- [ ] 2.14 Implement `ClaudeCodeAdapter::requires_cluihud_setup() -> true`
- [ ] 2.15 Delete the now-empty `src-tauri/src/claude/` directory and `src-tauri/src/tasks/transcript_parser.rs`. Run `grep -rn "crate::claude" src-tauri/src/` and `grep -rn "tasks::transcript_parser" src-tauri/src/` to confirm zero leftover refs. Run `cargo build` clean.

## 3. Refactor hook pipeline through the adapter

- [ ] 3.1 Add `agent_id_cache: Arc<DashMap<SessionId, AgentId>>` field to the app state. Populated **before** PTY spawn in `commands::create_session_with_agent`; entry removed in `destroy_session`. Used by hook dispatcher for O(1) lookup.
- [ ] 3.2 In `src-tauri/src/hooks/server.rs`: when a hook event arrives with `cluihud_session_id`, resolve `agent_id` via:
    1. `agent_id_cache.get(session_id)` — fast path
    2. Fallback: query `db::get_session(session_id)?.agent_id`
    3. If both miss: log `warn!("orphan hook event for session {}; dropping", session_id)` and return early. Do NOT panic, do NOT buffer.
- [ ] 3.3 The dispatcher uses `registry.get(agent_id)` to get the adapter, then delegates parsing-specific logic to the adapter. The Unix socket / dispatch loop infrastructure stays unchanged.
- [ ] 3.4 In `src-tauri/src/hooks/cli.rs`: add `--agent <id>` flag (default `claude-code` for backward compat with existing `~/.claude/settings.json` hook configs that don't pass `--agent`)
- [ ] 3.5 Modify `setup.rs` signature to `pub fn run_setup(agent: AgentId) -> Result<()>`; for `claude-code` produce current output; for other agents the matching adapter's `setup_for_cluihud()` is invoked (returns `Ok(())` for non-setup-required adapters like OpenCode/Pi)
- [ ] 3.6 Add Tauri command `setup_agent(agent_id: String) -> Result<(), String>` exposed to frontend Settings panel; gated by the adapter's `requires_cluihud_setup()` flag (UI hides the button when false)

## 4. PTY spawn through adapter

- [ ] 4.1 In `src-tauri/src/pty.rs`: replace hardcoded `claude` binary references with `adapter.spawn(&ctx)?` returning `SpawnSpec`
- [ ] 4.2 The session store now carries `agent_id: AgentId`; spawn looks up the adapter via registry
- [ ] 4.3 `CLUIHUD_SESSION_ID` env var injection moves into `SpawnSpec.env` (no longer hardcoded in pty.rs)

## 5. DB migration

- [ ] 5.1 Add migration: `ALTER TABLE sessions ADD COLUMN agent_id TEXT NOT NULL DEFAULT 'claude-code'` AND `ALTER TABLE sessions ADD COLUMN agent_internal_session_id TEXT NULL` (used later by Pi/Codex for resume; included now so adapter changes don't need their own migration)
- [ ] 5.2 Update `db.rs` session CRUD (`insert_session`, `update_session`, `get_session`, `list_sessions`) to read/write both new columns. CC sessions write NULL to `agent_internal_session_id` (reserved for Pi/Codex resume — no semantic for CC, which uses `--continue`).
- [ ] 5.3 Backfill verified by SQLite DEFAULT; existing rows acquire `agent_id = 'claude-code'`, `agent_internal_session_id = NULL`
- [ ] 5.4 Document downgrade path explicitly in `CHANGELOG.md`: SQLite < 3.35 cannot DROP COLUMN; downgrade requires DB backup restoration
- [ ] 5.5 Test 11.7 below covers backfill correctness on a populated pre-foundation DB

## 6. Config schema

- [ ] 6.1 Add `agent_overrides: HashMap<String, AgentId>` to `Config` in `src-tauri/src/config.rs` (keys are **canonicalized** path strings, NOT `PathBuf` — to avoid hash mismatches between `/foo/bar` vs `/foo/bar/` vs `/foo/./bar` vs symlinked paths)
- [ ] 6.2 Add `default_agent: Option<AgentId>` to `Config` (user-set fallback when no override matches)
- [ ] 6.3 Add helper `canonicalize_project_path(p: &Path) -> String` using `dunce::canonicalize` for cross-platform safety; falls back to `p.to_string_lossy().into_owned()` if the path doesn't exist yet (deferred canonicalization)
- [ ] 6.4 Lookup priority for resolving "what agent for this session?": `agent_overrides[canonicalize(project_path)]` > `default_agent` > `registry.priority_list()[0_installed]` > error "no agents installed"
- [ ] 6.5 Tauri commands `set_agent_override(project_path, agent_id)` (canonicalizes on write), `clear_agent_override(project_path)`, `set_default_agent(agent_id)`
- [ ] 6.6 Test: write override for `/foo/bar`, read for `/foo/bar/` and `/foo/./bar` and a symlink to `/foo/bar` — all return same AgentId

## 7. Frontend agent store + capability gating (sync, no TOCTOU)

- [ ] 7.1 Backend: extend the `Session` struct that crosses the IPC boundary (`list_sessions`, `get_session`, `session:created`, `session:activated`) to include `agent_id: String` and `agent_capabilities: Vec<String>` (the bitflags serialized form). This avoids any async fetch from frontend.
- [ ] 7.2 Create `src/stores/agent.ts` with `agentAdapterAtom: AgentId | null`, `agentCapabilitiesAtom: Set<string>` (TS uses `Set<string>` exclusively — no enum types crossing the IPC), `availableAgentsAtom: AgentMetadata[]`, `hasCapabilityAtom: atomFamily((cap: string) => atom(get => get(agentCapabilitiesAtom).has(cap)))`
- [ ] 7.3 On session activation event (which now carries `agent_id` and `agent_capabilities` in payload — see 7.1), populate `agentAdapterAtom` and `agentCapabilitiesAtom` **synchronously** (no invoke). Listeners armed afterwards see the capability set already populated.
- [ ] 7.4 Update `stores/hooks.ts`: every event handler checks the relevant capability before mutating downstream atoms (e.g. `plan:ready` requires `PLAN_REVIEW`, `tasks:update` requires `TASK_LIST`, `cost:update` requires `RAW_COST_PER_MESSAGE`). On unknown session_id or missing capability, log `console.warn` and drop.
- [ ] 7.5 Update `stores/tasks.ts`, `stores/plan.ts`, `stores/askUser.ts` to gate writes by capability
- [ ] 7.6 If the frontend ever activates a session before `session:activated` event arrives (race during cold start), the listeners check `agentCapabilitiesAtom.size > 0` and queue events into a small in-memory buffer per session_id (max 100 events); flush when capabilities arrive. This is the only ordering safety net needed.

## 8. UI components capability gating

- [ ] 8.1 `components/plan/PlanPanel.tsx`: `if (!useAtomValue(hasCapabilityAtom("PLAN_REVIEW"))) return null` at the top
- [ ] 8.2 `components/tasks/TaskPanel.tsx`: same for `TASK_LIST`
- [ ] 8.3 `components/session/AskUserModal.tsx`: render only when `ASK_USER_BLOCKING`
- [ ] 8.4 `components/layout/StatusBar.tsx`: cost segment renders tokens (input/output/cache) when `RAW_COST_PER_MESSAGE` is on. **For CC sessions specifically, also render USD via the new Tauri command `get_session_cost_usd(session_id)` which returns `Some(usd)` for CC and `None` for other adapters** (until the future `pricing` module lands)
- [ ] 8.5 `components/sidebar/SessionRow.tsx`: render small badge with adapter id (icon or 2-letter abbrev like "CC", "OC", "Pi", "Cx")
- [ ] 8.6 `components/settings/AgentsSettings.tsx`: list each detected agent with status (installed/not), version (or "detecting..." while async refresh runs), "Run setup" button **only when** `agent.requires_cluihud_setup === true`

## 9. Session creation UX

- [ ] 9.1 In the new-session modal, if `availableAgentsAtom.size > 1`, show agent picker. **Default selection priority** (matches backend Decision 10):
    1. `config.agent_overrides[canonicalize(project_path)]`
    2. `config.default_agent`
    3. First-installed in priority list `[CC, Codex, OpenCode, Pi]`
- [ ] 9.2 If no agents installed, picker is disabled with message "Install at least one agent (Claude Code, Codex, OpenCode, or Pi) to create a session"
- [ ] 9.3 Persist selection in session row's `agent_id`
- [ ] 9.4 Add Tauri command `create_session_with_agent(project_path, agent_id, ...) -> SessionId`. Validates `agent_id` via `AgentId::new`; rejects if registry has no matching adapter or adapter is not installed.

## 10. Setup flow generalization

- [ ] 10.1 Settings panel exposes "Detected agents" section listing each agent with status (configured / not configured) and a "Run setup" button per agent
- [ ] 10.2 "Run setup" triggers `setup_agent(agent_id)`; success toast on completion
- [ ] 10.3 For CC: `setup_agent('claude-code')` calls existing `setup.rs::run_setup()` logic (now parameterized)

## 11. Tests + zero-regression validation

- [ ] 11.1 Integration test: create CC session → trigger plan via ExitPlanMode hook → PlanPanel renders → submit plan decision → FIFO receives decision (end-to-end, post-refactor)
- [ ] 11.2 Integration test: AskUserQuestion → modal renders → submit answer → FIFO receives answers (round-trip)
- [ ] 11.3 Integration test: TodoWrite/TaskCreate/TaskUpdate → TaskPanel updates correctly per tool name
- [ ] 11.4 Integration test: `parse_cost_line` returns `RawCost` with correct token counts on each fixture line type (with/without cache fields, missing model field)
- [ ] 11.5 Integration test: `SessionCostAggregator.add` accumulates correctly across multiple lines; `snapshot()` matches expected totals
- [ ] 11.6 Integration test: openspec watcher emits `openspec:changed` when a file in `openspec/specs/` is modified (verifying the move from `claude/openspec.rs` → `openspec.rs` preserves behavior)
- [ ] 11.7 Integration test: DB migration on a populated pre-foundation DB. Setup: create DB with sessions table sin `agent_id`/`agent_internal_session_id`, insert 3 fixture rows. Run migration. Assert: all 3 rows acquire `agent_id = 'claude-code'`, `agent_internal_session_id = NULL`. Assert: SELECT round-trips correctly with the new struct.
- [ ] 11.8 Integration test: annotation injection via UserPromptSubmit hook — set `pending_annotations`, fire UserPromptSubmit, assert prompt is mutated with annotations injected (this exercises `hooks/cli.rs::inject_edits` end-to-end through the post-refactor adapter routing)
- [ ] 11.9 Integration test: session resume — create session, kill cluihud, restart, resume the session via `--continue`. Verify `pty.rs` invokes `claude --continue`.
- [ ] 11.10 Integration test: file-changed event from PostToolUse[Edit] → `file:changed` Tauri event → `ModifiedFiles` panel update
- [ ] 11.11 Integration test: hook event with unknown session_id → dropped + warn log; no panic, no atom mutations
- [ ] 11.12 Integration test: agent_id resolution cache miss → DB fallback → cache populated for next event
- [ ] 11.13 Manual UX walk (post-refactor): open existing session, verify sidebar/topbar/plan/tasks/cost (USD)/askuser all render identically to pre-refactor screenshots saved for comparison
- [ ] 11.14 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` clean
- [ ] 11.15 Frontend: `npx tsc --noEmit` clean

## 12. Documentation

- [ ] 12.1 Update `CLAUDE.md` with new module layout (`agents/<id>/` instead of `claude/`)
- [ ] 12.2 Add `docs/agents/architecture.md` summarizing the trait, capabilities, transports, registry
- [ ] 12.3 Update OpenSpec `cc-adapter` spec.md (lands as part of this change) describing CC's adapter contract
