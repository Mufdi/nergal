# Implementation Plan: osc7501-session-status

> Grounded in the codebase at main `ab472c2` (branch `mufdidev/fri-623-osc7501-session-status`), symbols verified 2026-11-26. Claude Code facts: `~/.claude/projects/-home-felipe-Projects-cluihud/work/osc7501/research.md`. Nergal map: `nergal-side.md` in the same folder.

## Verified codebase facts (do not re-assume)

- `TerminalSession` struct at `src-tauri/src/terminal/session.rs:92`; fields include `sync_tail: Vec<u8>` (`:113`) and `image_registry: ImageRegistry` (`:114`); constructed in `with_config_and_pixels` (`:152-189`), which passes `writer` to `Terminal::new(size, config, "nergal", CARGO_PKG_VERSION, writer)` (`:169-175`). wezterm owns that writer; `TerminalSession` keeps no handle to it.
- `track_sync_mode` (`session.rs:206`) carries a 7-byte tail between calls and is the pattern to follow; `advance_bytes` (`session.rs:235`) calls `track_sync_mode(bytes)` then `self.terminal.advance_bytes(bytes)`. Return type is `()` today.
- Inline tests module `session.rs:675`; `CapturedWriter(Arc<Mutex<Vec<u8>>>)` at `:681`; a helper returning `(TerminalSession, CapturedWriter)` at `:725`; a helper at `:996` returns the bytes the emulator answers (`answer_to`); DA1 answer test near `:1037` expects `ESC[?65;4;6;18;22c`.
- `type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>` (`src-tauri/src/pty.rs:16`); `SharedWriterAdapter(SharedWriter)` (`pty.rs:20`) writes with `write_all` under one lock.
- `build_terminal_session(cols, rows, pw, ph, writer)` (`pty.rs:144`) is the single construction point; called by `spawn_pty` (writer built at `pty.rs:790-797`) and by a test helper near `pty.rs:2856`.
- Reader thread: `pty.rs:810-830` reads 8 KiB chunks, locks `reader_session`, calls `guard.advance_bytes(&buf[..n])` (`:824`), drops the guard at end of the `match`, then `reader_notify.notify_one()`. It has `agent_session`, `session_id` (as `eof_session`) and `eof_app: AppHandle` in scope (`pty.rs:806-808`).
- EOF path: `pty.rs:834-847`; for agent sessions it calls `on_agent_pty_eof(agents.inner(), &eof_session, &eof_pty_id)` (`:846`), defined at `pty.rs:970` (calls `agents.mark_dead_if`). Agent state is reached with `eof_app.try_state::<crate::agents::state::AgentRuntimeState>()`.
- `AgentRuntimeState` (`src-tauri/src/agents/state.rs`): `session_activity: Arc<DashMap<String, SessionActivity>>` (`:93`, initialized `:130`); `record_activity` (`:222`) overwrites mode/waiting_for/last_activity; `session_activity()` (`:240`) returns a clone. `SessionActivity { mode, last_activity, waiting_for }` at `:43-50`.
- MCP descriptor: `src-tauri/src/mcp/directory.rs:245` picks `(mode, last_activity, waiting_for)` from `agents.session_activity(&s.id)` when live, else the DB row.
- Message delivery reads `session_activity` directly, not the descriptor: `src-tauri/src/mcp/messaging.rs:192-200` computes `target_busy` (`running`/`needs_attention`) and `just_idled` (`idle` within `IDLE_SETTLE_SECS` of `last_activity`); the deferred wake re-checks busy at `:221-224` and calls `delivery::drain_idle(&db, delivery.as_ref(), &to, enabled)`. `drain_idle` (`src-tauri/src/mcp/delivery.rs:203`) skips a session whose previous wake is in flight. The Stop drain is at `src-tauri/src/hooks/server.rs:1103-1119` (`AppBridge::new(app.clone())`, `Config::load().cross_session.enabled`).
- wezterm wraps the writer passed to `Terminal::new` in `ThreadedWriter` (channel + own thread, `~/.cargo/git/checkouts/wezterm-26cdb9e734a97642/5046fc2/term/src/terminalstate/mod.rs:448-478`), so its answers never block the reader thread and arrive asynchronously.
- Test helper `CapturedWriter::drain` (`session.rs:696-708`) returns as soon as one byte is present; `session_with_writer` (`session.rs:725-729`) builds with `TerminalSession::new` (no reply sink).
- Pending tint: `isAwaiting` at `src/components/layout/TopBar.tsx:476` and `src/components/session/SessionRow.tsx:64` (`!!pendingAsks[id] || !!pendingAttention[id]`), class `nergal-ask-pending` (`src/styles/globals.css:886`).
- Session-keyed atoms MUST use `sessionScopedMapAtom` (`src/stores/sessionScope.ts:3-7`), e.g. `src/stores/activity.ts:16`.
- Hook server writes activity at `src-tauri/src/hooks/server.rs:1057-1066` (unchanged by this change).
- Frontend: listeners are registered in one async setup function in `src/stores/hooks.ts:81` returning `UnlistenFn[]` (pattern: `unlisteners.push(await listen<Payload>("event", (payload) => {...}))`, e.g. `attention:pending` near `:398-412`). `listen` comes from `@/lib/tauri`.
- `src/components/session/SessionIndicator.tsx`: `IndicatorState` union (`:7`), `STATE_COLORS`/`STATE_ANIMATIONS` maps (`:15-29`), component (`:37`), private `resolveState` (`:57`). Atoms: `modeMapAtom` (`src/stores/workspace.ts:160`), `planReviewStatusMapAtom` (`src/stores/plan`), `pendingAsksAtom`/`pendingAttentionAtom` (`src/stores/askUser.ts`).
- Frontend tests: vitest (`package.json` script `"test": "vitest run"`), files `src/**/*.test.ts` (e.g. `src/stores/planReady.test.ts`).

## Execution order

1. Rust pure core: new `src-tauri/src/terminal/program_status.rs` (scanner, report parser, msg sanitizing) and its tests.
2. Write queue + `TerminalSession` wiring: `PtyWriteQueue`, reply sink, `advance_bytes` returns the report bodies.
3. `AgentRuntimeState.program_status` (owned by pty_id), pure `combined_mode`, directory use.
4. Reader thread and EOF: apply reports, emit event, clear on owner EOF; messaging busy helper and the drain on leaving `blocked`.
5. Frontend: scoped atom + listener, pure `resolveIndicatorState`, indicator visuals, pending tint.

## Plan

### 1. `terminal/program_status.rs` (new, declared in `terminal/mod.rs`)

- `pub const MAX_BODY: usize = 4096`.
- `ProgramStatusScanner` with `pub fn feed(&mut self, bytes: &[u8]) -> Vec<Osc7501>` where `pub enum Osc7501 { Query, Report(Vec<u8>) }` (raw body). States: idle (matching the exact prefix `ESC ] 7501 ;`, a partial prefix carried across calls), body (collect until BEL or `ESC \`), skip (body over `MAX_BODY`: drop until terminator or abort). In body and skip, `ESC` + any byte other than `\`, CAN (`0x18`) or SUB (`0x1a`) aborts with no result; the aborting `ESC` is re-scanned as a possible prefix start. A trailing lone `ESC` is carried to the next call. Body `?` → `Query`. `ESC ] 75010 ;` must not match (the `;` right after `7501` is part of the prefix).
- `pub fn parse_report(body: &[u8]) -> ReportAction` with `pub enum ReportAction { Set(ProgramStatus), Clear, Ignore }`: split on `:`, then `key=value`. Any `id` → `Ignore`. `state=clear` → `Clear`. `state` in `working|blocked|done|idle|error` → `Set`. Unknown or missing `state` → `Ignore`. `kind` kept only for `blocked`. `msg`: base64-decode with `base64 = "0.22"` (`src-tauri/Cargo.toml:53`, `Engine` API, standard alphabet), `String::from_utf8_lossy`, then remove `char::is_control` characters (covers C0, DEL, C1); a decode failure drops `msg`.
- `pub struct ProgramStatus { pub state: RootState, pub kind: Option<String>, pub msg: Option<String> }` and `pub enum RootState { Working, Blocked, Done, Idle, Error }` with `as_str()`; derive `Clone, Debug, PartialEq, Eq, serde::Serialize`.

### 2. Write queue and `TerminalSession`

- `terminal/write_queue.rs` (new): `PtyWriteQueue::spawn(inner: Box<dyn Write + Send>) -> PtyWriteQueue` starts one thread that receives `Vec<u8>` from an unbounded `std::sync::mpsc` channel and `write_all`s + `flush`es them to `inner`, stopping (with a `tracing::warn!`) on the first error. `PtyWriteQueue::writer(&self) -> QueueWriter`; `QueueWriter: Write + Send + Clone` whose `write` sends the bytes and returns `Ok(len)` (a closed channel → `BrokenPipe`).
- `TerminalSession`: fields `status_scanner: ProgramStatusScanner` and `status_reply: Option<Box<dyn Write + Send>>`; `pub fn set_status_reply(&mut self, w: Box<dyn Write + Send>)`. Existing constructors unchanged (reply `None`).
- `pub fn advance_bytes(&mut self, bytes: &[u8]) -> Vec<Vec<u8>>`: `track_sync_mode(bytes)`; `status_scanner.feed(bytes)`; for each `Query` write `ESC ] 7501 ; ? ESC \` to `status_reply` (log on error, never panic); collect `Report` bodies; then `self.terminal.advance_bytes(bytes)`; return the bodies.
- `pty.rs` `build_terminal_session` (`:144`): take the `SharedWriter`, spawn a `PtyWriteQueue` over `SharedWriterAdapter(Arc::clone(&writer))`, pass `Box::new(queue.writer())` to the constructor and `set_status_reply(Box::new(queue.writer()))`. Update both callers (`spawn_pty`, test helper near `pty.rs:2856`).
- Caller fix: the reader `match` at `pty.rs:823-828` has an `Err` arm returning `()`; bind the bodies (`let bodies = match ... { Ok(mut g) => g.advance_bytes(..), Err(e) => { log; Vec::new() } };`). `rg "advance_bytes\(" src-tauri/src` for every other caller (tests, benches).
- Tests: add a `CapturedWriter::drain_len(n)` helper that waits until at least `n` bytes are present (500 ms deadline). Unit tests build the session with `session_with_writer` and `set_status_reply(Box::new(w.clone()))`. A pty-level test goes through `build_terminal_session` (test helper near `pty.rs:2856`) with a capturing inner writer: it sends a `key_down`, waits for its bytes with `drain_len`, then feeds a chunk with the 7501 query and checks that the answer also reaches the inner writer. No order between a key and an answer is asserted: wezterm writes through `BufWriter::new(ThreadedWriter::new(writer))` (`term/src/terminalstate/mod.rs:509`), so only answer-before-DA1 is guaranteed. Note: wezterm's writer also carries every key event (`session.rs:312`), mouse report (`:449`, `:482`) and focus event (`:319`).

### 3. Agent state and MCP mode

- `AgentRuntimeState`: `program_status: Arc<DashMap<String, OwnedStatus>>`, `pub struct OwnedStatus { pub pty_id: String, pub status: ProgramStatus, pub set_at: u64 }`.
  - `pub fn apply_program_status(&self, sid: &str, pty_id: &str, action: ReportAction, now: u64) -> Option<StatusChange>`: ignore unless `live_ptys[sid] == pty_id` (`live_ptys` at `agents/state.rs:109`, set by `mark_live`); a live report replaces any entry, another owner's included; `Set` stores (owner = `pty_id`, `set_at` = `now`) and returns a change only when `status` differs (an identical report keeps the old `set_at`); `Clear` removes and returns a change only when an entry existed; `Ignore` → `None`. `StatusChange { previous: Option<ProgramStatus>, current: Option<ProgramStatus> }`.
  - `pub fn clear_program_status_if(&self, sid: &str, pty_id: &str) -> Option<StatusChange>` for EOF (owner only).
  - `pub fn program_status(&self, sid: &str) -> Option<OwnedStatus>`.
  - Never touch `session_activity`.
- Pure `pub fn combined_mode(hook: Option<&SessionActivity>, root: Option<&OwnedStatus>) -> (String, Option<String>, Option<u64>)` (mode, waiting_for, last_activity = max of both, `None` when neither) per `session-directory`; `waiting_for` from `msg` cut to 200 chars, else `permission|question|login|dialog`.
- `mcp/directory.rs:245`: live branch uses `combined_mode(activity.as_ref(), root.as_ref())`, falling back to the DB row when both are `None`.

### 4. Reader thread, EOF, delivery (`pty.rs`, `mcp/messaging.rs`)

- Reader thread, after releasing the session guard and only when `agent_session`: for each body, compute `busy_before = delivery_busy(agents, sid)`, then `parse_report` → `agents.apply_program_status(&session_id, &pty_id, action, now)` → on `Some(change)`: emit `session:program-status` (payload from a pure `fn status_payload(sid, &change) -> serde_json::Value`); if `busy_before && !delivery_busy(agents, sid)`, call `schedule_ready_drain(db, agents, Arc::new(AppBridge::new(app.clone())), sid, Config::load().cross_session.enabled)` with `SharedDb` and `AgentRuntimeState` from `try_state`. Never hold the session mutex while emitting or scheduling.
- EOF block (`pty.rs:845-847`): after `on_agent_pty_eof`, `clear_program_status_if(&eof_session, &eof_pty_id)`; on `Some`, emit `state: null` (and schedule the drain if it was `blocked` — harmless: the PTY is gone, `drain_idle` finds no live target; skip it to keep teardown quiet).
- `mcp/messaging.rs`: pure `pub fn busy_rule(root: Option<RootState>, hook_mode: Option<&str>) -> bool` (design D9: root `blocked` → busy; root `working` → busy iff hook `running`/`needs_attention`; root `done`/`idle`/`error` → not busy; no root → hook `running`/`needs_attention`) and `pub fn delivery_busy(agents: &AgentRuntimeState, sid: &str) -> bool` over it. Use `delivery_busy` at `:192-196` (`target_busy`) and `:221-224` (deferred wake re-check). `pub async fn ready_drain(db: SharedDb, agents: AgentRuntimeState, gate: Option<Arc<WorktreeGateState>>, delivery: Arc<dyn SessionDelivery>, sid: String, enabled: bool) -> usize`: `tokio::time::sleep(IDLE_SETTLE_SECS + 1)` → if `delivery_busy` return 0 → `drain_idle(&db, delivery.as_ref(), &sid, enabled)`; if it pasted 0 and `gate` is set → `drain_worktree_outcomes(&gate, delivery, &sid)` (check its exact signature at its definition and adapt; the `Stop` path passes `&bridge`). `pub fn schedule_ready_drain(...)` = `tauri::async_runtime::spawn(ready_drain(...))`. Check how `ctx.delivery` is typed at `messaging.rs:212` and reuse that type. Tests: `#[tokio::test(start_paused = true)]` on `ready_drain`; add `[dev-dependencies] tokio = { version = "1", features = ["test-util", "macros", "rt"] }` (`src-tauri/Cargo.toml:162-163`); make `MockDelivery` (`mcp/delivery.rs:333`) `pub(crate)` inside a `#[cfg(test)]` module reachable from `mcp::messaging` tests.
- `hooks/server.rs:1057-1066` and the `agent_completed` clear at `:1585-1591`: compute `delivery_busy` before and after `record_activity`; on busy to not busy, and when the event is not `Stop`/`SessionEnd`, call `schedule_ready_drain`. Resolve `WorktreeGateState` with `app.try_state` as the `Stop` path does.
- `mcp/worktree_sessions.rs:509-523` `notify_outcome`: replace `mode == "idle"` with `!delivery_busy(agents, sid) && (agents.program_status(sid).is_some() || hook mode == "idle")`.
- `hooks/server.rs:1103-1119`: run the `Stop` drain and the worktree-outcome drain only when `!delivery_busy(agent_state, csid)` (the hook activity is already recorded above, at `:1057-1066`, so a plain `Stop` is not busy unless the root status is `blocked`).

### 5. Frontend

- `src/stores/programStatus.ts` (new): `export type RootState = "working" | "blocked" | "done" | "idle" | "error"`; `export interface ProgramStatusEntry { state: RootState; kind: string | null; msg: string | null }`; `export const programStatusMapAtom = sessionScopedMapAtom<ProgramStatusEntry>()`; pure `applyProgramStatus(prev, payload)` (remove the key when `state` is null; return `prev` unchanged for an equal entry).
- `src/stores/hooks.ts` setup (`:81`): `unlisteners.push(await listen<ProgramStatusPayload>("session:program-status", ...))`. No clear on `session_end` (design D5).
- `src/lib/sessionIndicatorState.ts` (new): `resolveIndicatorState({ sessionStatus, mode, planReview, pendingAsk, pendingAttention, root })` per the MODIFIED requirement; `isAwaitingSession({ pendingAsk, pendingAttention, root })` for the tint. `IndicatorState` adds `"done" | "error"`.
- `SessionIndicator.tsx`: read `programStatusMapAtom`, call `resolveIndicatorState`, `done` → `ring-1 ring-inset ring-green-500 bg-transparent`, `error` → `bg-red-500`, no animation; delete the private `resolveState`.
- `TopBar.tsx:476`, `SessionRow.tsx:64`: `isAwaiting` via `isAwaitingSession` with the root entry.
- Tests: `src/lib/sessionIndicatorState.test.ts`, `src/stores/programStatus.test.ts`.

## Builder coordination

- Group A (Rust, steps 1-4) then group B (frontend, step 5); no shared files. Contract: event `session:program-status`, payload `{session_id, state, kind, msg}`.
- Interface for the test-writer: `terminal::program_status::{ProgramStatusScanner, Osc7501, parse_report, ReportAction, ProgramStatus, RootState, MAX_BODY}`; `terminal::write_queue::{PtyWriteQueue, QueueWriter}`; `TerminalSession::{advance_bytes -> Vec<Vec<u8>>, set_status_reply}`; `agents::state::{OwnedStatus, StatusChange, AgentRuntimeState::{apply_program_status, clear_program_status_if, program_status}, combined_mode}`; `pty::status_payload`; `mcp::messaging::{busy_rule, delivery_busy, ready_drain, schedule_ready_drain}`; frontend `applyProgramStatus`, `resolveIndicatorState`, `isAwaitingSession`.
- Trap: the reply must be enqueued before `self.terminal.advance_bytes(bytes)` in the same call, and wezterm must write through the same queue, or Claude Code sees DA1 first.
- Trap: do not hold the `TerminalSession` mutex while calling `emit`, touching `AgentRuntimeState`, or spawning the drain.
- Trap: the root status never writes `session_activity` (it would reset `just_idled` and invent a hook mode).

## Per-phase risk

- [Scanner mis-detects a prefix inside another OSC, or swallows output after an aborted OSC] → exact prefix match; abort on `ESC`+other, CAN, SUB; tests for OSC 8/11/75010 and `ESC [ 2 J` after an unterminated body.
- [Reply ordering] → test with the real wezterm terminal and one captured buffer for both writers, read with `drain_len`.
- [Queue thread leaks] → it ends when every `QueueWriter` is dropped (channel closed), i.e. when the session and wezterm drop.
- [Old PTY clobbers new] → owner tests (EOF and late report from another pty_id).
- [Delivery strands or pastes into a dialog] → an exhaustive `busy_rule` test over all 24 root x hook combinations (the invariant, in place of a property-test library the project does not have); `schedule_ready_drain` tests with `MockDelivery` and paused tokio time; walk 6.6 sends a message during the trust prompt.
- [UI regression for sessions without status] → indicator tests repeat today's scenarios with `root` undefined.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test`; `rustfmt --edition 2024 --check <changed .rs files>`; `npx tsc --noEmit`; `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh main osc7501-session-status`.
- Walk with the installed deb (tasks group Verification).

## Review fixes (adversarial + frontend review, 2026-11-26)

Interface for the test-writer (group 7):
- `TerminalSession::take_query_seen(&mut self) -> bool`: true once after `advance_bytes` answered at least one OSC 7501 query, then false until the next query.
- `AgentRuntimeState::{arm_program_status(sid, pty_id), disarm_program_status(sid), program_status_armed(sid, pty_id) -> bool}`; the reader thread checks `program_status_armed(sid, pty_id)` through the pure gate `reports_apply(agent_session, armed)` before calling `apply_program_status`, whose contract stays as it is (so the group 3 tests stay valid); `clear_program_status_if` also disarms. Use the `DashMap::entry` API so compare-and-write is atomic per shard.
- `HookEvent::SessionEnd { session_id, #[serde(default)] reason: Option<String> }` (`src-tauri/src/hooks/events.rs:57`; update the match at `:343` and the test near `:575`). The hook server disarms on `SessionEnd` with `reason != Some("clear")`.
- Pure helpers in `mcp/messaging.rs`: `stop_drain_allowed(root: Option<&RootState>) -> bool` (false only for `blocked`), `ready_transition(busy_before: bool, busy_after: bool) -> bool`, `idle_since(hook: Option<&SessionActivity>, root: Option<&OwnedStatus>) -> Option<u64>` (the time the target stopped being busy, design D9), and the send-path `just_idled` uses it.
- `delivery::is_in_flight(session_id) -> bool` (over `in_flight_set`, `mcp/delivery.rs:173-191`); `ready_drain` and the `Stop` path call `drain_worktree_outcomes` only when `drain_idle` pasted 0 and `!is_in_flight`.
- Pure gate in `pty.rs`: `reports_apply(agent_session: bool, armed: bool) -> bool`.
- Remove the unreachable trigger at `hooks/server.rs:1064-1072`; keep the `agent_completed` trigger (`:1600-1604`) through `ready_transition`.
- Frontend: `SessionIndicator` renders `error` as lucide `X` (`size-3 shrink-0 text-red-500`); `StatusBar.tsx` `modeDotColor` (`:68-72`) replaced by the shared resolution (export a pure `indicatorDotClass(state)` from `src/lib/sessionIndicatorState.ts` or reuse `resolveIndicatorState` + a class map); exhaustive `switch` on `RootState`; contiguous `useAtomValue` block; `_removed` replaced if the lint config rejects it.
- Docs: `docs/design.md` §3.10 ("All three respect reduced-motion" now names the static dots), `docs/architecture.md:59-67` (terminal modules `program_status`, `write_queue`, pre-scan and write queue sentence).
