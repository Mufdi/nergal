## 1. The PR object reaches the frontend

- [x] 1.1 Test (Red): Rust, in `#[path]` test files: `statusline_to_agent_status` maps `"pr": {"number": 1234, "url": "u", "review_state": "pending"}` to `github_pr` with those values and `kind: null`; no `pr` → `github_pr: null`; non-object `pr` → `null`; `HookEvent::AgentStatus` deserializes `github_pr` and defaults it to `None`; `pr_signature`: `None` → "none", number 12 → "pr:12", `kind: "mr"` → "none".
- [x] 1.2 Add `GithubPr`, the feeder mapping, the event / emit / payload fields, `github_pr: None` at every other `AgentStatus` literal (including the legacy `StatusLine` arm's `AgentStatusEmit`), and the deduped `info` log with SessionEnd cleanup.

## 2. Signal-driven PR refresh

- [x] 2.1 Test (Red): vitest `prSignature` (null → "none"; `{number: 12}` → "pr:12"; review_state ignored; `kind: "mr"` → "none"); `shouldRefreshPr(entry, signal, now)`: no entry → true; same signal, 10 s old → false; different signal → true; 61 s old → true; `entry.signal` undefined and signal undefined → false unless older than 60 s; `entry.signal` undefined and a defined signal → true. Store tests with a fresh Jotai store and mocked `invoke`: `refreshPrInfoAction("s1")` writes `{ data, fetchedAt, signal }` with the session's current signal; on a rejected `invoke`, and on an `invoke` that resolves `null` (or another PR number) while the signal is `pr:12`, it keeps the previous `data` and `fetchedAt` and leaves `signal` undefined; with signal `none` and a resolved PR (state MERGED) it records the PR; two calls whose `invoke`s resolve in reverse order keep the later call's result; a call for a session missing from the workspaces writes nothing.
- [x] 2.2 Add `src/stores/prSignal.ts`, `prSignalMapAtom`, `signal` on `PrInfoCacheEntry`, `refreshPrInfoAction`, the `github_pr` field on `AgentStatus`, and the listener write.
- [x] 2.3 GitPanel: remove `refreshPr` from the 5 s poll, refresh via `shouldRefreshPr` on mount, on signal change and every 10 s, CI effect keyed on number and state; ShipDialog calls `refreshPrInfoAction` after a successful ship; PrViewer in the `cleanup_merged_session` failure branch; `PR_INFO_TTL_MS` becomes 60 s.

## 3. Review follow-ups (Revision 1)

- [x] 3.1 Test (Red): update `src/stores/prInfoRefresh.test.ts` to the Revision 1 rule (rejected call, or `null` under `pr:12`: previous data kept, `signal` = current signal, `fetchedAt` = now; a different PR number under `pr:12` is cached; a call while one is in flight does nothing; `setPrInfoFromShip` (or the forced variant) writes the given PR without calling `invoke` and wins over an older in-flight fetch); `src/stores/prSignal.test.ts`: a case named for "a change while the panel showed another session" (entry signal `none`, current `pr:12`, 10 s old → true) and `PR_FALLBACK_MS` = 60_000; Rust: the pure payload builder serializes `github_pr`.
- [x] 3.2 Implement Revision 1 (design.md "Revision 1"), including the listener write-on-change, the single constant, the SessionEnd cleanup move, the stale GitPanel comment and the `workspace.ts` import order.

- [x] 3.3 Test (Red): `prInfoRefresh.test.ts`: a forced refresh while one is in flight calls `invoke` again and its result wins over the older response; the older call's `finally` does not clear the forced call's in-flight state; `setPrInfoFromShip` with `prInfo.number === 0` calls `invoke` (forced refresh) instead of caching `#0`; after the session is removed from the workspaces mid-fetch, the in-flight map has no key for it.
- [x] 3.4 Implement Revision 2 (design.md "Revision 2"); PrViewer's merge path uses the forced refresh.

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- [x] 4.2 `~/.claude/skills/work/scripts/work-gates.sh mufdidev/fri-507-bracketed-paste-gate statusline-pr-signal` passes.
- [x] 4.3 Manual walk: open a session whose branch has an open PR; the git panel shows it; `~/.cache/nergal/nergal.log` shows that session's PR signature once, not on every status line tick, and the pill does not flicker while Claude Code works.
- [x] 4.4 Manual walk: Ship a branch from Nergal; the PR pill appears right after the success toast, without waiting (and without a second `gh` call). Merge it from the PR viewer: with a successful cleanup the session closes; if the cleanup fails, the pill shows MERGED right away.
- [x] 4.5 Manual walk: close a PR on GitHub while Claude Code works in that session; the pill changes within a few seconds of its next status update. With Claude Code idle, it changes within about 70 s. The CI indicator updates about every 20 s.
- [x] 4.6 Manual walk: with the session open on another session's tab, close the first session's PR on GitHub while its Claude Code works; switch back: the pill shows the new state on mount. Inside a worktree, check out another branch with an open PR in the terminal: the pill keeps showing the session's stored branch PR and does not flicker.
