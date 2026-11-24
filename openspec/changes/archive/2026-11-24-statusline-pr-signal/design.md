## Context

The git panel polls `get_pr_status` (`gh pr view`, network) every 5 seconds (`src/components/git/GitPanel.tsx:194-198`). Claude Code's status line input carries `pr` (number, url, review_state, kind) for the branch's open PR, updated event-driven with a 300 ms debounce, and quiet while the session is idle (code.claude.com/docs/en/statusline).

## Goals / Non-Goals

**Goals:** fewer `gh pr view` calls; PR changes visible sooner when Claude Code reports them; no staleness for PRs Nergal itself creates or merges.

**Non-Goals:** replacing `gh pr view`; parsing review state or GitLab merge requests; a new status line field for CI.

## Decisions

### D1. One refresh rule keyed by the signature at fetch time
`PrInfoCacheEntry` records `signal` (the session's signature when the fetch started). A pure `shouldRefreshPr(entry, currentSignal, now)` returns true for no entry, a signature that differs from `entry.signal`, or an entry older than 60 s. Alternative (first plan): a "last seen" baseline plus a change nonce. Rejected (iprev R1 #4): the first observation could hide a PR created before it, and a nonce bumped while the panel showed another session was lost on remount.

### D2. Signature = PR number or `none`
`PrInfo` holds `number, title, state, url` (`src-tauri/src/worktree.rs:1154`); a review-state change shows nothing new, and a GitLab merge request cannot be fetched by `gh`, so both map out of the signature.

### D3. Nergal's own PR actions refresh directly
Ship (`git_ship`, `ShipDialog.tsx:301-314`) and merge (`gh_pr_merge`, `PrViewer.tsx:520`) call a shared `refreshPrInfoAction(sessionId)` (`src/stores/git.ts`), the same one GitPanel uses. Claude Code is usually idle right after them, so the status line would not signal.

### D4. Backend dedups the signature log
A per-session last-signature map in `server.rs` logs at `info` only on change and is cleared at SessionEnd next to `model_cache` (`server.rs:700`). The default filter is `info` (`src-tauri/src/lib.rs:360-361`), so the walk reads it in `~/.cache/nergal/nergal.log` without setup.

### D5. CI poll keys on number and state
The checks effect (`GitPanel.tsx:236-251`) depends on the `PrInfo` object, which every refresh replaces, so it re-ran (and fetched) about every 5 s. It now depends on `prInfo?.number` and `prInfo?.state`, restoring the 20 s cadence of `openspec/specs/ship-flow/spec.md:88`.

### D6. The shared refresh is failure- and order-safe
`refreshPrInfoAction` bumps a per-session sequence number before the fetch and drops the response when a newer fetch for that session started (a Ship refresh must not lose to an older fallback response). On a rejected `invoke`, and on a response that contradicts a `pr:N` signature (null or another number: `pr_status` turns every `gh` failure, network or rate limit included, into `Ok(None)`, `src-tauri/src/worktree.rs:1152-1162`), it keeps the cached `data` and `fetchedAt` and clears `signal`, so the next check retries. A `none` signature with a PR in the response is not a contradiction: `gh pr view <branch>` also returns merged and closed PRs. It writes nothing when the session no longer exists. A merge from the PR viewer deletes the owning session on a successful cleanup (`cleanup_merged_session` only returns `deleted: true`, `src-tauri/src/commands/workspaces_sessions.rs:938`), so the merge path refreshes only in the cleanup's failure branch (`PrViewer.tsx:539-545`), where the session survives. `PR_INFO_TTL_MS` (`src/stores/git.ts:188`) becomes 60 s, the single age `shouldRefreshPr` uses; the panel checks every 10 s, so the worst-case fallback is 70 s and a retry after a failure comes within 10 s.

## Risks / Trade-offs

- [A user's own status line command, which Nergal never overwrites (`src-tauri/src/setup.rs:552-555`)] → no signal; the 60 s fallback applies.
- [The CI indicator updates every 20 s instead of about every 5 s] → matches the spec; visible in the walk.
- [Several sessions on one branch] → each keeps its own signature and cache; one `gh pr view` per session as today.

## iprev cap (3 rounds, medium risk)

Round 3 returned REVISE with two medium and two low findings; all four are folded above (D6 and the spec) without a fourth round, per the round cap. The fixes are mechanical restatements of the rule (contradiction handling, a 10 s check cadence, the merge cleanup branch), covered by the Red tests in task 2.1.

## Revision 1: review follow-ups (2026-11-23, user: fix every finding)

- `gh` is the authority (adversarial #1). Claude Code reports the PR of the checked-out branch; `get_pr_status` asks for the session's stored `worktree_branch` (`src-tauri/src/commands/ship_pr.rs:222-223`). A response with a PR is cached whatever the signature. A failure, or an empty response under a `pr:N` signature, keeps the cached data and records the current signature with `fetchedAt = now`, so the next try waits for the 60 s age instead of retrying every 10 s forever. This replaces D6's "clear the signal" rule.
- In-flight guard (adversarial #2). A per-session in-flight flag; checks skip while it is set, so a slow `gh pr view` (no timeout) cannot stack processes or starve its own writes. Ship bypasses the guard: it writes the `pr_info` that `git_ship` already returns (`ShipDialog.tsx:51`, `:312`) through the same sequence guard, with no `gh` call.
- One age constant: `PR_FALLBACK_MS = 60_000` lives in `src/stores/prSignal.ts`; `PR_INFO_TTL_MS` is removed from `src/stores/git.ts`.
- The listener writes `prSignalMapAtom[sid]` only when the signature changed, so status ticks no longer re-render the git panel.
- The backend signature-log map is cleared at the top of the `SessionEnd` handling, before any early return of the vault finalization.
- An emitted `agent:status-update` payload test pins the `github_pr` key: the payload struct is built by a pure function the test calls.
- The merge path refreshes only when the session survives the cleanup; the spec text says so.
- The "A change while the panel showed another session" scenario gets a named `shouldRefreshPr` case; the GitPanel wiring (effects, intervals) is covered by walks 3.3-3.6, since vitest has no DOM.

## Revision 2: re-review follow-ups (2026-11-23)

- Forced refresh: `refreshPrInfoAction(sessionId, { force: true })` bumps the sequence and skips the in-flight guard; the `finally` clears the in-flight flag only when its own sequence is still current. The PR viewer's merge path uses it, so an older fetch that started before the merge cannot stamp OPEN as fresh.
- Ship with an unknown PR: `create_pr` can return `number: 0` (`src-tauri/src/worktree.rs:1223-1228`); Ship then starts a forced refresh instead of caching `#0`.
- The in-flight map entry is deleted in `finally` rather than set to `false`, and not recreated for a session that was pruned meanwhile.
- A signature change during a fetch leads to a second fetch within 10 s, by design: the response predates the change, so it is stamped with the signature current at fetch start.
- Out of scope, filed separately: with no signature, a `gh` failure reaches the frontend as `Ok(None)` and clears the pill. It predates this change; fixing it means `pr_status` (`src-tauri/src/worktree.rs:1152-1162`) distinguishing "no pull requests found" from other `gh` failures, which also changes the MCP tool `get_pr_status`.
- Walks observe behaviour (pill, log line) rather than counting `get_pr_status` calls, which are not logged; call counts are pinned by the unit tests.
