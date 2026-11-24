# Implementation Plan: plan-decision-delivery

> Grounded in current codebase, symbols verified 2026-11-23.

## Verified codebase facts (do not re-assume)

- `submit_plan_decision` command: `src-tauri/src/commands/plans.rs:113-147`, a synchronous `pub fn` (Tauri runs it on the main thread). Unix branch writes with `std::fs::write(&decision_path, &payload)` at `:146`; Windows branch follows (frozen, untouched). Other commands in the same file take `State<'_, AgentRuntimeState>` (`plans.rs:297`).
- Adapter seam: `ClaudeCodeAdapter::submit_plan_decision` at `src-tauri/src/agents/claude_code/adapter.rs:280-299` uses `tokio::fs::write(&fifo_path, ..)`; existing tests `submit_plan_decision_without_registered_fifo_returns_session_locked` (`:593`) and `submit_plan_decision_writes_to_registered_fifo` (`:612`). Check the second test's setup: it may write to a regular file and need a real FIFO after D3.
- Hook side: `FifoGuard::create` runs `mkfifo` (`src-tauri/src/hooks/cli.rs:149-176`) and unlinks on drop; `blocking_fifo_read_liveness_aware` opens `O_RDONLY | O_NONBLOCK`, polls 1 s, stops at EOF or when the buffer parses as JSON.
- `libc` is a `cfg(unix)` dependency (`src-tauri/Cargo.toml:110-116`).
- Registry: `pending_plan_fifos: Arc<DashMap<String, PathBuf>>` (`adapter.rs:44`), `register_pending_plan_fifo` (`adapter.rs:74`), called from the `PlanReview` handler with the Nergal session id (`server.rs:1247-1251`) through `agent_state.claude_code`. The frontend sends the same id as `sessionId` (`plan:ready` `session_id` is `nergal_session_id.unwrap_or(session_id)`, `server.rs:1281`).
- Event order: the `PreToolUse` hook is `async: true` in `~/.claude/settings.json`; the server spawns a task per connection (`server.rs:370`).
- Emitters: `PreToolUse` `plan:ready` without `decision_path` (`src-tauri/src/hooks/server.rs:1099-1135`); `PlanReview` with it (`:1237-1285`), emitted only inside `if let Some((path, content)) = resolve_active_plan(..)`. `resolve_active_plan` at `:761`. Plan state is keyed by CC `session_id` (`plan_state.get_or_create(session_id)`, `src-tauri/src/plan_state.rs:25`); `PlanManager` exposes `current_path()` (`src-tauri/src/agents/claude_code/plan.rs:127`) and `current_content()` (`:111`).
- Frontend listener: `src/stores/hooks.ts:276-300` builds `planData` with `decisionPath: payload.decision_path ?? ""`, writes `planStateMapAtom` and `planDocumentsAtom` (`withBoundedEntry`, `PLAN_DOCUMENTS_MAX` from `src/stores/plan.ts:86`) and sets `pending_review` unconditionally.
- `PlanState` and `defaultPlanState`: `src/stores/plan.ts:43-53`. `PlanReviewStatus = "idle" | "pending_review" | "submitted"` (`:33`).
- Only the active tab's `DocumentContent` renders (`src/components/layout/RightPanel.tsx:219-221`).
- Panel: `PlanPanel` reads `docs[path]` (`src/components/plan/PlanPanel.tsx:29-30`), `backendSessionId`/`decisionPath` at `:76-77`; `handleRevise` `:98-113`, `handleApprove` `:115-127` (silent `idle` when either is empty).
- Tests: vitest without DOM; Rust tests may live in `#[path]` files for hash locking.

## Execution order

1. Rust FIFO helper: new `src-tauri/src/plan_fifo.rs` (`#[cfg(unix)]`, registered in `src-tauri/src/lib.rs`), `write_plan_decision(path: &Path, payload: &[u8], deadline: Duration) -> io::Result<()>` per design D2; tests in `src-tauri/src/plan_fifo_tests.rs`.
2. Registry access: add `claim_pending_plan_fifo(session_id, requested: &Path) -> bool` on `ClaudeCodeAdapter` (`DashMap::remove_if`, true only when the stored path equals `requested`). In the `PlanReview` handler (`server.rs:1247-1251`), register only when `Path::new(fifo_path).parent() == Some(ipc_dir)` (`platform::ipc_dir()`, `src-tauri/src/platform/mod.rs:69`), emit `decision_path: ""` with a `tracing::warn!` when the parent check rejects the FIFO, and add a `tracing::warn!` in the `resolve_active_plan` `None` branch. All of it under `#[cfg(unix)]`; the Windows registration stays as today (`ipc_dir()` is Unix-only, `src-tauri/src/platform/mod.rs:68-69`).
3. Command (the claim and helper only in the `#[cfg(unix)]` branch; the Windows branch keeps its code, optionally inside `spawn_blocking`, without a claim): `submit_plan_decision` becomes `async`, takes `State<'_, AgentRuntimeState>`, claims the registration (mismatch → error, nothing opened), then runs the helper in `tokio::task::spawn_blocking` with a 5 s deadline. The claim is not undone on error. Keep `save_plan_edits_if_dirty` and the deny template as they are. The adapter's `submit_plan_decision` uses the helper too (Unix), inside `spawn_blocking`; rewrite its test `submit_plan_decision_writes_to_registered_fifo` (`adapter.rs:612`) with `mkfifo` and a reader thread, and add a regular-file case that returns `AdapterError::Io`.
4. Frontend: pure `applyPlanReady(prevPlan, prevStatus, payload)` in new `src/stores/planReady.ts` returning `{ plan, status }` per design D1; the `plan:ready` listener applies `plan` to `planStateMapAtom[sid]` and to `planDocumentsAtom[payload.path]`, and `status` to `planReviewStatusMapAtom[sid]`. Tab opening and activity stay as they are.
5. `PlanPanel`: the `A`/`R` keydown handler returns on `e.repeat`; a `useRef` in-flight flag makes `handleApprove`/`handleRevise` return while a submit is pending (set before `invoke`, cleared in `finally`). A pure `decisionGuard({ repeat, inFlight, decisionPath, sessionId })` in `src/stores/planReady.ts` returns `"ignore" | "fail" | "send"`. Both call one failure path for an empty decision path or session id and for a rejected invoke: error toast (D3 copy, backend error text as description), status `idle`; Revise does not clear annotations on failure.

## Per-phase risk

- Helper: a partial non-blocking write must continue from the written offset; the 100000-byte test covers payloads larger than the pipe buffer. The test reader must open the FIFO before the write starts.
- Command signature change: the frontend `invoke` already awaits a promise, so `async` changes nothing for callers.
- Registry: the adapter's own `submit_plan_decision` removes the entry before writing (`adapter.rs:285`); keep that, since a failed write there means a dead FIFO.
- Command: the `SharedPlanState` `std::sync::Mutex` guard for `save_plan_edits_if_dirty` must drop before any `.await`, or the future is not `Send` and the command does not compile.
- Frontend: `applyPlanReady` compares the payload with `prevPlan.original`, never `content` (`setPlanContentAtom`, `src/stores/plan.ts:65-72`, edits `content`); content equality compares the full markdown string; the preview and the review resolve from the same `tool_input`, so a same-cycle pair carries the same text.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, `npx tsc --noEmit`, `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh main plan-decision-delivery`.
- Manual walk on the installed `.deb` (tasks group 4).

## Revision 1 plan (review follow-ups)

- `src-tauri/src/plan_fifo.rs`: `ENXIO` retry until `min(deadline, 1 s)`; add `pub fn fifo_in_ipc_dir(fifo: &Path, ipc_dir: Option<&Path>) -> bool` (parent equals `ipc_dir`).
- `src-tauri/src/hooks/server.rs:1245-1270` (PlanReview, Unix): use `fifo_in_ipc_dir(Path::new(fifo_path), ipc_dir().ok().as_deref())`; log `ipc_dir()` errors separately.
- `src-tauri/src/platform/mod.rs:329-345` `plan_review_fifo_path`: Unix name `plan-{pid}-{uuid}.fifo` with `uuid::Uuid::new_v4().simple()`; Windows and other targets unchanged. Only caller: `src-tauri/src/hooks/cli.rs:226`.
- `src/stores/plan.ts:33`: add `"undelivered"` to `PlanReviewStatus`; new atom `planDecisionInFlightAtom` (session-scoped map of booleans, `sessionScopedMapAtom` like `planReviewStatusMapAtom` at `:35`).
- `src/stores/planReady.ts`: add `isLiveReview(docPlan, sessionPlan, status)`.
- `src/components/plan/PlanPanel.tsx`: `canAnnotate` from `isLiveReview(docs[path], planStateMap[sessionId], reviewStatus)`; header shows `Not delivered. Answer in the terminal.` for `undelivered`; buttons `disabled` + `aria-busy` while in flight; `failDecision(action, err)` uses the toast copy of design Revision 1 and sets `undelivered`; empty Revise feedback goes through `failDecision`; listeners call handlers through a ref updated each render. Check other readers of `canAnnotate`/`activePlanReviewStatusAtom` (`src/stores/annotations.ts:31` `canEnterAnnotationModeAtom`) still behave.
- `docs/hooks.md:25-33`: FIFO name with nonce, `pending_review` only from the review event, `submitted`/`undelivered` transitions.

## Revision 2 plan (re-review follow-ups)

- `src/stores/planReady.ts`: `isReviewDocument(doc, session)`, `showReviewAnnotations(doc, session, status)` (pending_review or undelivered); `isLiveReview` = `isReviewDocument` and `pending_review`; `decisionGuard` gains `status: PlanReviewStatus` and returns `ignore` unless `pending_review`.
- `src/components/plan/PlanPanel.tsx`: `annotationsEnabled` (passed to `AnnotatableMarkdownView` at `:427`), the `X` key and the clear listener use `showReviewAnnotations`; Approve/Revise, `A`/`R`, global comment and the annotation toggle use `isLiveReview`; `guardDecision` passes `appStore.get(planReviewStatusMapAtom)[sessionId]`. Verify in `src/components/plan/AnnotatableMarkdownView.tsx` that `annotationsEnabled` with annotation mode off renders and restores highlights without letting a text selection create one (the `CREATE` handler and `annotationModeRef`); if creation does not depend on annotation mode, gate creation on `pending_review` explicitly.
- `src-tauri/src/plan_fifo.rs`: `pub fn review_decision_path(fifo_path: &str, ipc_dir: Option<&Path>) -> Option<String>` built on `fifo_in_ipc_dir`; `ENXIO` budget 900 ms. `src-tauri/src/hooks/server.rs` PlanReview (Unix) applies it: `Some` → register and emit that path, `None` → no registration and an empty `decision_path`.
- `docs/hooks.md:13` FIFO name; `:33` causes of `undelivered`.

## Revision 3 plan (walk findings)

### Verified codebase facts (do not re-assume)
- `src-tauri/src/hooks/cli.rs:206` `plan_review`: reads stdin JSON, `tool_input` at `:249`; no-session early `output_allow()` at `:215-219`; decision mapping at `:284-295` via `parse_plan_review_decision` (`:312`, enum `PlanReviewDecision` `:307`); `output_allow` at `:792` prints `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}`.
- `src-tauri/src/agents/claude_code/adapter.rs:74` `register_pending_plan_fifo` uses `DashMap::insert` (returns the replaced path, discarded today); `:80` `claim_pending_plan_fifo` (`remove_if`).
- `src-tauri/src/hooks/server.rs:1268-1275` registers the FIFO on `PlanReview` (key: Nergal session id); `:1174` `PostToolUse` handler (has `agent_id` in the event, `events.rs:66`); `:1198` `Stop`; `:1129` `SessionEnd`.
- `src-tauri/src/plan_fifo.rs:85` `write_plan_decision(path, payload, deadline)` (no create, no symlink, ENXIO budget).
- `src-tauri/src/setup.rs:41-45` Nergal `PostToolUse` matcher; `merge_hook` upgrades the matcher in place (`:494-509`).
- Frontend: `src/stores/plan.ts:33` `PlanReviewStatus`; `src/stores/hooks.ts:133-135` `stop` sets `idle` unconditionally, `:181-183` `session_end`; `plan:ready` listener `:278`; `src/stores/planReady.ts` `showReviewAnnotations` (`:86`), `decisionGuard` (`:59`); `src/components/plan/PlanPanel.tsx:52-53` status flags, header label for `undelivered`.

### Execution order
1. `cli.rs`: pure `plan_review_output(decision, tool_input) -> Option<serde_json::Value>` (Allow → allow + `updatedInput`, Deny → deny + message, Released → `None`); `PlanReviewDecision::Released` from `{"released": true}`; no-session path prints nothing. `plan_review` prints only `Some`.
2. Adapter: `register_pending_plan_fifo` returns the replaced path; add `release_pending_plan_fifo(session_id) -> Option<PathBuf>` (remove).
3. Server: helper `release_plan_review(path)` spawns a blocking thread that calls `write_plan_decision(path, b"{\"released\":true}", 1 s)` and logs a failure. `PlanReview`: release the replaced path when it differs from the new one. `PostToolUse` with `tool_name == "ExitPlanMode"` and no `agent_id`: release + emit `plan:review-closed { session_id }` (Nergal id). `Stop`, `SessionEnd`: release.
4. Setup: append `ExitPlanMode` to the `PostToolUse` matcher.
5. Frontend: `"answered"` status; pure `applyReviewClosed(status) -> { status, toast }` in `planReady.ts`; listener in `hooks.ts`; `stop` keeps `answered`; `showReviewAnnotations` true for `answered`; PlanPanel header label and copy.
6. Docs: `docs/hooks.md` plan review section (updatedInput, release, `answered`).

### Per-phase risk
- Setup matcher change only reaches users after setup runs (existing flow: hooks status shows not registered → register). Without it, D5 still releases on `PlanReview`/`Stop`; only the `answered` label needs it.
- A release write to a FIFO whose hook already exited fails fast (ENXIO budget ≤ 1 s) on a blocking thread; the event loop never waits.
