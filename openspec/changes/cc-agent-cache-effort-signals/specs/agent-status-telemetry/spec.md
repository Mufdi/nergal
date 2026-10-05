## ADDED Requirements

### Requirement: Statusline feeder forwards the prompt cache object

The `nergal hook statusline` feeder SHALL map CC's statusline `prompt_cache` object into the `AgentStatus` socket payload as a nested `prompt_cache` object with these optional fields: `warm` (bool), `caching_observed` (bool), `ttl` (string), `expires_at` (u64 epoch seconds), `requests` (u64), `misses` (u64), `hit_ratio` (f64, 0..1), `miss_recache_tokens` (u64), `last_miss_at` (u64), `last_miss_causes` (string list from `last_miss_cause.causes`). Every key SHALL always be present in the emitted object; a missing or mistyped source field SHALL be `null` (`last_miss_causes` SHALL be `[]`). A missing or non-object `prompt_cache` SHALL map to `null` without failing the payload.

#### Scenario: Full prompt cache object maps field by field
- **WHEN** the statusline stdin carries `"prompt_cache": {"warm": true, "caching_observed": true, "ttl": "1h", "expires_at": 1738429200, "requests": 14, "misses": 2, "hit_ratio": 0.91, "miss_recache_tokens": 310200, "last_miss_at": 1738425230, "last_miss_cause": {"causes": ["tools_changed"], "tools_added": 2}}`
- **THEN** the payload `prompt_cache` SHALL equal `{"warm": true, "caching_observed": true, "ttl": "1h", "expires_at": 1738429200, "requests": 14, "misses": 2, "hit_ratio": 0.91, "miss_recache_tokens": 310200, "last_miss_at": 1738425230, "last_miss_causes": ["tools_changed"]}`

#### Scenario: Older CC without prompt_cache
- **WHEN** the statusline stdin has no `prompt_cache` key (CC < v2.1.251)
- **THEN** the payload `prompt_cache` SHALL be `null`
- **AND** every other `AgentStatus` field SHALL map as before

#### Scenario: Null last miss cause and null hit ratio
- **WHEN** `prompt_cache` is `{"warm": false, "hit_ratio": null, "last_miss_cause": null}`
- **THEN** the payload `prompt_cache` SHALL be `{"warm": false, "caching_observed": null, "ttl": null, "expires_at": null, "requests": null, "misses": null, "hit_ratio": null, "miss_recache_tokens": null, "last_miss_at": null, "last_miss_causes": []}`

### Requirement: Status bar shows the prompt cache chip

The status bar SHALL render a prompt-cache chip for the active session when `prompt_cache.hit_ratio` is a number and `prompt_cache.caching_observed` is not `false`. The chip SHALL show the hit ratio as a rounded percentage. It SHALL append the word `cold` when `warm` is `false`. The tooltip SHALL show warm/cold state, TTL, requests, misses, tokens re-cached by misses, and the last miss cause in human words when present. Cause wording: `tools_changed` → `tool definitions changed`, `system_prompt_changed` → `system prompt changed`, `ttl_expired_5m` → `idle past the 5m TTL`, `ttl_expired_1h` → `idle past the 1h TTL`, `likely_server_side` → `likely server-side eviction`, any other id → the id with `_` replaced by spaces. Several causes SHALL join with `, `.

#### Scenario: Warm cache chip
- **WHEN** the active session status has `prompt_cache = {hit_ratio: 0.91, warm: true, caching_observed: true}`
- **THEN** the chip text SHALL be `91%` with no `cold` marker

#### Scenario: Cold cache chip with miss cause
- **WHEN** `prompt_cache = {hit_ratio: 0.42, warm: false, misses: 3, last_miss_causes: ["ttl_expired_5m"]}`
- **THEN** the chip text SHALL be `42% cold`
- **AND** the tooltip SHALL include `Last miss: idle past the 5m TTL`

#### Scenario: Caching not observed hides the chip
- **WHEN** `prompt_cache = {caching_observed: false, hit_ratio: null}`
- **THEN** no prompt-cache chip SHALL render

#### Scenario: Unknown miss cause falls back to the raw name
- **WHEN** `last_miss_causes = ["some_new_cause"]`
- **THEN** the tooltip SHALL show `Last miss: some new cause`

#### Scenario: Several miss causes
- **WHEN** `last_miss_causes = ["tools_changed", "system_prompt_changed"]`
- **THEN** the tooltip SHALL show `Last miss: tool definitions changed, system prompt changed`

### Requirement: Last-response effort from the transcript

On a `Stop` hook with a `transcript_path`, the backend SHALL read the top-level `effort` string of the last `type: "assistant"` record in that transcript. It SHALL emit `effort:last-response` with `{session_id, effort}` when a value exists. Records without `effort` (CC < v2.1.212) SHALL be skipped. No emit SHALL happen when no assistant record carries `effort`.

#### Scenario: Last assistant record wins
- **WHEN** the transcript holds assistant records with `effort` `"high"` then `"medium"`, followed by a user record
- **THEN** the extracted effort SHALL be `"medium"`

#### Scenario: Older transcript without effort
- **WHEN** no assistant record in the transcript has an `effort` key
- **THEN** the extracted effort SHALL be `None` and no `effort:last-response` event SHALL be emitted

#### Scenario: Malformed lines are skipped
- **WHEN** the transcript contains a non-JSON line after the last valid assistant record with `effort: "low"`
- **THEN** the extracted effort SHALL be `"low"`

### Requirement: Status bar effort chip shows the last-response effort

The effort chip next to the model SHALL show the current effort setting (`effort_level` from the statusline). When the last-response effort for the session differs from that setting, the chip SHALL append `ran <effort>`. When no setting is known, the chip SHALL show the last-response effort alone. The tooltip SHALL read `Setting: <a>` and `Last response: <b>` for each known value. The last-response effort SHALL clear when the user submits a prompt in that session.

#### Scenario: Same effort shows one value
- **WHEN** `effort_level = "high"` and last-response effort is `"high"`
- **THEN** the chip text SHALL be `high`

#### Scenario: Different effort shows both
- **WHEN** `effort_level = "high"` and last-response effort is `"medium"`
- **THEN** the chip text SHALL be `high ran medium`

#### Scenario: Prompt submit clears the last-response effort
- **WHEN** `effort_level = "low"`, last-response effort is `"high"`, and the user submits a prompt
- **THEN** the chip text SHALL be `low` until the next `effort:last-response`

#### Scenario: Only the transcript value is known
- **WHEN** `effort_level` is `null` and last-response effort is `"xhigh"`
- **THEN** the chip text SHALL be `xhigh`

### Requirement: Subagent transcript summary on stop

On `SubagentStop` with an `agent_transcript_path`, the backend SHALL read at most the last 256 KiB of that transcript and forward on the `hook:event` frame `agent_tokens` (input + cache creation + cache read tokens of the latest assistant record's `message.usage`) and, when the payload `last_assistant_message` is empty or absent, `agent_message` as the text of the latest assistant record that has any. A missing, unreadable, empty or partially written transcript, and malformed lines, SHALL leave both fields absent without failing the event.

#### Scenario: Stopped agent reports from its transcript
- **WHEN** `SubagentStop {last_assistant_message: "", agent_transcript_path: P}` arrives and the last assistant record of `P` has text `final report` and usage `{input_tokens: 2, cache_creation_input_tokens: 238, cache_read_input_tokens: 49509}`
- **THEN** the frame SHALL carry `agent_message: "final report"` and `agent_tokens: 49749`

#### Scenario: Payload message wins
- **WHEN** the payload `last_assistant_message` is `from payload` and the transcript has other text
- **THEN** `agent_message` SHALL be `from payload` and `agent_tokens` SHALL still come from the transcript

#### Scenario: Transcript unavailable
- **WHEN** `agent_transcript_path` points to a missing file
- **THEN** the frame SHALL omit `agent_tokens` and `agent_message`
