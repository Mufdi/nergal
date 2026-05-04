## 0. Pre-requisite

- [ ] 0.1 `agent-adapter-foundation` change SHALL be merged.
- [ ] 0.2 Recommendation (not a hard block): `opencode-adapter` merged before this — confirms the `Transport` enum design holds for non-PTY transports.

## 1. Spike: capture real Pi JSONL session

- [ ] 1.1 Install Pi locally (`npm install -g @mariozechner/pi-coding-agent`); configure credentials per Pi docs
- [ ] 1.2 Run a small Pi session in a test cwd; copy the resulting `.jsonl` file to `tests/fixtures/pi/session.jsonl`
- [ ] 1.3 Document each entry type observed in `docs/agents/pi-jsonl-schema.md` with one example each (`session`, `agent` of each role, `tool_call`, `tool_result`, `model_change`, `compaction`, etc.)
- [ ] 1.4 Specifically capture the `usage` field shape on `agent` entries (where the cost lives)
- [ ] 1.5 Confirm path encoding for `~/.pi/agent/sessions/` (slashes → dashes per docs)

## 2. Backend module scaffold

- [ ] 2.1 Create `src-tauri/src/agents/pi/mod.rs` with `PiAdapter` struct implementing the trait
- [ ] 2.2 No new external deps — reuse `notify` (already used by transcript watcher) and `serde_json`
- [ ] 2.3 Register `PiAdapter` in `AgentRegistry::default_registrations()`

## 3. Session resolver

- [ ] 3.1 Create `agents/pi/session_resolver.rs`
- [ ] 3.2 Function `encode_cwd_to_pi_path(cwd: &Path) -> String` matching Pi's slashes→dashes convention; test with multiple cwds
- [ ] 3.3 Function `wait_for_jsonl(sessions_dir, timeout) -> Result<PathBuf>` polling for newest `.jsonl` (100ms × 20 default)
- [ ] 3.4 Function `extract_pi_session_uuid(jsonl_path) -> Option<String>` reading the first line and parsing the `session` header

## 4. JSONL tail watcher

- [ ] 4.1 Create `agents/pi/jsonl_tail.rs` with `JsonlTail::open(path)` and `run(sink, parser)` methods
- [ ] 4.2 On open: read existing content fully, emit events for each line via `parser`
- [ ] 4.3 Use `notify::RecommendedWatcher` for modify events; on each modify, read from last offset to EOF, emit new lines
- [ ] 4.4 Track `last_offset: u64` in struct; persist nothing (idempotent)
- [ ] 4.5 Handle file rotation / replacement gracefully (Pi may not rotate, but defensive)
- [ ] 4.6 Unit test: write JSONL fixture line-by-line into a temp file with delays, assert tail emits each line as a parsed `TranscriptEvent`

## 5. Transcript parser

- [ ] 5.1 Create `agents/pi/transcript.rs` with `parse_transcript_line(line: &str) -> Option<TranscriptEvent>`
- [ ] 5.2 Define internal `PiEntry` enum mirroring the schema doc from task 1.3, with `#[serde(tag = "type")]`
- [ ] 5.3 Map each PiEntry variant to TranscriptEvent per the table in design.md
- [ ] 5.4 For `agent` with `usage` and role `assistant`: emit `Cost(RawCost { ... })` ALONGSIDE the `Message` variant (one line can produce two events)
- [ ] 5.5 Discard `cost.usd` field; we only emit token counts (per foundation D6)
- [ ] 5.6 Test parser against `tests/fixtures/pi/session.jsonl`: assert correct event counts per type

## 6. Adapter trait implementation

- [ ] 6.1 `id()`, `display_name()`, `capabilities()`, `transport()` per design.md
- [ ] 6.2 `detect()`: check `~/.pi/agent/` and `which pi`; populate version (via `pi --version` if cheap)
- [ ] 6.3 `spawn()`: returns `SpawnSpec { binary: pi_path, args: maybe ["--resume", uuid], env: { CLUIHUD_SESSION_ID } }`
- [ ] 6.4 `start_event_pump()`: orchestrate session_resolver + jsonl_tail per design.md
- [ ] 6.5 `submit_plan_decision()`: returns `Err(NotSupported(PLAN_REVIEW))`
- [ ] 6.6 `submit_ask_answer()`: returns `Err(NotSupported(ASK_USER_BLOCKING))`
- [ ] 6.7 `parse_transcript_line()`: delegates to `transcript::parse_transcript_line`

## 7. PTY layer integration

- [ ] 7.1 No changes — Pi uses the existing PTY layer like CC. The PTY spawn calls `adapter.spawn(ctx)` which returns the proper `SpawnSpec`.
- [ ] 7.2 Verify `CLUIHUD_SESSION_ID` propagation: env var injection via SpawnSpec.env reaches the Pi child process

## 8. Session UUID persistence for resume

- [ ] 8.1 Add column `agent_internal_session_id TEXT` to `sessions` table (general-purpose, used by Pi to store the Pi UUID)
- [ ] 8.2 At first JSONL line read, extract Pi's UUID from the `session` entry header and persist via `db.update_agent_internal_session_id(cluihud_session_id, pi_uuid)`
- [ ] 8.3 On resume, `spawn(ctx)` reads `agent_internal_session_id` from the session row and passes as `--resume <uuid>`
- [ ] 8.4 Test: create Pi session, kill cluihud, restart, resume the session — verify `pi --resume <uuid>` is invoked

## 9. Auto-detection wiring

- [ ] 9.1 `PiAdapter::detect()` runs as part of `AgentRegistry::scan()` at startup
- [ ] 9.2 Settings → "Rescan agents" picks up Pi if newly installed

## 10. Settings panel for Pi

- [ ] 10.1 In `AgentsSettings.tsx`, add a Pi section: install status, version, link to Pi docs, instructions to configure credentials per Pi's own flow
- [ ] 10.2 No setup button (Pi has no hooks); banner clarifies "Pi has no plan mode or ask-user blocking — read-only observation in cluihud"

## 11. Tests

- [ ] 11.1 Unit test: `encode_cwd_to_pi_path` for several cwds (root, with spaces, deep paths)
- [ ] 11.2 Unit test: `parse_transcript_line` against fixture JSONL — verify event counts and shapes
- [ ] 11.3 Integration test: temp dir simulating Pi's sessions dir, write fixtures with delays, JsonlTail emits events into sink
- [ ] 11.4 Integration test: full lifecycle — adapter `start_event_pump` → JSONL appears → events flow → activity drawer populates (mock the frontend with an event log)
- [ ] 11.5 Manual test: real Pi session, observe TUI in terminal, verify activity drawer fills, status bar shows token totals

## 12. Documentation

- [ ] 12.1 `docs/agents/pi.md`: install, capabilities (read-only), no plan/no ask, TUI nativo + paneles read-only, troubleshooting
- [ ] 12.2 Update `CLAUDE.md` with Pi adapter notes
