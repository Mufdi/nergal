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
- MCP descriptor: `src-tauri/src/mcp/directory.rs:245` picks `(mode, last_activity, waiting_for)` from `agents.session_activity(&s.id)` when live, else the DB row. This is the only place that needs the combined mode.
- Hook server writes activity at `src-tauri/src/hooks/server.rs:1057-1066` (unchanged by this change).
- Frontend: listeners are registered in one async setup function in `src/stores/hooks.ts:81` returning `UnlistenFn[]` (pattern: `unlisteners.push(await listen<Payload>("event", (payload) => {...}))`, e.g. `attention:pending` near `:398-412`). `listen` comes from `@/lib/tauri`.
- `src/components/session/SessionIndicator.tsx`: `IndicatorState` union (`:7`), `STATE_COLORS`/`STATE_ANIMATIONS` maps (`:15-29`), component (`:37`), private `resolveState` (`:57`). Atoms: `modeMapAtom` (`src/stores/workspace.ts:160`), `planReviewStatusMapAtom` (`src/stores/plan`), `pendingAsksAtom`/`pendingAttentionAtom` (`src/stores/askUser.ts`).
- Frontend tests: vitest (`package.json` script `"test": "vitest run"`), files `src/**/*.test.ts` (e.g. `src/stores/planReady.test.ts`).

## Execution order

1. Rust pure core: new `src-tauri/src/terminal/program_status.rs` (scanner + report parser) and its tests.
2. `TerminalSession` wiring: reply writer, `advance_bytes` returns the reports.
3. `AgentRuntimeState.program_status` + pure `combined_mode`, directory use.
4. Reader thread: apply reports, emit event, clear on EOF.
5. Frontend: atom + listener, pure `resolveIndicatorState`, indicator visuals.

## Plan

### 1. `terminal/program_status.rs` (new, declared in `terminal/mod.rs`)

- `ProgramStatusScanner` with `fn feed(&mut self, bytes: &[u8]) -> Vec<Osc7501>` where `Osc7501` is `Query` or `Report(Vec<u8>)` (the raw body). State: matching the prefix `ESC ] 7501 ;` (partial prefix carried), collecting the body, discarding an oversized body (cap `MAX_BODY = 4096`), terminator BEL or `ESC \` (an `ESC` at the end of a chunk carried over). A body equal to `?` is `Query`.
- `fn parse_report(body: &[u8]) -> Option<ReportAction>` with `ReportAction::{Set(ProgramStatus), Clear, Ignore}`: split on `:`, then `key=value`. Any `id` → `Ignore`. `state=clear` → `Clear`. `state` in `working|blocked|done|idle|error` → `Set`. Unknown or missing `state` → `Ignore`. `kind` kept only for `blocked`. `msg` base64-decoded (`base64 = "0.22"` is a direct dependency, `src-tauri/Cargo.toml:53`; use its `Engine` API with the standard alphabet), then `String::from_utf8_lossy`; a decode failure drops `msg`.
- `ProgramStatus { state: RootState, kind: Option<String>, msg: Option<String> }`, `RootState` enum with `as_str()`, `Clone + PartialEq + Debug + Serialize`.
- Tests: every `program-status-protocol` parsing scenario, the split and oversized cases, BEL and ST terminators, a query split across chunks.

### 2. `TerminalSession`

- Add a field `program_status: ProgramStatusScanner` and `status_reply: Option<Box<dyn Write + Send>>`.
- New constructor parameter or builder `with_status_reply(writer)`; `build_terminal_session` passes `Box::new(SharedWriterAdapter(Arc::clone(&writer)))` a second time. Keep the existing constructors compiling for other callers (tests): `None` means no reply.
- `advance_bytes(&mut self, bytes) -> Vec<Vec<u8>>`: run `track_sync_mode`, then the scanner; for each `Query` write `ESC ] 7501 ; ? ESC \` to `status_reply` (log on error, never panic); collect `Report` bodies; then `self.terminal.advance_bytes(bytes)`; return the bodies. Callers that ignore the return compile with `let _ =` or by not binding (it is not `#[must_use]`).
- Tests in `session.rs` tests module, using the captured writer: query answered; answer precedes DA1 within one chunk; split query answered once; report writes nothing; report leaves the grid unchanged (`ok` at column 0).

### 3. Agent state and MCP mode

- `AgentRuntimeState`: `program_status: Arc<DashMap<String, ProgramStatus>>`; `apply_program_status(&self, sid, action) -> Option<Option<ProgramStatus>>` returns `Some(new)` only when the stored value changed (`Some(None)` = removed); `program_status(&self, sid)`; `clear_program_status(&self, sid) -> bool`. Stamp `last_activity` in `session_activity` when a status is set, without touching its mode.
- Pure `fn combined_mode(hook: Option<&SessionActivity>, root: Option<&ProgramStatus>) -> (String /*mode*/, Option<String> /*waiting_for*/)` per the `session-directory` requirement (200-char cut on `msg`, `permission|question|login|dialog` fallback). Place it next to `SessionActivity` in `agents/state.rs` or in `mcp/directory.rs`; tests cover every scenario of the requirement.
- `mcp/directory.rs:245`: in the live branch, use `combined_mode` with the hook activity and the root status; a live session with a root status but no hook activity uses `combined_mode(None, root)` and `s.updated_at` for `last_activity`.

### 4. Reader thread and EOF (`pty.rs`)

- Bind the returned bodies inside the lock, release the guard, then (only when `agent_session`) for each body: `parse_report` → `agents.apply_program_status(&session_id, action)` → when changed, `eof_app.emit("session:program-status", payload)` with `{session_id, state, kind, msg}` (`state` `null` when removed). Use `try_state::<AgentRuntimeState>()` as the EOF path does. Do not hold the session mutex while emitting.
- `on_agent_pty_eof` (`pty.rs:970`): also clear the root status; if it existed, emit `session:program-status` with `state: null`. It needs the `AppHandle` for the emit: pass it, or emit from the EOF block at `pty.rs:845-847` after the call.
- Extract the apply-and-decide step into a pure helper (`fn status_event(sid, change) -> Option<Payload>`) so tests cover dedupe and quake (`agent_session = false` → nothing).

### 5. Frontend

- `src/stores/programStatus.ts` (new): `export type RootState = "working" | "blocked" | "done" | "idle" | "error"`; `programStatusMapAtom: atom<Record<string, { state: RootState; kind: string | null; msg: string | null }>>({})`; a pure reducer `applyProgramStatus(prev, payload)` (remove the key when `state` is null).
- `src/stores/hooks.ts` setup (`:81`): `unlisteners.push(await listen<ProgramStatusPayload>("session:program-status", (p) => set(programStatusMapAtom, (prev) => applyProgramStatus(prev, p))))`. Also drop the session's key on the existing `session_end` handling (`hooks.ts:182-185`) so an ended session falls back cleanly.
- `src/lib/sessionIndicatorState.ts` (new): pure `resolveIndicatorState({ sessionStatus, mode, planReview, pendingAsk, pendingAttention, root })` implementing the `session-state-indicators` priority; `IndicatorState` adds `"done" | "error"`.
- `SessionIndicator.tsx`: read `programStatusMapAtom`, call `resolveIndicatorState`, add `done` (`ring-1 ring-inset ring-green-500 bg-transparent`) and `error` (`bg-red-500`) to `STATE_COLORS`, `""` animations. Delete the private `resolveState`.
- Tests: `src/lib/sessionIndicatorState.test.ts` (every indicator scenario, including no-root fallbacks), `src/stores/programStatus.test.ts` (reducer set/replace/remove).

## Builder coordination

- One Rust builder group (1-4) then one frontend group (5); they share no files. The Tauri event name `session:program-status` and payload `{session_id, state, kind, msg}` are the contract between them.
- Interface for the test-writer: `terminal::program_status::{ProgramStatusScanner, Osc7501, parse_report, ReportAction, ProgramStatus, RootState, MAX_BODY}`; `TerminalSession::advance_bytes -> Vec<Vec<u8>>` and a constructor that takes the reply writer; `AgentRuntimeState::{apply_program_status, program_status, clear_program_status}`; `combined_mode(hook, root)`; frontend `applyProgramStatus`, `resolveIndicatorState`.
- Trap: the reply must be written before `self.terminal.advance_bytes(bytes)` in the same call, or Claude Code sees DA1 first and turns the protocol off.
- Trap: `advance_bytes` gains a return value; check every caller (`rg "advance_bytes\(" src-tauri/src`) including tests and the emitter benchmarks.
- Trap: do not hold the `TerminalSession` mutex while calling `emit` or touching `AgentRuntimeState`.

## Per-phase risk

- [Scanner mis-detects a prefix inside another OSC] → it matches the exact prefix `ESC ] 7501 ;`; tests feed OSC 8 and OSC 11 around a report.
- [Reply ordering] → the DA1-ordering test runs the real wezterm terminal with the captured writer.
- [Stale `program_status` after PTY reuse] → EOF clears it; a test drives `apply` then the EOF clear.
- [UI regression for sessions without status] → indicator tests repeat today's scenarios with `root` undefined.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test`; `rustfmt --edition 2024 --check <changed .rs files>`; `npx tsc --noEmit`; `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh main osc7501-session-status`.
- Walk with the installed deb (see tasks group Verification).
