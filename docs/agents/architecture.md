# Agent-agnostic architecture

Cluihud was built as a Claude Code (CC) wrapper. The `agent-adapter-foundation`
change extracts the CC integration into an `AgentAdapter` trait so other CLI
agents (OpenCode, Pi, Codex, …) can plug in without duplicating the
hooks/transcript/plan/cost integration per-agent.

This file is the curator's-eye view of the layout. The OpenSpec change docs
live in `openspec/changes/agent-adapter-foundation/` and carry the design
decisions, risks, and review history; this doc just maps the runtime shape.

## Layout

```
src-tauri/src/
├── agents/
│   ├── mod.rs                   AgentAdapter trait, AgentId newtype,
│   │                            AgentCapability bitflags, Transport enum,
│   │                            SpawnContext/Spec, TranscriptEvent, RawCost,
│   │                            PlanDecision, AdapterError, EventSink alias
│   ├── registry.rs              AgentRegistry (register / get / scan /
│   │                            priority_list / register_supplementary_adapters)
│   ├── state.rs                 AgentRuntimeState (registry + agent_id_cache
│   │                            + typed Arc<ClaudeCodeAdapter>)
│   ├── cost_aggregator.rs       SessionCostAggregator (running per-session totals)
│   └── claude_code/
│       ├── mod.rs               re-exports ClaudeCodeAdapter
│       ├── adapter.rs           ClaudeCodeAdapter impl AgentAdapter
│       ├── transcript.rs        `.jsonl` notify watcher
│       ├── plan.rs              plan file watcher + PlanManager
│       ├── cost.rs              parse_cost_from_transcript (legacy) +
│       │                        parse_cost_line + legacy_usd_for_sonnet4
│       └── tasks_parser.rs      transcript → TaskStore parser
└── openspec.rs                  workspace-scoped OpenSpec artifact reader
                                 (NOT agent-bound; any agent in a project with
                                 openspec/ should see the panel — Decision 8)
```

## Core types (cheat sheet)

- **`AgentAdapter`** (trait, in `agents/mod.rs`): every adapter implements this.
  Methods grouped by purpose:
  - Identity & advertising: `id`, `display_name`, `capabilities`, `transport`,
    `requires_cluihud_setup`.
  - Detection: `detect` (lightweight, no spawn), `refresh_version` (async,
    background — the version probe is slow because it shells out).
  - Lifecycle: `spawn(ctx) -> SpawnSpec`, `start_event_pump(session_id, sink)`,
    `stop_event_pump(session_id)`.
  - Per-line parsing: `parse_transcript_line` (sync, hot path).
  - Capability-gated ops: `submit_plan_decision`, `submit_ask_answer` (default
    `Err(NotSupported)`).

- **`AgentCapability`** (bitflags): UI gates by bit. Wire form is
  `Vec<&'static str>` via custom Serialize so TS can mirror it as a string union.

- **`AgentId`** (string newtype): validated against `^[a-z][a-z0-9-]{0,31}$`.
  Known constructors (`AgentId::claude_code()`, `opencode()`, `pi()`, `codex()`)
  bypass validation but the round-trip test in `mod.rs::tests` enforces they
  remain valid.

- **`Transport`** (enum, data not herencia): `FileHooks` (CC, Codex), `HttpSse`
  (OpenCode), `JsonlTail` (Pi), `RpcStdio` (reserved). The runtime dispatcher
  routes by variant; adapters carry the protocol-specific path/url info inside
  the variant.

- **`AgentRegistry`**: in-process map of `Arc<dyn AgentAdapter>` keyed by
  `AgentId`. Duplicate registrations are rejected. `priority_list()` is the
  stable codified default order: CC > Codex > OpenCode > Pi (Decision 10).

- **`AgentRuntimeState`** (Tauri-managed): `Arc<AgentRegistry>` + a
  `DashMap<cluihud_session_id, AgentId>` cache + a typed
  `Arc<ClaudeCodeAdapter>` for CC-specific side-channels (FIFO registration).
  Clone is cheap. Bootstrapped at app startup from `bootstrap()`.

- **`SessionCostAggregator`**: owns a `parking_lot::Mutex<SessionCostTotals>`.
  Sync `add()` is intentional — `parse_transcript_line` is a hot path;
  awaiting on a `tokio::Mutex` per line would pollute the call site.

## Lifecycle of a session

```
            ┌─────────────────────────────────────────────────┐
            │ user clicks "new session" in workspace sidebar   │
            └────────────────────┬─────────────────────────────┘
                                 ▼
              commands::create_session(workspace_id, name, agent_id?)
                                 │
                                 ▼
              ┌───────────────────────────────────────────┐
              │ DB INSERT (agent_id stored on row)         │
              │ AgentRuntimeState.register_session()       │  ← cache populated
              │   (BEFORE PTY spawn, Decision 9)           │     before any hook
              └─────────────────────┬─────────────────────┘
                                    ▼
              pty::start_claude_session(session_id, …)
                                    │
                                    ▼
              registry.get(agent_id) → adapter.spawn(ctx)
                                    │
                                    ▼
              shell PTY spawned with CLUIHUD_SESSION_ID env;
              composed `<binary> <args> \n` written into the shell
                                    │
              ┌─────────────────────┼─────────────────────┐
              ▼                     ▼                     ▼
    SessionStart hook fires  transcript watcher    plan watcher
    →    socket → server          (CC today: app-global; future: adapter-owned)
                                    │
                                    ▼
              dispatcher resolves agent_id via cache (or DB fallback)
              dispatches event to per-agent handler;
              FrontendHookEvent emitted to React.
```

When the session ends (`commands::delete_session`):

1. `agents.forget_session()` clears the cache entry.
2. DB `DELETE` cascades.
3. PTY teardown via existing path.
4. (Future) `adapter.stop_event_pump(session_id)` for adapter-owned watchers.

## Cost flow (foundation, transitional)

Two flows exist side-by-side until the generic `pricing` module lands:

- **Legacy whole-file path** (current production):
  `claude_code/cost.rs::parse_cost_from_transcript` walks the JSONL on `Stop`
  hook; the runtime emits `cost:update` to the frontend. Used for the status
  bar USD figure.
- **Streaming path** (foundation seam): adapters emit `TranscriptEvent::Cost`
  per line via `parse_transcript_line`; `SessionCostAggregator` accumulates;
  the runtime calls `legacy_usd_for_sonnet4` (private to the CC adapter) for
  the USD figure. Other adapters return `None` until the pricing module is
  built.

Both paths agree on USD for CC sessions — see
`tests/agent_foundation_cost.rs`.

## Capability gating in the frontend

The wire shape `agent_capabilities: string[]` lands in `Session.agent_capabilities`
(see `models.rs`). The frontend store (`src/stores/agent.ts`) populates
`activeAgentMetadataAtom` from session activation; `hasCapabilityAtom` is the
canonical UI gate. `FULL_CAPABILITY_SET` is the default for sessions that
predate the field being populated end-to-end so the foundation rolls out
without UX regression for existing CC users.

## What this foundation deliberately does NOT do

- **No automatic plan-mode synthesis** for adapters that don't have it.
  Capability gating hides the panel — Decision 3.
- **No mid-flight agent switching**. A session is bound to one adapter at
  creation — Decision 5.
- **No DB downgrade**. SQLite < 3.35 can't `DROP COLUMN`; the migration is
  forward-only. CHANGELOG documents the recovery path (restore backup).
- **No cross-adapter session resume**. Each agent has its own resume
  semantics; cluihud doesn't try to unify them.

## Adding a new adapter (preview for opencode/pi/codex)

1. Create `src-tauri/src/agents/<id>/`:
    - `adapter.rs` with `<Id>Adapter: AgentAdapter`
    - `mod.rs` re-exporting the adapter
    - any protocol-specific submodules (SSE client, JSONL tail, etc.)
2. Add `pub mod <id>;` to `agents/mod.rs`.
3. Append a single `reg.register(Arc::new(<Id>Adapter::new()))?;` line to
   `agents/registry.rs::register_supplementary_adapters`.
4. Update `AgentRegistry::priority_list()` only if the new adapter belongs in
   a specific slot — otherwise it falls in by registration order at the end.
5. Tests:
    - Capability set declared correctly.
    - `detect()` is filesystem-only (no spawn).
    - `parse_transcript_line` (or its absence — return `None` if the adapter
      is event-driven only) round-trips a fixture.
    - `start_event_pump` cleans up on `stop_event_pump`.

The trait, registry, dispatcher, and frontend gating require **no changes**
to add a new adapter. That is the foundation's contract.

## References

- OpenSpec change docs: `openspec/changes/agent-adapter-foundation/`
- Pending follow-ups: `openspec/changes/{opencode,pi,codex}-adapter/`
- Iterative review log: `openspec/changes/agent-adapter-foundation/.review-history.md`
