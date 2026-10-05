## 1. Background-agent attention and subagent hooks (backend)

- [x] 1.1 Test (Red): `events.rs` deserializes `SubagentStart`/`SubagentStop` with and without `agent_type`/`effort`/`last_assistant_message`; `mcp_mode` is `needs_attention` for `agent_needs_input` + `permission_prompt` and `None` for `agent_completed` and subagent events; `waiting_for` is `None` for `agent_completed`
- [x] 1.2 Test (Red): `setup.rs` HOOKS contain Notification `permission_prompt|agent_needs_input|agent_completed`, async `SubagentStart`/`SubagentStop` with matcher `.+`; `merge_hook` upgrades an existing `permission_prompt` Notification entry in place without a duplicate
- [x] 1.3 Test (Red): `server.rs` `from_hook` maps `SubagentStart`/`SubagentStop` to `subagent_start`/`subagent_stop` with `agent_id`, `agent_type`, `agent_message`, `effort_level`; `is_droppable_subagent_event` is true for empty/missing `agent_type`; `mcp_activity_after_completion` turns a `needs_attention` activity whose `waiting_for` starts with `background agent` into `idle` with no `waiting_for`, and leaves permission/plan/ask waits and other modes alone
- [x] 1.4 Add `SubagentStart`/`SubagentStop` variants, `notification` attention helper, `mcp_mode`/`waiting_for`/`session_id` arms in `src-tauri/src/hooks/events.rs`
- [x] 1.5 Widen the Notification matcher and add the two subagent rows in the `HOOKS` table of `src-tauri/src/setup.rs` (table only)
- [x] 1.6 Extend `FrontendHookEvent` + `from_hook`; drop empty-`agent_type` subagent events at the top of `process_event` (before orphan registration, MCP activity, emits); split the Notification arm into `attention:pending` vs `agent:background-completed`; on `agent_completed` reset a `needs_attention` MCP activity to `idle` in `src-tauri/src/hooks/server.rs`

## 2. Background-agent attention and subagent activity (frontend)

- [x] 2.1 Test (Red): `src/lib/agentTelemetry.test.ts` covers `notificationAttentionMessage`, `pickToolRowToComplete` (same-name match, any-name fallback skips `Agent`/`Task`), `applySubagentStart` (claims the newest unclaimed running `Agent`/`Task` row, appends when none, ignores a row already claimed), `shouldClearOnBackgroundCompletion` (true only for `agent_needs_input`), `applySubagentStop` (pairing by `agentId`, standalone fallback, concurrent agents, effort suffix, detail from last message)
- [x] 2.2 Implement `src/lib/agentTelemetry.ts` subagent + notification helpers
- [x] 2.3 Add `"subagent"` + `agentId` to `ActivityEntry` and subagent fields to `HookEvent` in `src/lib/types.ts`; fill the color maps and filter order in `ActivityDrawer.tsx` and `ActivityLog.tsx` (sky-400)
- [x] 2.4 Add `startSubagentActivityAtom` and `completeSubagentActivityAtom` in `src/stores/activity.ts`; exclude `Agent`/`Task` rows from the any-name fallback of `completeToolActivityAtom` (pure `pickToolRowToComplete` tested in 2.1)
- [x] 2.5 Wire `subagent_start`/`subagent_stop`/`notification` cases, the `attention:pending` copy and the `agent:background-completed` listener (activity entry + notify + clear `pendingAttentionAtom` only when its value is `agent_needs_input`); `pendingAttentionAtom` stores the notification type (`src/stores/askUser.ts`, `SessionIndicator.tsx` type) in `src/stores/hooks.ts`

## 3. Prompt cache in the status bar

- [x] 3.1 Test (Red): `cli.rs` maps the full documented `prompt_cache` object, maps an absent object to `null`, and maps `hit_ratio: null` + `last_miss_cause: null` to `null` + `[]`
- [x] 3.2 Test (Red): `events.rs` deserializes `AgentStatus` with and without `prompt_cache`
- [x] 3.3 Test (Red): `agentTelemetry.test.ts` covers `promptCacheChip` (warm, cold, not observed, null ratio) and `formatMissCauses` (the five documented ids, unknown fallback, several causes joined)
- [x] 3.4 Implement `prompt_cache_status` in `src-tauri/src/hooks/cli.rs` and the `PromptCacheStatus` struct in `events.rs`; add `prompt_cache: None` to the `HookEvent::AgentStatus` literals in `agents/pi/jsonl_tail.rs`, `agents/codex/rollout_tail.rs`, `agents/opencode/sse_client.rs` and the `server.rs` test literal
- [x] 3.5 Thread `prompt_cache` through the `AgentStatus` destructure, `AgentStatusEmit` and `emit_agent_status` in `server.rs` (legacy `StatusLine` arm passes `None`)
- [x] 3.6 Add `PromptCacheStatus` to `AgentStatus` in `src/stores/workspace.ts` and render the chip + tooltip in `StatusBar.tsx`

## 4. Last-response effort

- [x] 4.1 Test (Red): `cost.rs` single-pass scan returns the last assistant `effort`, `None` for older transcripts, skips malformed lines and non-assistant records, and keeps cost totals unchanged
- [x] 4.2 Test (Red): `agentTelemetry.test.ts` covers `effortChipText` (same, different, setting only, transcript only, none)
- [x] 4.3 Implement the single-pass scan in `src-tauri/src/agents/claude_code/cost.rs` and emit `effort:last-response` from the Stop arm in `server.rs`
- [x] 4.4 Add `lastResponseEffortMapAtom` + listener, clear it on `user_prompt_submit`, and render the effort chip via `effortChipText` (tooltip `Setting:` / `Last response:`) in `StatusBar.tsx`

## 5. Review fixes (adversarial review)

- [x] 5.1 Test (Red): `preserves_background_wait` (server.rs), null `last_miss_causes` (events.rs), `closeUnclaimedSubagentToolRows` + `shouldClearOnTurnEvent` (agentTelemetry.test.ts)
- [x] 5.2 Keep background waits across foreground PreToolUse/PostToolUse/Stop in `process_event` and in the `post_tool_use`/`stop` cases of `src/stores/hooks.ts`
- [x] 5.3 Close unclaimed `Agent`/`Task` rows at `stop` via `closeUnclaimedSubagentRowsAtom` in `src/stores/activity.ts`
- [x] 5.4 Null-tolerant `last_miss_causes`; `scan_transcript` reuses `assistant_effort`

## 6a. Status snapshots are not activity

- [x] 6a.1 Test (Red): `agentTelemetry.test.ts` covers `shouldLogGenericActivity` (`agent_status` and `statusline` skipped, unknown types logged)
- [x] 6a.2 Skip the generic `Event: <type>` row for those types in the `default:` branch of `src/stores/hooks.ts`

## 6b. Subagent stop closes its own rows (FRI-501)

- [x] 6b.1 Test (Red): `server.rs` forwards `agent_id`/`agent_type` on pre/post tool events only when present; `agentTelemetry.test.ts` covers tagged-row close per agent, handback detail, head move with stop time and duration
- [x] 6b.2 Add `agent_id`/`agent_type` to `HookEvent::PreToolUse`/`PostToolUse` (`events.rs`) and forward them in `FrontendHookEvent::from_hook` (`server.rs`)
- [x] 6b.3 Tag tool rows with `ownerAgentId` and capture the handback message in `src/stores/hooks.ts`; close tagged rows, set detail and move the row to the head in `applySubagentStop` (`src/lib/agentTelemetry.ts`)
- [ ] 6b.4 Manual walk: background Explore subagent; its row appears at the top when it finishes with the report as detail and no pulsing `SubagentHandback` row remains

## 6. Docs and Verification

- [x] 6.1 Update `docs/hooks.md` (Notification types, subagent hooks) and `docs/design.md` §1.2 (sky-400 also encodes subagent activity)
- [x] 6.2 Run `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit && pnpm test`
- [x] 6.3 Audit added comments (WHY only) and run the placeholder scan on the diff
- [ ] 6.4 Manual walk (installed app, after `nergal setup`): `~/.claude/settings.json` shows the widened Notification matcher and the two subagent entries
- [ ] 6.5 Manual walk: ask Claude to run two background Explore subagents; Activities shows exactly two `Subagent: Explore` rows (no extra `Agent` tool rows) that turn done with duration and last message
- [ ] 6.6 Manual walk: after a few turns the status bar shows the cache chip; idle past the TTL then send a prompt; the chip shows `cold` or a miss with its cause in the tooltip
- [ ] 6.7 Manual walk: run `/effort low`, send a prompt; the effort chip reads `low`; the tooltip lists the last response effort
- [ ] 6.8 Manual walk: run `claude agents` inside a Nergal session with a background session that asks for input; the session tab gets the attention tint; when the background session finishes (`agent_completed`) the tint clears and Activities shows `Background agent finished` (or record that the signal does not reach Nergal)
