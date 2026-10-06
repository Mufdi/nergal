# API contract (shared by test-writer and Builder)

Names and signatures the Red tests target. The Builder implements exactly these.

## Rust

`src-tauri/src/hooks/events.rs`
- `pub const BACKGROUND_INPUT_WAITING_PREFIX: &str = "background agent";`
- `HookEvent::SubagentStart { session_id: String, #[serde(default)] agent_id: Option<String>, #[serde(default)] agent_type: Option<String> }` (serde rename `SubagentStart`)
- `HookEvent::SubagentStop { session_id: String, #[serde(default)] agent_id: Option<String>, #[serde(default)] agent_type: Option<String>, #[serde(default)] last_assistant_message: Option<String>, #[serde(default)] effort: serde_json::Value }` (serde rename `SubagentStop`)
- `impl HookEvent { pub fn effort_level(&self) -> Option<String> }` — `effort.level` string of `SubagentStop`, else `None`.
- `mcp_mode()`: Notification → `Some("needs_attention")` except `notification_type == Some("agent_completed")` → `None`. Subagent variants → `None`.
- `waiting_for()`: Notification `agent_needs_input` → `"background agent: <message>"`, or `"background agent"` when `message` is `None`. Notification `agent_completed` → `None`. Other notification types unchanged (message, else type, else `"notification"`). Subagent variants → `None`.
- `#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)] pub struct PromptCacheStatus { #[serde(default)] pub warm: Option<bool>, caching_observed: Option<bool>, ttl: Option<String>, expires_at: Option<u64>, requests: Option<u64>, misses: Option<u64>, hit_ratio: Option<f64>, miss_recache_tokens: Option<u64>, last_miss_at: Option<u64>, #[serde(default)] last_miss_causes: Vec<String> }` (all fields `pub`, all `#[serde(default)]`).
- `HookEvent::AgentStatus` gains `#[serde(default)] prompt_cache: Option<PromptCacheStatus>`.

`src-tauri/src/hooks/cli.rs`
- `fn prompt_cache_status(cc: &serde_json::Value) -> serde_json::Value` — `Value::Null` when `cc.prompt_cache` is absent or not an object; otherwise an object with ALL keys `warm, caching_observed, ttl, expires_at, requests, misses, hit_ratio, miss_recache_tokens, last_miss_at, last_miss_causes` (missing/mistyped → `null`; `last_miss_causes` → `[]` unless `last_miss_cause.causes` is an array of strings).
- `statusline_to_agent_status` output gains key `"prompt_cache"` = `prompt_cache_status(cc)`.

`src-tauri/src/hooks/server.rs`
- `FrontendHookEvent` gains `agent_id`, `agent_type`, `agent_message`, `effort_level: Option<String>`, each `#[serde(skip_serializing_if = "Option::is_none")]`.
- `from_hook`: `SubagentStart` → `event_type "subagent_start"`, `agent_id`, `agent_type`; `SubagentStop` → `"subagent_stop"`, `agent_id`, `agent_type`, `agent_message` = `last_assistant_message`, `effort_level` = `effort.level`.
- `fn is_droppable_subagent_event(event: &HookEvent) -> bool` — true for `SubagentStart`/`SubagentStop` whose `agent_type` is `None` or empty; false for every other event.
- `fn mcp_activity_after_completion(mode: &str, waiting_for: Option<&str>) -> Option<&'static str>` — `Some("idle")` only when `mode == "needs_attention"` and `waiting_for` starts with `BACKGROUND_INPUT_WAITING_PREFIX`; else `None`.

`src-tauri/src/agents/claude_code/cost.rs`
- `pub fn assistant_effort_in_line(line: &str) -> Option<String>` — top-level `effort` string of a JSON record whose `type == "assistant"`; `None` otherwise (non-JSON, other types, missing/non-string effort).
- `pub struct TranscriptScan { pub cost: CostSummary, pub last_effort: Option<String> }` + `pub fn scan_transcript(path: &Path) -> TranscriptScan` — one pass; `cost` equals `parse_cost_from_transcript(path)`; `last_effort` = last non-`None` `assistant_effort_in_line`. Missing file → default cost, `None`.

`src-tauri/src/setup.rs` (HOOKS table only)
- Notification row matcher `permission_prompt|agent_needs_input|agent_completed`.
- `SubagentStart`: matcher `.+`, command `nergal hook send subagent-start`, async, no timeout.
- `SubagentStop`: matcher `.+`, command `nergal hook send subagent-stop`, async, no timeout.

## TypeScript

`src/lib/types.ts`
- `ActivityEntry.type` adds `"subagent"`; `ActivityEntry.agentId?: string`.
- `HookEvent` adds optional `agent_id?, agent_type?, agent_message?, effort_level?: string`.

`src/lib/agentTelemetry.ts` (pure, no Jotai, no Tauri)
- `export interface PromptCacheStatus { warm: boolean | null; caching_observed: boolean | null; ttl: string | null; expires_at: number | null; requests: number | null; misses: number | null; hit_ratio: number | null; miss_recache_tokens: number | null; last_miss_at: number | null; last_miss_causes: string[] }`
- `notificationAttentionMessage(type?: string | null): string` — `"agent_needs_input"` → `"Background agent needs input"`; otherwise `` `Claude needs attention: ${type ?? "input"}` ``.
- `shouldClearOnBackgroundCompletion(value: string | undefined): boolean` — true only for `"agent_needs_input"`.
- `pickToolRowToComplete(list: ActivityEntry[], toolName?: string): number` — index of the last `tool_use` + `running` entry with `toolName === toolName`; else the last running `tool_use` whose `toolName` is not `"Agent"`/`"Task"`; else `-1`.
- `applySubagentStart(list: ActivityEntry[], p: { agentId: string; agentType: string; id: string; timestamp: number }): ActivityEntry[]` — new array. Claims the newest `tool_use` + `running` entry with `toolName` `"Agent"` or `"Task"` and no `agentId`: becomes `{...entry, type: "subagent", agentId, message: "Subagent: <agentType>"}` (keeps its id/timestamp/status). Otherwise appends `{id, timestamp, type: "subagent", message: "Subagent: <agentType>", status: "running", agentId}`.
- `applySubagentStop(list: ActivityEntry[], p: { agentId: string; agentType: string; message?: string | null; effort?: string | null; id: string; timestamp: number }): ActivityEntry[]` — label `Subagent: <agentType>` plus `` ` · ${effort}` `` when effort. Last `subagent` + `running` entry with the same `agentId` → `{...entry, message: label, status: "done", durationMs: p.timestamp - entry.timestamp, detail: p.message ?? entry.detail}`. None → append `{id, timestamp, type: "subagent", message: label, status: "done", agentId, detail: p.message ?? undefined}`.
- `formatMissCauses(causes: string[]): string` — maps `tools_changed`→`tool definitions changed`, `system_prompt_changed`→`system prompt changed`, `ttl_expired_5m`→`idle past the 5m TTL`, `ttl_expired_1h`→`idle past the 1h TTL`, `likely_server_side`→`likely server-side eviction`, other → `_` replaced by spaces; joined with `", "`.
- `promptCacheChip(pc: PromptCacheStatus | null | undefined): string | null` — `null` when `pc` is nullish, `pc.caching_observed === false`, or `pc.hit_ratio` is not a number; else `` `${Math.round(hit_ratio*100)}%` `` plus `" cold"` when `pc.warm === false`.
- `effortChipText(setting: string | null | undefined, lastResponse: string | null | undefined): string | null` — both null → `null`; only one → that one; equal → setting; different → `` `${setting} ran ${lastResponse}` ``.
