# Tasks — cluihud-mcp-server

> Implementation status (2026-06-15). Core session directory (phases 1-5, CC
> registration, frontend toggle), descriptor liveness (4.2b), AND the opt-in AI
> summarizer (phase 6 — config + keyring + two backends + debounced runner + UI
> + tests) **landed + verified green** (clippy -D warnings / 504 tests / fmt /
> tsc / vite build). Two slices remain **deferred** with rationale (flagged
> inline): the multi-agent registrars + `claude agents --json` enrichment
> (phase 4.4 / 7.1b — config formats need live verification). The manual
> two-session runtime walk (8.6) + a live LLM summary call require a running app.

## 1. Dedicated MCP transport (backend)

- [x] 1.1 `src-tauri/src/mcp/transport.rs`: framing (4-byte LE length + payload) over a **new** socket `/tmp/cluihud-mcp.sock`; hook socket untouched.
- [x] 1.2 Socket mode `0600`, stale-socket removal on bind (mirrors `server.rs:198`).
- [x] 1.3 `src-tauri/src/mcp/mod.rs`: JSON-RPC types, `initialize`/`tools/list`/`tools/call`, tool registry (daemon owns schemas via `tool_definitions`).
- [x] 1.4 `mcp_server_enabled` in `config.rs`, **default off**; daemon binds always, `tools/call` → `mcp_disabled` when off.

## 2. stdio shim (CLI)

- [x] 2.1 `cluihud mcp` subcommand: stdio relay to the dedicated socket.
- [x] 2.2 Degraded mode: local `initialize` + vendored `tools/list` (same `tool_definitions` source → no drift); structured error on `tools/call`.
- [x] 2.3 Fast, non-hanging connect; missing/dead socket degrades immediately.

## 3. Identity + uid boundary (daemon)

- [x] 3.1 uid wall: `0600` socket + `peer_cred().uid()` reject of other uids (the only access boundary). No `/proc` pid-walk.
- [x] 3.2 Cooperative identity: validate the reported `CLUIHUD_SESSION_ID` / CC side map against the live registry; unknown → unidentified.
- [x] 3.3 Lazy per-call re-resolution (connect-before-register race); side map torn down with `forget_session`.
- [x] 3.4 `list_sessions`/`get_session` global-read within the uid; cross-workspace exposure documented in the descriptor + Settings disclosure.

## 4. Descriptor assembly + directory tools

- [x] 4.1 Snapshot-then-release: brief DB lock → owned `Vec<Workspace>` → guard dropped before assembly; branch from the persisted column (no git subprocess on the read path).
- [x] 4.2 Descriptor (name, workspace, branch, agent, bg-tasks/crons).
- [x] 4.2b **LIVE (2026-06-15, commits `a3d4f4d`+`968002f`)**: `mode`/`last_activity`/`waiting_for`/`recently_touched_files:[{path,tool}]`/`last_assistant_message` now fed by a runtime activity side-map (`agents/state.rs`) mirroring the frontend `modeMapAtom`, written by `hooks/server.rs::process_event`. Root-cause fix — the DB `status`/`updated_at` columns only move on lifecycle mutations, so they read stale. `summary` stays null (reserved → section 6). Gotcha canon: PostToolUse matcher excludes `Read`, so files are captured from PreToolUse (unmatched). bg-tasks "skip empty" gate removed so an empty Stop clears a finished task.
- [x] 4.3 `whoami` / `list_sessions(include_self?)` (self-exclusion) / `get_session(id)` (not-found error).
- [x] 4.4 **SUPERSEDED 2026-06-21** (live shape verified): `claude agents --json` returns `[{pid, cwd, kind, startedAt, sessionId, status}]` — it does NOT carry `waiting_for`. That field is now populated by the runtime hook side-map (descriptor-liveness work via `HookEvent::waiting_for()`), correctly `None` when nothing is blocking, so the original "waiting_for null" premise is resolved by another path. The only thing `claude agents --json` adds is a coarse CC-only `status` (busy/idle) already covered by `is_live` + the finer `mode`. Building the cache would be redundant + CC-specific, so it is intentionally NOT implemented (scope discipline).

## 5. Background tasks / crons (additive)

- [x] 5.1 `HookEvent::Stop` extended with `background_tasks` + `session_crons` (`#[serde(default)]`, pass-through JSON). (SubagentStop is not modeled in cluihud today — additive when it is.)
- [x] 5.2 Captured into session state (`set_session_background`), surfaced in `get_session`; legacy-payload deserialize unit-tested.

## 6. Opt-in AI summaries — NEXT (key-free agent-CLI OR provider-agnostic key)

> **Design pivoted 2026-06-15 (user decision).** Backend is **user-selected,
> off by default, two mutually-exclusive switches** (never both, never auto):
> (A) **Agent CLI** headless (`claude -p`) on the user's **subscription — NO API
> key** (verified this session: headless `claude -p` authenticates via Max with
> no key); (B) **provider-agnostic API key** (OpenAI-compatible base URL + model
> + key), key in **OS keyring**, never plaintext. Both off → no summary, no
> error. Supersedes the old "dedicated Anthropic key only" spec. See
> `specs/session-summary/spec.md` (revised).

- [x] 6.2 Migration `021_session_summaries.sql` created + registered in `db.rs` (table ready; numbering corrected 014→021 after ClickUp migrations 015-020).
- [x] 6.1 Config in `config.rs`: `summary: SummaryConfig` with `backend: SummaryBackend` enum (`off | agent_cli | api_key`, default `off`) + `agent_command`/`api_base_url`/`api_model`/`disabled_projects`. Mutual exclusivity is **structural** (single enum value). Per-project opt-out via `effective_summary_backend(path)`. Key NOT in config.
- [x] 6.1b Keyring integration (`mcp/summary/secret.rs`, `keyring` crate) for `api_key` mode: `store_api_key`/`load_api_key`/`has_api_key`/`clear_api_key`; **no plaintext fallback** (diverges from `clickup/auth.rs` per the user's never-plaintext constraint). Tauri commands `set_summary_api_key`/`clear_summary_api_key`/`has_summary_api_key`.
- [x] 6.3 `mcp/summary/` module: `Summarizer` trait + `AgentCliBackend` (spawn `<binary> <args> <prompt>`, prompt incl. transcript as the verified `-p` arg, 120s timeout, `token_cost = None`) + `ApiKeyBackend` (POST OpenAI-compatible `/chat/completions`, parse `usage.total_tokens`). `summarize_transcript(backend, cfg, agent_cmd, path)` reads + tail-truncates to 48KB (line-aligned). Agent resolution honors the **default agent** in Settings via a per-adapter `AgentAdapter::headless_print_command()` returning binary + args + a `HeadlessOutput` strategy. **All four agents wired + verified key-free in vivo**: claude/pi (`-p`, clean stdout), codex (`exec --output-last-message <file>`), opencode (`run --format json`, JSONL text parts + token totals). An explicit per-summary command overrides it.
- [x] 6.4 Detached, debounced runner (`mcp/summary/runner.rs`): `maybe_spawn` on `Stop`, gated by `effective_summary_backend != Off` for the session's workspace repo; 60s debounce + single-flight guards; in-process `tauri::async_runtime::spawn` (summaries regenerate next Stop, so no detached *process* needed unlike MOCs); writes `session_summaries`; descriptor reads from the table. Read path never blocks.
- [x] 6.5 Single entrypoint (`summarize_transcript`) reusable by M4's post-session MOC summary.
- [x] 6.6 Settings → MCP `SummarySection.tsx`: two mutually-exclusive switches (Agent CLI / API key) + agent-command field (mode A) + base-URL/model/password fields with keyring store/clear (mode B) + subscription-quota note.
- [x] 6.7 Tests: config invariants (off-by-default, per-project opt-out, agent-command default, snake_case serde) + db round-trip/upsert + `summarize_transcript` Off-bails + api_key missing-config fails fast + transcript tail truncation. (Live LLM call verified manually, not in CI.) 504 tests green.

## 7. Registration + frontend

- [x] 7.1 (CC) Idempotent registration of `cluihud mcp` into `~/.claude.json` `mcpServers`, pinned `/usr/bin/cluihud`; best-effort disable-time deregistration; startup sync; pure helper unit-tested.
- [x] 7.1b **DONE 2026-06-21** (formats verified live + empirically parse-tested): **Codex** registrar (`~/.codex/config.toml` `[mcp_servers.cluihud]` command+args=["mcp"], via `toml_edit` to preserve formatting/comments; idempotent; `codex mcp list` confirmed "enabled") + **OpenCode** registrar (`~/.config/opencode/opencode.json` `mcp.cluihud` `{type:"local",command:[cmd,"mcp"],enabled:true}`, JSON; `opencode mcp list` confirmed "connected"). Both best-effort (a per-agent failure is logged, never blocks the others or the toggle). **Pi** intentionally NOT registered: its CLI has no MCP-server mechanism (no `pi mcp` subcommand; settings.json has no MCP key — it uses `pi install` extensions). 7 new pure-helper unit tests.
- [x] 7.2 (MCP toggle) Settings → MCP section: enable toggle (default off) + global-read disclosure. AI-summaries UI lands with phase 6.

## 8. Tests

- [x] 8.1 Transport framing: fragmented/partial, oversized, zero-length, short write, clean-EOF, partial-EOF (7 tests).
- [x] 8.2 JSON-RPC dispatch: initialize / tools/list / tools/call / unknown method / unknown tool / disabled path / invalid params (8 tests).
- [x] 8.3 Identity table: registered-id resolve / unknown → unidentified / CC side-map resolve / forget teardown; shim degraded-mode (3 tests).
- [x] 8.4 Descriptor assembly: empty directory + whoami-unidentified; registration add/idempotent/preserve/remove/missing/non-object (6 tests); Stop legacy + with-fields deserialize (2 tests).
- [x] 8.5 `cargo clippy -- -D warnings && cargo test && cargo fmt --check` + `npx tsc --noEmit` + `vite build` — all green.
- [x] 8.6 **WALK PASSED (live, user-verified 2026-06-19)**: two-session walk — cross-workspace `list_sessions`; `whoami` (CC + one non-CC); other-uid connection rejected; bg-tasks surfacing.

## 9. Revision 1 — lazy summary generation (pull) + historical read + FK cleanup

> **In-place revision (2026-06-16, design.md Revision 1).** Phase-6 generated a
> summary on every `Stop` past the debounce → speculative spend, contradicting
> the pull-not-push thesis. Invert to **lazy-on-read** (only `get_session`
> triggers), surface **recently-dead** sessions for the "yesterday's session"
> case, and add the **FK ON DELETE CASCADE** that `021` shipped without (orphan
> leak). See `specs/session-summary` + `specs/session-directory` (revised).

- [x] 9.1 Migration `022_session_transcripts.sql` (next free number): create `session_transcripts(session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE, transcript_path TEXT NOT NULL, last_stop_at INTEGER NOT NULL)`; **rebuild** `session_summaries` to add the FK. The rebuild MUST be **atomic + idempotent + orphan-safe** (iprev r1 #1/#2, r2 #3/#5): leading comment "022 manages its own transaction"; head `DROP TABLE IF EXISTS session_summaries_new;`; `BEGIN; … COMMIT;` wrap; copy with **explicit column names on both sides** (not `SELECT *` — position-mapping corruption risk) filtering orphans: `INSERT INTO session_summaries_new (session_id, summary, model, token_cost, updated_at) SELECT session_id, summary, model, token_cost, updated_at FROM session_summaries WHERE session_id IN (SELECT id FROM sessions)`. Then `DROP TABLE session_summaries; ALTER TABLE … RENAME`. Use the exact SQL in design.md R1.3 "Final shape". Register in `db.rs` migration list. `PRAGMA foreign_keys=ON` already on every connection (`db.rs:135,152,1212`); no `foreign_keys=OFF` dance (nothing references `session_summaries`).
- [x] 9.2 `db.rs` CRUD: `set_session_transcript(session_id, path, last_stop_at)` upsert; `get_session_transcript(session_id) -> Option<{path, last_stop_at}>`; **bulk `get_all_session_transcripts() -> HashMap<String, {path, last_stop_at}>`** (parallel to `get_all_session_summaries`, directory.rs:73) so `list_sessions` gets every row's `last_stop_at` in the owned snapshot without N+1 under the lock (iprev #4); a way to read `last_stop_at` alongside the summary `updated_at` for dirty detection. **Change `set_session_summary` to take the consumed `updated_at` timestamp as a parameter** (instead of computing `now_secs()` internally) so the runner can stamp it to the consumed `last_stop_at` (iprev r2 #1); update the two existing call-site unit tests (`db.rs:1280,1288`) to pass the timestamp explicitly so the phase-6 persistence guarantee keeps passing (iprev r3 #2). Round-trip unit tests + FK-cascade test (insert session+summary+transcript, `delete_session`, assert both companion rows gone) + **orphan-rebuild test** (pre-seed an orphan summary row, run migrations, assert 022 completes and the orphan is dropped, iprev #10) + **idempotency test** (pre-seed a leftover `session_summaries_new`, run migrations, assert 022 completes cleanly via `DROP IF EXISTS`, iprev r2 #4).
- [x] 9.3 `hooks/server.rs::process_event` (Stop branch, ~line 539): replace `summary::runner::maybe_spawn(...)` with a cheap `db.set_session_transcript(csid, transcript_path, now)` upsert. **No LLM on Stop.** The marker is written **unconditionally** (LLM-free, regardless of summary opt-in — iprev #9, not a contradiction with "No row when disabled" which constrains `session_summaries`). Keep feeding the runtime side-map (bg-tasks, last message) unchanged.
- [x] 9.4 `mcp/summary/runner.rs`: rework `maybe_spawn` → pull entrypoint callable from the read path (no longer called from Stop). **Gate ordering is fixed** (iprev r2 #2): (1) cheap debounce check on the `last_run` DashMap, bail without spawning if inside the window; (2) in-flight `insert()` — the sole correctness gate, bail if already present (iprev r1 #7); (3) stamp `last_run` at spawn time, before the await, so both the disabled-project (`dirty := no row` permanently true) and failing-transcript paths are rate-capped, not retried every read (iprev r1 #3 + r2 #2); (4) spawn detached; (5) **inside** the `async move`: `Config::load()`, resolve backend/project, re-read dirty — drop if Off or clean. This keeps the read path FS/lock-free (iprev r1 #5, snapshot-then-release). Add a **process-wide generation semaphore** (small fixed cap, e.g. 2) capping concurrency. Thread the already-read session/summary/project from `get_session` through instead of re-querying (iprev r1 #11). Capture the marker's `last_stop_at` at generation start and stamp `set_session_summary(..., consumed_last_stop_at)` (iprev r2 #1, closes the mid-generation-Stop gap). Step 5 **drops cleanly when no transcript marker resolves** (a markerless dirty session has nothing to summarize — bounded inert spawn, not an error path; iprev r3 #3). `dirty := no summary row OR last_stop_at > summary.updated_at`.
- [x] 9.5 `mcp/directory.rs`: relax the live-only gate in `get_session` (and `list_sessions`) to also surface **recently-dead** sessions (`last_stop_at` within a recency window — config knob, default 7d). The dead roster is enumerated from the `sessions` table (`get_workspaces`, db.rs:295) cross-referenced with the bulk transcript map; **a session with no `session_transcripts` row has no `last_stop_at` and is excluded** until its next Stop (iprev r2 #6 — acceptable degradation, not "all dead sessions within 7d appear"). Fold the bulk `get_all_session_transcripts()` into the existing owned snapshot (directory.rs:68-74) for inclusion + `summary_stale`. `get_session` triggers lazy generation when dirty (sole trigger), passing the already-read summary/project to the runner (iprev #11); `list_sessions` serves cache only, never generates. Add a recency-window helper.
- [x] 9.6 Descriptor: add `is_live: bool` and `summary_stale: bool` to `SessionDescriptor` (`directory.rs`), populate from liveness + (`last_stop_at` vs summary `updated_at`). Update `descriptor_from` signature + the two callers. JSON field names snake_case. **Document the dead-session field contract** (iprev #6): for a restarted dead session the activity side-maps are empty, so `mode` is the frozen DB value and `recently_touched_files`/`background_tasks`/`last_assistant_message` are empty — only `summary` + `git_branch` are meaningful. Note this in the descriptor doc-comment + the session-directory spec. Also note the summary **timestamp denotes "activity covered through" (the consumed `last_stop_at`), not generation wall-clock** (iprev r3 #1) so a consumer/UI doesn't misread it as production time.
- [x] 9.7 Recency window config: add the knob (e.g. `summary.history_window_days`, default 7) to `SummaryConfig` with `#[serde(default)]`; it is backend-owned (already in `BACKEND_OWNED_CONFIG_KEYS` family — verify no frontend clobber).
- [x] 9.8 Tests: dirty-detection truth table (no row → dirty / stale → dirty / fresh → clean); **consumed-timestamp** (Stop mid-generation keeps the session dirty → next read regenerates, iprev r2 #1); `list_sessions` never generates (cache-only) vs `get_session` triggers on dirty; recency-window inclusion/exclusion of dead sessions + markerless-session exclusion (iprev r2 #6); semaphore caps concurrency; debounce arms on the **failure** path and on the **disabled-project** path (no re-spawn on repeated reads, iprev r1 #3 + r2 #2); single-flight ordering (two concurrent triggers → one spawn, iprev r1 #7); write-after-delete → logged FK error, no crash (iprev #8); FK cascade + orphan-rebuild + idempotency-rerun (9.2). Re-run full check (`cargo clippy -- -D warnings && cargo test && cargo fmt --check && npx tsc --noEmit`).
- [x] 9.10 **Security review (security-auditor, 2026-06-16) → SHIP.** Same-uid boundary unbroken (dead-session surfacing adds data quantity, not a new principal). Three hardenings applied beyond the plan: (a) `gen_semaphore().acquire()` result handled (bail on closed-semaphore instead of silently exceeding the cap); (b) `Stop` rejects any `transcript_path` not ending in `.jsonl` before storing the marker — blocks a crafted same-uid hook payload from steering the summarizer at an arbitrary file (all four adapters write `.jsonl`, so no real transcript is rejected); (c) clock-skew comment on `within_window` (future `last_stop_at` saturates to 0 → benign, daemon-stamped never caller-supplied). Migration 022 confirmed injection-free + data-safe; no bare `unwrap`/`expect` in changed production code.
- [x] 9.9 **WALK PASSED (live, user-verified 2026-06-19)**: live walk — solo-session turn produces no LLM call on Stop (only marker upsert); `get_session` on a dead yesterday-session regenerates from disk; deleting a session drops its summary+transcript rows.
