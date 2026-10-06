Linear: FRI-501

## Why

Claude Code now exposes agent, cache and effort data that Nergal drops. A background subagent that finishes leaves no trace in Activities. A `Notification` of type `agent_needs_input` or `agent_completed` never reaches Nergal, because the hook matcher only lets `permission_prompt` through. The cost of a prompt-cache miss is invisible in the status bar.

Verified payload facts (CC v2.1.289 docs + local transcripts) correct two assumptions from the issue:

- `agent_needs_input` and `agent_completed` describe **background sessions in agent view** (and teammate or auto-mode notices), not in-session subagents. In-session subagents report through `SubagentStart` / `SubagentStop`.
- No hook payload carries a parent agent id. A nested subagent tree (depth up to 3) cannot be rebuilt from hooks.

## What Changes

- The `Notification` hook matcher grows to `permission_prompt|agent_needs_input|agent_completed`.
- `agent_needs_input` gets the attention treatment (tab tint, sidebar indicator, activity entry, desktop notification).
- `agent_completed` becomes an activity entry and a desktop notification. It does not set attention. It clears a pending notification attention of the same session, because a background agent is answered in agent view and fires no other clearing event.
- New hooks `SubagentStart` and `SubagentStop` (matcher `.+`, async) feed subagent lifecycle entries into Activities. Entries pair by `agent_id`, show duration, the subagent's last message and its effort level. A subagent start takes over the running `Agent` tool row, so each subagent shows once.
- The statusline feeder forwards CC's `prompt_cache` object. The status bar shows the hit ratio, a `cold` marker when the cache is cold, and the last miss cause in the tooltip.
- At `Stop`, the backend reads the effort recorded on the last assistant message of the transcript. The model only appears in the status bar, so "effort where the model is shown" maps to the status bar effort chip next to the model. The chip shows the last-response effort when it differs from the current setting.
- All parsers tolerate missing fields from older CC versions.

## Capabilities

### New Capabilities
- `agent-status-telemetry`: status bar chips fed by the CC statusline and transcript: prompt-cache hit ratio and warm/cold state, current and last-response effort.

### Modified Capabilities
- `session-state-indicators`: the attention state also covers pending `Notification` attention (`permission_prompt`, `agent_needs_input`). `agent_completed` never sets attention.
- `activity-timeline`: Activities records subagent lifecycle entries and background-session completion entries.
- `cc-adapter`: the setup flow registers `SubagentStart`, `SubagentStop` and the widened `Notification` matcher. The hook list in the spec is refreshed to the real set.

## Impact

- Rust: `src-tauri/src/setup.rs` (HOOKS table only), `src-tauri/src/hooks/{events,server,cli}.rs`, `src-tauri/src/agents/claude_code/cost.rs`. One-line `prompt_cache: None` edits where `HookEvent::AgentStatus` literals live: `agents/pi/jsonl_tail.rs`, `agents/codex/rollout_tail.rs`, `agents/opencode/sse_client.rs` (frozen agents, compile only).
- Frontend: `src/stores/{hooks,workspace,activity}.ts`, `src/lib/types.ts`, `src/components/layout/StatusBar.tsx`, `src/components/activity/{ActivityDrawer,ActivityLog}.tsx`, a new pure helper module with vitest tests.
- User settings: `nergal setup` upgrades the existing `Notification` entry matcher in place and adds two async hook entries. No DB schema change.
- Out of scope: nested subagent trees (no parent id in payloads), `subagentStatusLine` (Nergal does not install one).
