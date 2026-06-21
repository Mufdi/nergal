## Context

cluihud is a long-lived Tauri process that already holds, in memory and SQLite, the state of every session it spawns: PTY handles, `CLUIHUD_SESSION_ID` per session, mode map (idle/active/tool), git metadata, activity stream, and transcript paths. The agent adapters (`agents/claude_code/adapter.rs:228`, `pi:168`, `opencode:209`, `codex:175`) inject `CLUIHUD_SESSION_ID` at spawn for every agent.

**Verified codebase facts that shape this design (checked 2026-06-08):**
- The existing hook Unix socket (`hooks/server.rs:202`) is **fire-and-forget**: the accept loop (`server.rs:205-261`) reads newline-delimited lines via `BufReader::lines()` and dispatches `process_event`; it has **no response/write path**. It cannot carry MCP request/response. → MCP needs a **new bidirectional transport**, not a message tag on this socket.
- There is **no LLM-invocation path anywhere in the backend**. `obsidian/post_session.rs` is marker/lock/detached-drain plumbing; `moc.rs` builds summaries by string concatenation. There is no model selection, auth-for-inference, token accounting, or transcript-read-for-prompt. → `session-summary` is **net-new LLM machinery**, not a reuse of existing code.
- `AgentRuntimeState` (`agents/state.rs:25`) is guarded by a blocking `std::sync::Mutex` and maps `cluihud_session_id -> AgentId` (`register_session`/`resolve`/`forget_session`).

CC injects `CLAUDE_CODE_SESSION_ID` + `CLAUDECODE=1` into stdio MCP server env (v2.1.154, also on `--resume` per v2.1.163). Codex, Pi, OpenCode support MCP server configuration.

## Goals / Non-Goals

**Goals:**
- A single daemon owning the live session directory across all workspaces.
- Agent-agnostic access via a thin stdio shim over a dedicated, authenticated transport.
- Cooperative env-hint identity within a uid wall — honest about being self-asserted, not adversarially authenticated.
- Cheap, always-fresh directory metadata with bounded lock scope (no reactor stalls).
- Opt-in AI summaries built honestly as net-new LLM machinery, off by default.
- Background tasks/crons (CC v2.1.150) folded into the descriptor additively.

**Non-Goals:**
- Cross-session *messaging* — `cross-session-messaging`.
- Agent-initiated *creation* of sessions — `agent-spawned-worktrees`.
- Per-caller authorization on directory reads (Decision 2b: the directory is global-read within the uid; identity is not an access gate here). The uid wall is the only enforced boundary.
- HTTP/SSE MCP transport (the dedicated socket is sufficient for v1; the daemon boundary leaves HTTP as a later option).

## Decisions

### 1. New bidirectional transport, NOT reuse of the hook socket

**Decision**: The MCP daemon exposes a **dedicated Unix socket** (`/tmp/cluihud-mcp.sock`, mode `0600`, in a per-user dir) speaking length-framed JSON-RPC with per-request response correlation. The existing hook socket is left untouched (it is fire-and-forget and cannot answer requests).

**Why**: MCP (`initialize`, `tools/list`, `tools/call`) requires request→response over a persistent bidirectional connection. `server.rs` has no write-back path and frames by newline, which can't carry multi-line JSON-RPC. Bolting responses onto it would be a larger, riskier change than a clean dedicated transport.

**Framing**: length-prefixed JSON (4-byte LE length + payload) to avoid newline-in-JSON ambiguity. The shim and daemon share one transport module.

**Transport trait** (multiplatform): `trait McpTransport { fn accept(); fn recv_framed(); fn send_framed(); }` with a `UnixSocketTransport` impl; a future `NamedPipeTransport` (Windows) drops in without touching dispatch.

### 2. Cooperative identity from the env hint; the uid is the only real boundary

**Decision**: Identity is the `CLUIHUD_SESSION_ID` (and `CLAUDE_CODE_SESSION_ID`) the shim reports, **validated against the daemon's live session registry** (the daemon knows which session ids are real, live sessions — it spawned them). It is treated as a **cooperative** identifier, not an adversarial authentication. The only enforced boundary is the **uid**: the socket is mode `0600` in a per-user directory, and the daemon additionally checks `peer_cred().uid()` and rejects other uids.

**Why the elaborate pid-walk was dropped** (round-2 finding): a `/proc` PPID-walk from the peer pid up to a session `child_pid` is **TOCTOU-unsound** (the peer pid is fixed at `connect(2)`, but `/proc/<pid>/stat` and the parent chain are read later and can change; pid recycling could even walk into a *different* session's `child_pid`) and it defends a threat that does not exist here: against a **same-uid** adversary there is no boundary to defend — that process can already `ptrace` the shim or read `/proc/<agent-pid>/environ` to lift `CLUIHUD_SESSION_ID` directly. So peer-cred-pid resolution buys **no confidentiality** over the env hint against the only adversary in scope, while adding racy code. The honest baseline is: cooperative env identity + a uid wall.

**What identity is for here**: `whoami`'s self-answer, and laying the groundwork for `cross-session-messaging` sender attribution (knowing which session a message claims to be from). It is **not** an access gate in this change — see Decision 2b.

**Race handling**: a shim that connects before its session is registered is `unidentified`; identity is re-validated lazily on each tool call, so it becomes identified as soon as the registry knows the id. Teardown on disconnect drops the connection→session binding and the `claude_code_session_id -> cluihud_session_id` side map (alongside `forget_session`, `state.rs:81`).

**Multiplatform**: the uid check + socket perms are unix; behind the identity abstraction so a Windows port supplies its own per-user restriction.

### 2b. The directory is intentionally global-read within the uid

**Decision**: `list_sessions` / `get_session` do **not** gate on caller identity. Any same-uid caller that reaches the socket reads every live session's descriptor (cross-workspace paths, branches, recently-touched files, and — when enabled — summaries). This is stated plainly rather than implied away.

**Why**: This is a single-user desktop app; the sessions all belong to one person. A same-uid access wall (Decision 2) is the real boundary; per-caller authorization inside it would be theater (the same user owns all sessions and the env that identifies them). The descriptor schema documents this cross-workspace exposure (`session-directory` spec). If a future multi-user or sandboxed scenario appears, read-gating on identity tier is the extension point — out of scope now.

**Lazy re-resolution**: identity is resolved **per tool call**, not once on connect, so a shim that connected before its session registered (race) becomes identified as soon as the daemon learns the mapping — no permanently-null connection.

**Lifecycle**: on socket disconnect (peer close) the daemon drops the connection→session binding. The `claude_code_session_id -> cluihud_session_id` side map is torn down alongside `forget_session` (`state.rs:81`). "Live session" = a session with an active PTY child known to the daemon; that set is the directory's liveness source.

### 3. Cheap directory with bounded lock scope; enrichment off the hot path

**Decision**: `list_sessions`/`get_session` assemble from data cluihud already holds, using a **snapshot-then-release** discipline: acquire the `AgentRuntimeState` mutex only to copy the cheap fields out, release it, then do any git/IO **outside** the lock. No blocking call (git, subprocess) runs while the mutex is held.

`claude agents --json` enrichment (CC `waitingFor`/`state`, v2.1.162/168) is **out-of-band**: refreshed on a timer / on hook events into a cache, never spawned synchronously on a directory read. The daemon's own state stays primary and is what a read returns; the cache is a non-blocking overlay.

**Why**: `AgentRuntimeState` is a blocking `std::sync::Mutex` inside async handlers; holding it across git/subprocess I/O would serialize all sessions and stall the tokio reactor. The spec's "always fresh" and "cheap hot path" only hold if the lock is released before slow work and subprocess spawns never sit on the read path.

### 4. Background tasks/crons — additive

**Decision**: Extend `HookEvent::Stop` / `SubagentStop` (`hooks/events.rs:27`) with `background_tasks: Vec<BackgroundTask>` and `session_crons: Vec<SessionCron>` (`#[serde(default)]`). Capture into session state; surface in `get_session`.

**Why**: CC v2.1.150 already sends these. Additive deserialization keeps older payloads valid (existing `hooks/` tests cover this).

### 5. Opt-in AI summary — net-new LLM machinery, honestly scoped

**Decision**: `session-summary` is **new** inference machinery, not an extraction from `post_session.rs`. It requires: a model-invocation path (cheap model, e.g. haiku), credential resolution via a **dedicated configured API key** (`config.rs` setting), token accounting, transcript read, a detached runner, and **SQLite persistence** (a migration in `src-tauri/migrations/`, not ad-hoc state). Off by default; global setting + per-project override. When disabled, nothing is read, invoked, stored, or sent.

**Credential decision** (round-2 finding): do **not** attempt to reuse the session's own agent auth. CC/Codex/Pi credentials live in their own config/keychains, are scoped to that agent's own usage, and lifting them to issue independent inference calls is fragile, conflates billing, and repeats the "reuse what doesn't cleanly exist" mistake. A separate configured key is the only clean path; if no key is configured, summaries stay off with a clear settings hint.

**Why**: The review verified no summarizer exists to reuse. Pretending otherwise hid real cost. M4's eventual MOC summary can later share this new entrypoint, but this change builds it from zero.

**Storage**: a `session_summaries(session_id PK, summary TEXT, model TEXT, token_cost INT, updated_at INT)` table via migration. Precedence: per-project-enabled gates whether a row is written at all. "Never on disk when disabled" = no row, no transcript read.

**Refresh**: debounce on `Stop` (avoid mid-turn), frequency cap, timestamped; a directory read never blocks on it (returns last row or null).

### 6. MCP protocol surface ownership

**Decision**: The **daemon owns the tool registry and JSON schemas**. The shim is a pure relay: `initialize` and `tools/list` are forwarded to the daemon. In daemon-unreachable (degraded) mode the shim answers `initialize` locally and returns a **static, vendored tool list** for `tools/list`, and a structured error for `tools/call`. **Both** the vendored tool list **and** the `initialize` capabilities/`protocolVersion` reported in degraded mode are generated from the same daemon-registry source at build time, so a degraded `initialize` cannot drift from what the daemon advertises.

**Why**: `tools/list` must return input/output schemas and `initialize` must report capabilities; without a single build-time source, the shim would either duplicate (drift) or be unable to answer when degraded.

### 7. Registration is idempotent and reversible

**Decision**: Registering `cluihud mcp` into agent configs (CC `mcpServers` in `~/.claude.json`; Codex/Pi/OpenCode equivalents) is idempotent (no duplicate entries on re-run). The registered command **pins the installed absolute path `/usr/bin/cluihud`** (the `.deb`/`.rpm` install location), explicitly **not** a `$PATH` resolution at registration time — `$PATH` would bake in the `~/.cargo/bin/cluihud` shadow that CLAUDE.md documents. Cleanup is **best-effort at disable time** (the app is running and can edit the configs). Uninstall-time deregistration is **not** attempted from maintainer scripts (those run as root, but the configs live in each user's `$HOME` — fragile and unspecified); an orphaned entry after uninstall degrades to a structured error when the agent tries to spawn the missing binary, not a hard agent failure.

**Why**: The project already hit binary-path fragility (cargo-install shadowing `/usr/bin/cluihud`, per CLAUDE.md). Pinning the install path avoids re-introducing the shadow; honest best-effort cleanup avoids promising an uninstall hook that can't be reliably implemented across multi-user `$HOME` layouts.

### 8. Default-off until the trust baseline is proven

**Decision**: `mcp_server_enabled` defaults to **off** for the initial release. It is opt-in until the directory's global-read posture (Decision 2b) is something the user knowingly turns on and the dependent changes exist.

**Why**: A partial-failure daemon still gets registered into agent configs; default-off is the safe rollback posture, and the directory exposes cross-workspace data to any same-uid caller (Decision 2b) so it should be an explicit opt-in.

## Risks / Trade-offs

**[Risk] New transport is more code than "reuse the socket"** → Accepted and re-estimated (see proposal `files_estimate`). It is the honest cost; the fire-and-forget hook socket genuinely cannot answer requests.

**[Risk] Transport framing bugs (partial reads, short writes, length corruption)** → The length-prefixed read-exact loop is the highest-risk new code. Enforced by explicit unit tests for fragmented frames, oversized length, and zero-length payloads (tasks §8.1). Unix socket perms (0600) + uid check behind the identity abstraction for a Windows port.

**[Risk] Mutex held across slow I/O stalls the reactor** → Snapshot-then-release (Decision 3) has two enforcement layers because no single check covers both hazard shapes: (a) holding the guard across an `.await` is caught automatically by **`clippy::await_holding_lock`** (in `cargo clippy -- -D warnings`); (b) holding the `std::sync::Mutex` across a **synchronous** blocking call (git via `std::process::Command` in `worktree.rs`, blocking `std::fs`) has **no `.await`**, so the lint cannot see it — this case is prevented structurally (the assembly function takes a snapshot of owned data and the guard is dropped before any git/fs/subprocess call) and verified in **code review**. The structural rule, not the lint, is the primary guarantee; the lint is a backstop for the async sub-case.

**[Risk] AI summary cost/privacy** → Off by default, per-project override, cheap model, dedicated configured key (no agent-auth reuse), SQLite row only when enabled, nothing read/invoked when disabled.

**[Trace] Tests** → JSON-RPC dispatch, transport framing (fragmented/oversized/zero-length), the identity-validation table (valid env id matching a live session / invalid id → unidentified / connect-before-register → lazy resolve), the disabled-daemon error path, and descriptor assembly are pure or near-pure and MUST have unit tests (not manual-only). See tasks §8.

## Revision 1: lazy summary generation (pull), historical read, FK cleanup (2026-06-16)

**What revealed the gap**: with phase 6 live (commits `7f32021`→`b19c956`), the `Stop`-triggered runner (Decision 5 §Refresh) generates a summary on **every** turn whose `Stop` clears the 60s debounce — i.e. one LLM call per normal turn, regardless of whether any observer ever reads it. That is speculative spend on the user's subscription quota and contradicts this change's own thesis that **MCP resolves the pull, not the push**. The original `session-summary` intent (opt-in AI recaps surfaced in the directory) is **unchanged**; only the *refresh trigger* and *storage cleanup* are corrected, so this is an in-place revision per the `config.yaml` Mid-implementation-revision rule, not a new change.

### Decision R1.1 — Generation is pull-based (on read), not push-based (on Stop)

**Decision**: `Stop` no longer invokes the summarizer. It performs a **cheap, LLM-free upsert** of `(transcript_path, last_stop_at)` into a durable marker table. Generation is triggered from the **read path**, and only by `get_session` (the intentional single-session drill-in) — never by `list_sessions`. A read serves the last persisted summary immediately (stale-while-revalidate); if the session is *dirty* (`last_stop_at > session_summaries.updated_at`, or no row) and no run is in flight, the read spawns generation detached and returns the current value (stale or null) without blocking.

**Dirty definition**: `dirty := no summary row OR last_stop_at > summary.updated_at`. This subsumes the "skip if transcript unchanged" guard — an unchanged session is never dirty.

**Single-flight ordering (iprev round 1, #7)**: two concurrent `get_session`s on the same dirty session both pass the outer dirty test, so correctness rests entirely on the runner's gate ordering. The runner SHALL **insert into the in-flight set first** (the authoritative `insert()`-returns-false gate, `runner.rs:81`), then re-read dirty *under that guard*, and bail if not dirty or already in flight. Outer dirty checks in the read path are an optimization only; the in-flight insert is the sole correctness gate.

**Debounce arms on spawn, not only on success (iprev round 1, critical #3)**: phase 6 stamped `last_run` only in the `Ok` arm (`runner.rs:106`). Under the old Stop trigger that was bounded by turn rate; under pull the trigger is caller-controlled, so a `get_session` poll on a dead session whose generation *always fails* (rotated/missing 7-day-old transcript) would re-spawn an LLM call on every read — single-flight only serializes, the semaphore only caps concurrency, neither caps **rate**. The debounce SHALL be stamped **at spawn time** (before the await), independent of success/failure, so a failing session is rate-limited identically to a succeeding one.

**Gate ordering pins the rate-cap for always-dirty sessions (iprev round 2, #2)**: a summaries-disabled project never gets a `session_summaries` row, so `dirty := no row` is permanently true for all its sessions; likewise a perpetually-failing transcript. If the debounce were checked only *after* the async prelude (`Config::load()` + locks), a `get_session` loop over such sessions would churn that prelude every read. The runner SHALL therefore order the gates: **(1) debounce check first** — a cheap `last_run` DashMap read; bail without spawning if inside the window — **(2) in-flight `insert()`** (the sole correctness gate, bail if already present), **(3) stamp `last_run`**, **(4) spawn detached**, **(5) inside the task: load config, resolve backend/project, re-read dirty; if Off or clean, drop**. Steps 1-3 are cheap and synchronous-but-lock-light (DashMap only, no `Config::load`, no DB lock); the expensive prelude lives in step 5 inside the spawn. This caps *both* the disabled-project path and the failing-transcript path to one attempt per debounce window.

**Consumed-timestamp stamping closes a mid-generation-Stop gap (iprev round 2, #1)**: phase 6 stamps `summary.updated_at = now` on completion. With `dirty := last_stop_at > updated_at`, a Stop that lands *while generation is running* is lost: Stop@T1 → read spawns gen → Stop@T2.5 (marker `last_stop_at=T2.5`) → gen finishes@T3 stamping `updated_at=T3`; now `T2.5 < T3` ⇒ not dirty, and if the session then dies the T2.5 turn is never summarized while `summary_stale=false` *lies*. Fix: the runner SHALL capture the marker's `last_stop_at` **at generation start** (the "consumed" value) and stamp `summary.updated_at` to **that consumed value**, not wall-clock `now`. Then a Stop arriving mid-generation advances `last_stop_at` beyond the consumed value ⇒ `last_stop_at > updated_at` stays true ⇒ still dirty ⇒ the next read regenerates and picks up the final turn. This requires `set_session_summary` to take the consumed timestamp instead of computing `now_secs()` internally.

**Why `get_session` only, never `list_sessions`**: `list_sessions` returns every session; triggering generation for each dirty one would re-create the push burst under another name (a directory poll → N LLM calls). `list_sessions` therefore serves **cache only**. Bounding generation to explicit single-session reads caps cost to "one LLM call per session a caller actually drills into, max once per debounce window."

**Alternatives considered**:
- *(a) Keep push, add a rate-cap (1 per N min / every K turns)*. Rejected: still speculative — generates for sessions no one reads. A band-aid on the wrong trigger.
- *(b) Generate only when a future `cross-session-messaging` send occurs*. Not an alternative but a **subset**: a `send_to_session` is just one more reader that trips the same lazy path. Folding it in now would couple this fix to an unbuilt change; the lazy mechanism already covers it for free.
- *(c) On-read synchronous generation (block the read until fresh)*. Rejected: violates the existing "directory read never blocks on summarization" requirement. SWR keeps reads cheap.

**Trade-off**: the first `get_session` after each turn returns a stale-or-null summary while regeneration runs in the background. Accepted — the entire premise of cross-session summaries is that reads are rare relative to turns, so the amortized cost collapses toward zero in a solo workflow (no cross-reads → no generation).

### Decision R1.2 — Historical read of recent dead sessions (read-contract widening)

**Decision**: `get_session` / `list_sessions` surface not only live sessions but also **recently-dead** ones (a session with a `last_stop_at` inside a recency window, default 7 days, configurable). The descriptor gains `is_live: bool` so a caller can tell a running session from a recalled one, and `summary_stale: bool` so it knows whether the served summary predates the latest activity. For a dead session, `get_session` can still trigger lazy generation because the transcript JSONL persists on disk and its path is the durable marker from R1.1.

**Why**: the highest-value summary case is *"I need context from a session I worked yesterday and is not running now"*. Under the live-only gate that session is invisible even though its transcript (and possibly its summary) exists. Pure-lazy makes this worse — a session that died unread never got summarized — so the durable transcript pointer (R1.3) is what lets a first read generate from disk on demand.

**Security note (why this rides the iprev + security review)**: widening the read surface stays **within the same uid trust boundary** (Decision 2 + 2b — the 0600 socket + uid check are unchanged; all sessions belong to one user). It does **not** cross a boundary, but it does enlarge the *data* a same-uid caller can pull (historical transcripts get summarized on read) and it makes a read carry a **side effect** (spawn an LLM). Amplification is capped structurally: per-session debounce + single-flight (carried over) + a **global generation semaphore** (max ~2 concurrent) + the `get_session`-only rule. A scripted `get_session` loop over N dead sessions is bounded by these, not unbounded.

**Degraded descriptor for restarted dead sessions (iprev round 1, #6)**: `descriptor_from` pulls `mode`/`last_activity`/`waiting_for`/`recently_touched_files`/`background_tasks`/`last_assistant_message` from the in-memory `AgentRuntimeState` side-maps, which are **empty** for a session not seen since daemon start (falls back to the frozen DB row, `directory.rs:142`). So a yesterday-session after an app restart returns `is_live:false`, a stale DB `mode`, empty activity fields, and only `summary` (regenerated from transcript) + `git_branch` (persisted column) carry meaning. This is acceptable degradation but SHALL be **documented** in the descriptor contract (session-directory spec) so callers don't read empty fields as "nothing happened."

**Marker is unconditional, summaries stay opt-in (iprev round 1, #9)**: the `session_transcripts` upsert on `Stop` is LLM-free and runs for **every** session regardless of the summary opt-in, so a transcript path is persisted even for projects with summaries disabled. This does **not** contradict the "No row when disabled" requirement (which constrains `session_summaries`, not the marker) — but it is called out so a reviewer doesn't read it as a contradiction. The marker is a cheap pointer (path + timestamp), not transcript content.

**Snapshot-then-release on the read path (iprev round 1, critical #5)**: the runner's synchronous prelude — `Config::load()` (a filesystem read, `runner.rs:63`) plus `db.lock()` for `find_session`/`workspace_repo_path` — must NOT run on the MCP read path before returning, or it violates the existing "Snapshot-then-release assembly (no reactor stalls)" requirement (Decision 3). The pull entrypoint SHALL return the cached summary first and do **all** FS/lock work (config load, project resolution, dirty re-check) **inside the detached `async move`**, not before the spawn. `list_sessions`'s `last_stop_at` inclusion + `summary_stale` data SHALL come from a **bulk** `get_all_session_transcripts()` read folded into the existing owned snapshot (parallel to `get_all_session_summaries()`, `directory.rs:73`), never an N+1 per-row query under the lock.

**Alternatives considered**:
- *Keep live-only (Scope A)*. Rejected by the user: leaves the yesterday-session case unsolved, which is the motivating scenario.
- *Surface all dead sessions, unbounded*. Rejected: a recency window keeps the directory legible and bounds how far back a read can reach to summarize.

### Decision R1.3 — Durable dirty marker + FK-cascade cleanup (corrects a phase-6 leak)

**Decision**: persist the marker in a new table `session_transcripts(session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE, transcript_path TEXT NOT NULL, last_stop_at INTEGER NOT NULL)` (migration 022). The marker must be **durable** (SQLite, not the in-memory `AgentRuntimeState` side-map) precisely so a dead session can still be located and summarized after the app restarts. The same migration **rebuilds `session_summaries` to add the FK it shipped without** — phase 6's `021` created `session_summaries(session_id PK, …)` with **no** `REFERENCES sessions(id)`, so `delete_session` (`DELETE FROM sessions`) orphans the summary row forever. SQLite has no `ALTER TABLE ADD CONSTRAINT`, so 022 recreates the table with the FK. `PRAGMA foreign_keys=ON` is already set on every connection (`db.rs:135,152,1212`), so cascade fires.

**Migration mechanics (iprev round 1, critical #1 + #2)** — the rebuild must survive the dev's own already-orphaned DB and a partial-failure re-run:

1. **Atomic + idempotent.** The migration runner (`db.rs:200`) calls `execute_batch(sql)` with **no enclosing transaction** and only bumps `schema_version` after `Ok`. A multi-statement rebuild that partially applies would leave `session_summaries_new` behind and re-running from `schema_version=21` would hit "table already exists" → permanent boot failure. So 022 SHALL: open with `DROP TABLE IF EXISTS session_summaries_new;` at the head, and wrap the rebuild in `BEGIN; … COMMIT;` (SQLite DDL is transactional). Renaming a child table under `foreign_keys=ON` is safe here because nothing references `session_summaries`, so no `PRAGMA foreign_keys=OFF` dance is needed — but atomicity is mandatory.
2. **Filter orphans in the copy.** The leak this revision fixes already produced orphan rows (every `delete_session` since `7f32021`). With immediate (non-deferred) FK enforcement, a blind `INSERT … SELECT *` of an orphan row → `FOREIGN KEY constraint failed` → the migration that fixes orphans chokes on them. The copy SHALL be `INSERT INTO session_summaries_new SELECT * FROM session_summaries WHERE session_id IN (SELECT id FROM sessions)` — orphans are dropped (harmless: summaries are regenerable under R1.1).

**Explicit columns in the copy, not `SELECT *` (iprev round 2, #3)**: a table rebuild that copies via `SELECT *` maps by column *position*; if the new `CREATE TABLE` ever lists columns in a different order than the old (easy when hand-adding the FK line), values land in the wrong columns with no error. The copy SHALL name columns on both sides.

**Self-managed transaction, commented (iprev round 2, #5)**: `execute_batch` runs with no enclosing transaction and SQLite does not auto-rollback a mid-batch statement error, so 022 manages its own `BEGIN…COMMIT`. A leading comment in the `.sql` SHALL state this, to guard against a future change wrapping the migration runner in its own transaction (which would make the embedded `BEGIN` fail with "cannot start a transaction within a transaction"). Recovery on partial failure relies on next-boot `DROP TABLE IF EXISTS session_summaries_new`.

Final shape:
```sql
-- 022 manages its own transaction (the migration runner uses execute_batch with no enclosing txn).
DROP TABLE IF EXISTS session_summaries_new;
BEGIN;
CREATE TABLE session_summaries_new(
  session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
  summary TEXT NOT NULL, model TEXT, token_cost INTEGER, updated_at INTEGER NOT NULL);
INSERT INTO session_summaries_new (session_id, summary, model, token_cost, updated_at)
  SELECT session_id, summary, model, token_cost, updated_at FROM session_summaries
  WHERE session_id IN (SELECT id FROM sessions);
DROP TABLE session_summaries;
ALTER TABLE session_summaries_new RENAME TO session_summaries;
COMMIT;
```

**Why FK over a manual `DELETE`**: the codebase's established pattern is `REFERENCES … ON DELETE CASCADE` (annotations `003`, obsidian `008`, clickup `015`). A declarative FK cleans up on **both** delete paths for free — direct `delete_session` and workspace deletion cascading `workspaces → sessions → {summaries, transcripts}`. An imperative `DELETE FROM session_summaries WHERE session_id=?` inside `delete_session` would be a patch that (a) diverges from the pattern and (b) misses the workspace-cascade path. Root-cause over patch.

**Write-after-delete (iprev round 1, #8)**: under R1.2 a generation can be in flight for a recently-dead session the user then deletes; the spawned `set_session_summary` then hits the FK and errors. This is **intended** — it is caught and logged best-effort (`runner.rs:102`), converting the old *silent orphan* into a *silent logged FK error*, no crash. Covered by a test (task 9.8).

**Alternatives considered**:
- *Add columns to `sessions` instead of a new table*. Rejected: `last_stop_at`/`transcript_path` are summary-subsystem state, not core session identity; a dedicated table keeps the concern isolated and its FK self-documenting. (Reversible if it later proves chatty.)
- *Keep the marker in the runtime side-map only*. Rejected: dies on app restart, so a dead session could never be summarized after a restart — defeats R1.2.
