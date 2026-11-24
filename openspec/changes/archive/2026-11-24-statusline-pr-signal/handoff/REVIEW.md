# REVIEW: statusline-pr-signal (FRI-473)

## Adversarial

Scope: `git diff mufdidev/fri-507-bracketed-paste-gate -- src src-tauri` plus the five untracked files.
Ran: vitest (2 new files, 18 pass), `tsc --noEmit` clean, `cargo test --lib pr_` (13 pass), `cargo clippy --all-targets -D warnings` clean, `cargo fmt --check` clean.
CC schema checked against code.claude.com/docs/en/statusline: `pr.number`, `pr.url`, `pr.review_state`, `pr.kind` match the feeder (`src-tauri/src/hooks/cli.rs:719-729`).

### Bugs

- **Important.** A persistent contradiction retries every 10 s forever and freezes the pill.
  - `src/stores/git.ts:197-205` clears `signal` on contradiction; `src/stores/prSignal.ts:26` then fires every check.
  - CC's `pr` is "open pull request for the current branch" (docs).
  - `get_pr_status` queries the DB `worktree_branch` instead (`src-tauri/src/commands/ship_pr.rs:222-223`).
  - An agent that runs `git checkout -b` inside a worktree creates a mismatch that never resolves.
  - Every `gh` answer is then dropped: a later MERGED or CLOSED state never shows.
  - Same outcome for a Ship of PR 13 while an idle CC still reports a stale `pr:12`.
  - Nothing is logged and there is no backoff. Fix: accept gh's answer after N contradictions, or back off and `warn`.
- **Important.** No in-flight guard, so the sequence guard can starve writes.
  - `check()` (`src/components/git/GitPanel.tsx:208-213`) does not know a fetch is pending.
  - Every check while stale starts a new fetch, which bumps `seq` (`src/stores/git.ts:184`). The older response is then dropped (`:195`).
  - If `gh pr view` takes over 10 s (it has no timeout), no response ever lands.
  - Concurrent `gh` processes also pile up on the blocking pool.
  - Milder form: a mount fetch is in flight when the first status tick moves the signal from undefined to a value. That makes two `gh` calls, and the first is wasted.
  - This also breaks the spec's "SHALL call `get_pr_status` for S once".
- Verified fine: the CI effect still stops on non-OPEN (`GitPanel.tsx:248`) and polls every 20 s (`:258`). Its deps are now number and state.
- Verified fine: the session switch re-runs the effect through `sessionId` (`GitPanel.tsx:214`). Refs avoid stale closures.
- Verified fine: boxing `prompt_cache` leaves the wire JSON unchanged. Serde `Box<T>` is transparent, and emit unboxes it (`src-tauri/src/hooks/server.rs:1487`).
- Verified fine: every `AgentStatus` producer sets `github_pr`, including the legacy `StatusLine` arm and codex/opencode/pi. `github_pr` is emitted at `server.rs:1616`.
- Verified fine: the dedup map key (`nergal_session_id.unwrap_or(session_id)`) matches the cleanup key. Events without a nergal id are dropped earlier (`server.rs:952`).
- Verified fine: a poisoned lock is skipped without a panic (`server.rs:624`, `:735`).
- **Nit.** The listener writes a new `prSignalMap` object on every tick (`src/stores/hooks.ts:356`).
  - So GitPanel (`GitPanel.tsx:90`) re-renders on every status tick of any session.
  - Return `prev` when the signature is unchanged.
- **Nit.** `pr_signature_cache` cleanup runs after the early returns of `finalize_session_obsidian` (`server.rs:679-694`, `:735`).
  - A claimed or already-deleted session leaves its entry behind. The growth is tiny and mirrors `model_cache`.
- **Nit.** Ship ignores `result.pr_info` (`src/components/git/ShipDialog.tsx:312`) and spends a second `gh` round trip (`:316`).
  - Seeding the cache would skip it.

### Security

- No findings. The info log carries only the session id and the signature (`server.rs:622-631`).
- `github_pr.url` is never rendered by the frontend.
- The map grows only through the user-private hook socket.

### Compliance

- **Important.** git-panel-v2, scenario "A change while the panel showed another session": no test or walk covers it.
  - It is implemented at `GitPanel.tsx:206-214`, but tasks 3.3-3.5 never switch sessions.
- **Important.** Scenarios "A new pull request appears" and "Repeated updates without a change" only have pure tests of `shouldRefreshPr`.
  - No test or walk drives the GitPanel wiring for those two scenarios.
  - Walk 3.3 checks the backend log, not frontend fetch counts. No walk has CC open a PR.
- **Important.** The spec says a successful PR merge "SHALL refresh that session's PR status at once".
  - The code refreshes only when the cleanup fails (`src/components/git/PrViewer.tsx:541`), per design D6.
  - The spec has no exception for a deleted session. Fix the spec wording.
- **Nit.** Design D6 says `PR_INFO_TTL_MS` is "the single age `shouldRefreshPr` uses".
  - `PR_INFO_TTL_MS` is now dead (`src/stores/git.ts:225`). `PR_FALLBACK_MS` duplicates it (`src/stores/prSignal.ts:13`).
- **Nit.** agent-status-telemetry, scenario "Pass-through to the frontend": no Rust test asserts that the emitted payload carries `github_pr`.
  - Only walk 3.5 implies it.
- Tasks 3.1-3.5 are still unchecked. The walks are pending.

Verdict: REVISE. Fix the two Important bugs (persistent contradiction, in-flight starvation) and align the merge clause of the spec.

## Frontend

Scope: GitPanel, ShipDialog, PrViewer, stores git/hooks/workspace, new prSignal.ts. `npx tsc --noEmit` is clean.

### 🔴
None.

### Important
- `src/stores/hooks.ts:356` writes `prSignalMapAtom` on every `agent:status-update` with a fresh map object, even when the string is unchanged. `GitPanel.tsx:90` reads the whole map via `useAtomValue`, so the panel re-renders on every status update. Skip the `set` when `prev[sid] === sig`, or read a per-session derived atom. No loop (the effect keys on the string), but it is avoidable render churn.
- `src/stores/git.ts:225` `PR_INFO_TTL_MS` is now unused (only its definition remains). `src/stores/prSignal.ts:12` defines its own `PR_FALLBACK_MS = 60_000`. design.md D6 says `PR_INFO_TTL_MS` is the single age. Use one constant in `shouldRefreshPr`, or delete the export.

### Nit
- `src/stores/git.ts:200` and `GitPanel.tsx:208`: after a failure the signal is cleared, so the check retries every 10 s while `gh` keeps failing (no backoff). That is still better than the old 5 s poll. Know it for the walk.
- `src/stores/git.ts:200`: on a first-ever fetch that fails, no entry is written (`if (!entry) return prev`). The pill stays empty and retries in 10 s. Previously a null entry was written. Acceptable.
- First status update after Ship: the cache holds signal `none`, then CC reports `pr:N`. That costs one extra `gh pr view`. Harmless.
- `src/components/git/GitPanel.tsx:197-200`: the "refresh loop" comment above the first effect no longer concerns PR state. Still true for the other refreshers.
- `src/stores/workspace.ts:1`: the type import sits above the `jotai` import. Order only.

### Verified, no issue
- Effect at `GitPanel.tsx:203-215`: one interval per `(sessionId, signal)`. Cleanup clears it on unmount and on session switch. Refs are reassigned each render, so the interval reads current maps. An old-session in-flight fetch writes only its own key.
- Pill flicker: `data` is kept during a fetch and replaced only on a good response. Failure or contradiction keeps old `data` and `fetchedAt`. No flicker.
- Race: a signal change during an in-flight fetch re-runs `check` and bumps `seq`. The stale response is dropped (`git.ts:193`).
- Two mounted panels for one session (panel plus zen) can double-fetch. The seq guard keeps the result correct.
- CI cadence: effect deps are `prInfoNumber` and `prInfoState`, so 20 s holds. I found no UI copy, tooltip or doc in `src/components/git`, `docs/patterns.md` or `docs/design.md` promising 5 s.
- ShipDialog: the "Shipped / PR #N created" toast still matches. The refresh is fire-and-forget after `refreshGit`.
- PrViewer: the "PR merged" toast is accurate. Refresh runs only in the cleanup failure branch, where the session survives. The action also returns early when the session is gone. No refresh on a deleted session.
- Walk 3.3-3.5: expect the CI indicator to tick every 20 s, not 5 s. A PR created by Ship should show at once.

Verdict: approve with two Important cleanups (status-update re-render churn, duplicate 60 s constant); nothing blocks the walk.

## Orchestrator ruling

- Every finding is fixed (user preference): Revision 1 in design.md, tasks group 3.

## Adversarial re-review (Revision 1)

Scope: the same diff plus the five untracked files, against design "Revision 1" and both delta specs.
Ran: vitest on the two store files (22 pass), `tsc --noEmit` clean, `cargo test --lib pr_` (14 pass).

### Prior findings

- Contradiction retry loop: fixed. A failure or empty `pr:N` answer stamps signal and `fetchedAt` (`src/stores/git.ts:203-210`).
- In-flight starvation: fixed. Guard at `src/stores/git.ts:187`, set at `:190`, cleared in `finally` at `:212-214`.
- Listener churn: fixed. Write only on change (`src/stores/hooks.ts:357`).
- Cleanup order: fixed. Removal runs before `claim_finalization` (`src-tauri/src/hooks/server.rs:680-683`).
- Ship second `gh` call: fixed. `setPrInfoFromShip` (`src/stores/git.ts:219-230`, `src/components/git/ShipDialog.tsx:316`).
- "Another session" scenario: fixed. Named case in `src/stores/prSignal.test.ts`, plus walk 4.6.
- Merge spec wording: fixed. The requirement now says "when the session survives the merge cleanup".
- Duplicate constant: fixed. `PR_INFO_TTL_MS` is gone; `PR_FALLBACK_MS` at `src/stores/prSignal.ts:13`.
- Pass-through test: fixed. `agent_status_payload_serializes_github_pr` in `src-tauri/src/hooks/pr_signature_tests.rs`.
- Frontend nits: first failure now writes a null entry (`git.ts:207`). Docblock at `GitPanel.tsx:169` is updated.

### Bugs

- **Important.** The merge refresh can be swallowed by the in-flight guard.
  - `PrViewer.tsx:541` calls `refreshPrInfoAction`, which returns at `git.ts:187` while a fetch is pending.
  - That older fetch may have started before the merge. It then lands OPEN, stamped fresh.
  - The pill stays OPEN up to 60 s. CC is usually idle, so no signal rescues it.
  - This breaks the merge clause and can flake walk 4.4. Odds are low (fetch runs about 1 s per 60 s).
  - Fix: a `force` path that bumps `seq` and skips the guard.
  - With `force`, clear the flag in `finally` only when `seq` still matches.

### Verified fine

- The flag cannot stick on an exception. Both early returns (`git.ts:186-187`) run before it is set.
- A hung `gh` holds the flag until it exits. That matches the old outcome, with one process instead of many.
- No loop under "gh is authority". PR 30 under `pr:12` caches with signal `pr:12`. `shouldRefreshPr` waits 60 s.
- MERGED PR 12 under `pr:12` caches normally. A `null` under `pr:12` keeps data and waits 60 s.
- A first-ever failure writes `{data: null, fetchedAt: now}`. The next try waits 60 s, not 10 s.
- Ship versus an in-flight fetch: the `seq` bump drops the old answer. The flag still clears in `finally`.
- GitPanel: one interval per `(sessionId, signal)`. Refs read current maps (`GitPanel.tsx:206-215`).
- The CI effect keys on number and state (`GitPanel.tsx:248`, `:260`).
- The backend feeds the payload through one pure builder (`server.rs:1488`, `agent_status_payload`).
- `finalize_session_obsidian` runs at every `SessionEnd`, ungated by the Obsidian config (`server.rs:1131`).
- It also runs on PTY EOF for launched sessions (`pty.rs:843`). Non-Obsidian setups get the cleanup too.
- The legacy `StatusLine` arm emits `github_pr: None`. The feeder sends only `AgentStatus`, so signals never alternate.

### Compliance

- agent-status-telemetry: all three scenarios have tests (`statusline_pr_tests.rs`, `pr_signature_tests.rs`).
- git-panel-v2: every scenario maps to code plus a unit test or a group 4 walk.
- Tasks 4.1-4.6 are still unchecked. The walks are pending.

### Nits

- **Nit.** A signal change during a fetch is skipped (`git.ts:187`). The answer stamps the old signal (`:191`).
  - A second `gh` then runs within 10 s. Harmless, but "SHALL call once" is not strict there.
- **Nit.** `create_pr` can return `PrInfo { number: 0 }` (`src-tauri/src/worktree.rs:1223-1228`).
  - Ship now caches it as-is. The pill may show `#0` for up to 60 s.
- **Nit.** Pruning a session mid-fetch leaves `[sid]: false` behind (`git.ts:213`). The leak is tiny.
- **Nit.** Under `none` or no signal, a `gh` blip returns `null` and erases the pill (`git.ts:203`).
  - This predates the change. The failure scenario covers only the `pr:N` case.
- **Nit.** Walks 4.3 and 4.5 cannot count `get_pr_status` calls; that command logs nothing.
  - "Repeated updates without a change" therefore rests on unit tests alone.

Verdict: APPROVE after the merge-refresh `force` fix; walks 4.1-4.6 still gate the close.
