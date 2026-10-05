# Implementation Plan: cc-agent-cache-effort-signals

> Grounded in the codebase at `cc08162`, symbols verified 2026-10-05. Behaviour (not just symbol existence) verified for the load-bearing claims below.

## Verified codebase facts (do not re-assume)

- `src-tauri/src/setup.rs:58-65` — the only Notification entry: matcher `permission_prompt`, command `nergal hook notification`, async.
- `src-tauri/src/setup.rs:363-402` (`merge_hook`) — finds an existing entry by command (`matches_hook_command`) and rewrites only its `matcher` when it differs; returns `true`. New entries append `{matcher?, hooks:[{type,command,async?,timeout?}]}`.
- `src-tauri/src/setup.rs:597` — `merge_hook_upgrades_matcher_in_place_for_wrapper_entry` is the test pattern to copy.
- `src-tauri/src/main.rs:34-38` — `hook send` ignores its trailing args; the payload's `hook_event_name` picks the `HookEvent` variant (`events.rs:5`, `#[serde(tag = "hook_event_name")]`).
- `src-tauri/src/hooks/cli.rs:492-546` (`notification`) — forwards `session_id`, `notification_type`, `message` for any type; exits early without `NERGAL_SESSION_ID`.
- `src-tauri/src/hooks/cli.rs:653-686` (`statusline_to_agent_status`) — pure mapper, all fields optional; tests at `cli.rs:749-827`.
- `src-tauri/src/hooks/events.rs:87-94` — `Notification {session_id, notification_type?, message?}`.
- `src-tauri/src/hooks/events.rs:154-180` — `AgentStatus` variant; every field `#[serde(default)]`.
- `src-tauri/src/hooks/events.rs:190-206` (`mcp_mode`) — exhaustive match; `Notification` → `needs_attention`. `waiting_for` (`:211`) and `session_id` (`:239`) also match variants.
- `src-tauri/src/hooks/server.rs:17-31` — `FrontendHookEvent`; `from_hook` (`:34`) is an exhaustive tuple match; test `from_hook_maps_every_event_variant_to_its_event_type` at `:1627` lists every variant.
- `src-tauri/src/hooks/server.rs:402-450` (`session_log_line`) — ends with `_ => None`, new variants need no arm.
- `src-tauri/src/hooks/server.rs:937-973` — `Stop` with `transcript_path` calls `parse_cost_from_transcript`, upserts cost, emits `cost:update`.
- `src-tauri/src/hooks/server.rs:1127-1184` — `StatusLine` (legacy) and `AgentStatus` arms build `AgentStatusEmit`; the `AgentStatus` destructure at `:1153` has no `..`. `emit_agent_status` at `:1248` is the single emit path. Only `server.rs` constructs `AgentStatusEmit`.
- `HookEvent::AgentStatus` literals outside server.rs: `agents/pi/jsonl_tail.rs:208`, `agents/codex/rollout_tail.rs:120`, `agents/opencode/sse_client.rs:249`; test literal `server.rs:1761`.
- `src-tauri/src/hooks/server.rs:741-786` — `process_event` registers orphan sessions and calls `agent_state.record_activity` before the `hook:event` emit at `:815`. `AgentRuntimeState::session_activity` (`agents/state.rs:217`) reads the recorded mode.
- `src-tauri/src/setup.rs:33-38` — `PreToolUse` is unmatched, so the `Agent` tool adds a running `tool_use` row (`hooks.ts:83-95`); `Agent` is not in the PostToolUse matcher (`setup.rs:41-43`).
- `src-tauri/src/hooks/server.rs:1204-1224` — `Notification` arm emits `attention:pending` for every type.
- `src-tauri/src/agents/claude_code/cost.rs:51-92` (`parse_cost_from_transcript`) — line-by-line `BufReader` over the transcript; skips non-JSON lines.
- `src/stores/hooks.ts:76-181` — `hook:event` switch; `default` adds `Event: <type>` (so `notification` currently logs a duplicate entry). `stop`/`post_tool_use`/`user_prompt_submit`/`session_end` clear `pendingAttentionAtom`.
- `src/stores/hooks.ts:328-344` — `attention:pending` sets `pendingAttentionAtom`, adds `Claude needs attention: <type>`, calls `notify`.
- `src/stores/askUser.ts:9` — `pendingAttentionAtom`; `TopBar.tsx:476`, `SessionRow.tsx:62`, `SessionIndicator.tsx:66-67` render it exactly like `pendingAsksAtom`.
- `src/stores/workspace.ts:193-219` — `AgentStatus` TS type + default.
- `src/stores/activity.ts:32-56` (`completeToolActivityAtom`) — pairing pattern to mirror for subagents.
- `src/lib/types.ts:6-18` (`HookEvent`), `:172-189` (`ActivityEntry`).
- `src/components/activity/ActivityDrawer.tsx:17-34` and `ActivityLog.tsx:12-19` — `Record<ActivityEntry["type"], string>` color maps; adding a type is a compile error until both are filled.
- `src/components/layout/StatusBar.tsx:53-58` (`rateLimitColor`), `:313-327` model + effort chips, `:268-287` context chip (pattern to copy).
- Real transcript shape (CC 2.1.289, local `~/.claude/projects/*/*.jsonl`): assistant records carry top-level `"effort":"medium"` and `"perTurnEffort":"medium"|null`.

## Execution order

1. Rust events + setup (Group 1): `events.rs` variants, `setup.rs` HOOKS, `server.rs` dispatch.
2. Frontend attention + subagent activity (Group 2): `types.ts`, `agentTelemetry.ts`, `activity.ts`, `hooks.ts`, drawer/log colors.
3. Prompt cache (Group 3): `cli.rs` mapper, `events.rs`, `server.rs` emit, `workspace.ts`, `StatusBar.tsx`.
4. Effort (Group 4): `cost.rs` single-pass extractor, `server.rs` Stop emit, store atom, `StatusBar.tsx`.
5. Docs: `docs/hooks.md` hook list, `docs/design.md` §1.2 sky-400 example.

## Plan

### Group 1 — backend events
- `events.rs`: add `SubagentStart {session_id, agent_id?, agent_type?}` and `SubagentStop {session_id, agent_id?, agent_type?, last_assistant_message?, effort: Value}`; helper `HookEvent::is_attention_notification()`; `mcp_mode`: Notification → `needs_attention` unless type is `agent_completed` (→ `None`); `waiting_for` likewise; subagent variants → `None`.
- `setup.rs` HOOKS only: Notification matcher widened; two new async entries with matcher `.+`.
- `server.rs`: `FrontendHookEvent` + `agent_id`, `agent_type`, `agent_message`, `effort_level` (skip if None); `from_hook` arms `subagent_start`/`subagent_stop`; pure `is_droppable_subagent_event` checked at the top of `process_event`; Notification arm splits `attention:pending` vs `agent:background-completed`; pure `mcp_activity_after_completion` decides the `idle` reset on `agent_completed` (only for a `waiting_for` with the `background agent` prefix from `events.rs` `waiting_for()`).

### Group 2 — frontend
- `src/lib/agentTelemetry.ts` (pure): `notificationAttentionMessage(type)`, `applySubagentStart(list, payload)`, `applySubagentStop(list, payload)`, `formatMissCauses`, `promptCacheChip`, `effortChipText`.
- `types.ts`: `ActivityEntry.type` + `"subagent"`, `agentId?`; `HookEvent` + optional subagent fields.
- `activity.ts`: `startSubagentActivityAtom` / `completeSubagentActivityAtom` delegating to the pure helpers; `completeToolActivityAtom` uses pure `pickToolRowToComplete`.
- `askUser.ts`: `pendingAttentionAtom` becomes `sessionScopedMapAtom<string>` (value = notification type); `SessionIndicator.tsx:62-63` parameter type follows.
- `hooks.ts`: `notification` case no-op; `subagent_start`/`subagent_stop` cases; `attention:pending` copy via helper; new `agent:background-completed` listener.

### Group 3 — prompt cache
- `cli.rs`: `prompt_cache_status(cc)` pure; `statusline_to_agent_status` includes it.
- `events.rs`: `PromptCacheStatus` struct (all `Option`, `last_miss_causes: Vec<String>` default) and `prompt_cache: Option<PromptCacheStatus>` on `AgentStatus`.
- `server.rs`: thread `prompt_cache` through `AgentStatusEmit` / payload; legacy `StatusLine` → `None`.
- `workspace.ts`: `PromptCacheStatus` TS type, `prompt_cache` field (default `null`).
- `StatusBar.tsx`: chip after the context chip, `DatabaseZap` icon, tooltip lines.

### Group 4 — effort
- `cost.rs`: `TranscriptScan {cost, last_effort}` from one pass; `parse_cost_from_transcript` delegates; pure `last_assistant_effort_in_line` used per line.
- `server.rs` Stop arm: emit `effort:last-response` when `Some`.
- `workspace.ts`: `lastResponseEffortMapAtom` (session-scoped) + active selector; `hooks.ts` listener.
- `StatusBar.tsx`: chip text via `effortChipText`.

## Per-phase risk

- [Exhaustive matches miss a new variant] → compiler enforces; the `from_hook` test lists both new variants.
- [FRI-498 merge conflict in `setup.rs`] → only `HOOKS` rows + tests module; report it.
- [A malformed `prompt_cache` breaks the whole AgentStatus deserialize] → the CLI mapper builds the object field by field with `as_*` accessors, so the server only sees well-typed values.
- [`effort` key on non-assistant records] → filter `type == "assistant"`.
- [Transcript read twice at Stop] → single pass returns both cost and effort.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit && pnpm test`
- Manual walk in the installed app (see tasks.md §5).
