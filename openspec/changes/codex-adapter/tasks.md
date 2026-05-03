## 0. Pre-requisite

- [ ] 0.1 `agent-adapter-foundation` change SHALL be merged.
- [ ] 0.2 Recommendation: `opencode-adapter` and `pi-adapter` merged before this — Codex landing last validates the trait against the most non-CC variant first (defensive ordering).

## 1. Spike: Codex hooks + rollout schema

- [ ] 1.1 Install Codex (`npm install -g @openai/codex` or brew); run `codex login`
- [ ] 1.2 Write a minimal `~/.codex/hooks.json` with each hook event echoing stdin to a log file; run a small Codex session
- [ ] 1.3 Capture stdin payloads per event in `tests/fixtures/codex/hook-payloads.jsonl`
- [ ] 1.4 Locate the rollout `.jsonl` in `~/.codex/sessions/YYYY/MM/DD/`; copy to `tests/fixtures/codex/rollout.jsonl`
- [ ] 1.5 Document observed schemas in `docs/agents/codex-schema.md`: hook payloads + rollout entry types
- [ ] 1.6 Specifically check: (a) cost/usage field presence in rollout, (b) task-list-shaped events, (c) PermissionRequest payload shape (does it include a free-form question or only allow/deny?)
- [ ] 1.7 Decide capability flag declarations based on findings; update `design.md` and the spec accordingly

## 2. Backend module scaffold

- [ ] 2.1 Create `src-tauri/src/agents/codex/mod.rs` with `CodexAdapter` struct implementing the trait
- [ ] 2.2 No new external deps
- [ ] 2.3 Register `CodexAdapter` in `AgentRegistry::default_registrations()`

## 3. setup_agent('codex')

- [ ] 3.1 Create `src-tauri/src/agents/codex/setup.rs` with `run_codex_setup() -> Result<()>`
- [ ] 3.2 Build the hooks JSON per design.md
- [ ] 3.3 Read existing `~/.codex/hooks.json` if present; merge cluihud entries with non-cluihud ones (same conservative merge as CC's setup.rs)
- [ ] 3.4 Cleanup obsolete cluihud hook entries from prior versions (analog of OBSOLETE_HOOKS in CC's setup.rs)
- [ ] 3.5 Write back atomically (write to temp + rename)
- [ ] 3.6 Wire to the foundation's `setup_agent(agent_id)` Tauri command: `agent_id == 'codex'` dispatches here

## 4. Hook subcommand `--agent codex` routing

- [ ] 4.1 Verify foundation's `cluihud hook send|inject-edits|ask-user|plan-review --agent <id>` correctly routes to the adapter; if not, add routing logic in `hooks/cli.rs`
- [ ] 4.2 The Codex adapter receives the parsed payload and applies its own schema interpretation (since CC and Codex schemas are similar but not identical)

## 5. Adapter trait implementation

- [ ] 5.1 `id()`, `display_name()`, `capabilities()`, `transport()` per design.md
- [ ] 5.2 `detect()`: check `~/.codex/`, `which codex`, `codex --version`
- [ ] 5.3 `spawn()`: returns `SpawnSpec { binary: codex, args: maybe ["resume", uuid], env: { CLUIHUD_SESSION_ID } }`
- [ ] 5.4 `start_event_pump()`: no-op (events arrive via shared Unix socket from `cluihud hook` subcommands)
- [ ] 5.5 `submit_plan_decision()`: returns `Err(NotSupported(PLAN_REVIEW))`
- [ ] 5.6 `submit_ask_answer()`: writes to FIFO at `/tmp/cluihud-ask-<pid>.fifo` (same mechanism as CC)
- [ ] 5.7 `parse_transcript_line()`: implements the rollout schema parser per the docs from task 1.5

## 6. Rollout JSONL transcript parsing

- [ ] 6.1 Create `agents/codex/transcript.rs` with `parse_transcript_line(line: &str) -> Option<TranscriptEvent>`
- [ ] 6.2 Handle observed entry types from spike (message, tool_call, tool_result, etc.)
- [ ] 6.3 If cost field is present: emit `Cost(RawCost { ... })` with appropriate field mapping (OpenAI uses `prompt_tokens`/`completion_tokens` typically; map to input/output)
- [ ] 6.4 Test against `tests/fixtures/codex/rollout.jsonl`

## 7. PTY layer integration

- [ ] 7.1 No changes — Codex uses the existing PTY layer like CC and Pi
- [ ] 7.2 Verify `CLUIHUD_SESSION_ID` propagates to the Codex child via SpawnSpec.env

## 8. Session UUID persistence for resume

- [ ] 8.1 At session start, the rollout filename includes the UUID: `~/.codex/sessions/YYYY/MM/DD/rollout-<uuid>.jsonl`
- [ ] 8.2 Adapter watches `~/.codex/sessions/` for new rollout files post-spawn; matches the file whose mtime is closest to spawn time
- [ ] 8.3 Persists `<uuid>` in the cluihud session row's `agent_internal_session_id` (column already added by `pi-adapter`)
- [ ] 8.4 On resume: `spawn(ctx)` returns `SpawnSpec { args: ["resume", uuid], ... }`

## 9. Trust-gate awareness

- [ ] 9.1 In `detect()` output, include flag `trusted_for_project: bool` based on whether Codex's trust marker (heuristic from spike, e.g., `<project>/.codex/.trusted` or `~/.codex/trust.json`) indicates trust
- [ ] 9.2 Frontend shows a yellow banner in the Codex session row if not trusted: "Codex requires trust for this project — run `codex trust` from a terminal"

## 10. Auto-detection wiring

- [ ] 10.1 `CodexAdapter::detect()` runs as part of `AgentRegistry::scan()` at startup
- [ ] 10.2 Settings → "Rescan agents" picks up Codex if newly installed

## 11. Settings panel for Codex

- [ ] 11.1 In `AgentsSettings.tsx`, add a Codex section: install status, version, "Run setup" button (invokes `setup_agent('codex')`), trust status indicator
- [ ] 11.2 If not trusted for the active project, show "Trust this project" instructions (no auto-action)

## 12. Tests

- [ ] 12.1 Unit test: `parse_transcript_line` against `tests/fixtures/codex/rollout.jsonl`
- [ ] 12.2 Integration test: `setup_agent('codex')` writes correct hooks.json; merging behavior preserved on existing files
- [ ] 12.3 Integration test: end-to-end PermissionRequest flow — Codex hook fires `cluihud hook ask-user`, FIFO blocks, frontend submits answer, FIFO unblocks, Codex receives decision
- [ ] 12.4 Manual test: real Codex session — spawn, observe TUI, accept a permission via UI, verify Codex proceeds; resume the session via cluihud and verify `codex resume <uuid>` is invoked

## 13. Documentation

- [ ] 13.1 `docs/agents/codex.md` with: install, setup, capabilities (no plan mode), trust-gate handling, troubleshooting
- [ ] 13.2 Update `CLAUDE.md` with Codex adapter notes
- [ ] 13.3 Final agent-agnostic milestone announcement: README/CHANGELOG entry confirming all 4 agents (CC, OpenCode, Pi, Codex) supported
