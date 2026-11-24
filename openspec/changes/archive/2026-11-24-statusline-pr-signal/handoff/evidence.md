# Evidence: statusline-pr-signal (FRI-473)

## Planning

- CC status line schema checked against code.claude.com/docs/en/statusline (`pr.number/url/review_state/kind`).
- iprev (opus): R1 REVISE (1 high: Ship/merge staleness; 5 medium; 4 low), R2 REVISE (2 medium, 3 low), R3 REVISE at the 3-round cap (2 medium, 2 low), folded into design D6 and the spec; `iprev-cap` event recorded.

## Baseline

- Branch `mufdidev/fri-473-statusline-pr-signal`, stacked on `mufdidev/fri-507-bracketed-paste-gate` (`60dc567`), gates green there: cargo test 1289, pnpm test 561, clippy/fmt/tsc clean.

## Red (test-writer)

- New: `hooks/statusline_pr_tests.rs` (4 feeder cases), `hooks/pr_signature_tests.rs` (signature + deserialization, 6), `src/stores/prSignal.test.ts` (prSignature, shouldRefreshPr incl. exact-60 s no refresh), `src/stores/prInfoRefresh.test.ts` (7 action cases: write, rejected, null under pr:12, other number, MERGED under none, reverse order, missing session). Includes appended to cli.rs and server.rs. Locked (4 files).
- Red: Rust 8 compile errors (missing `github_pr`, `GithubPr`, `pr_signature`); vitest 2 files failed (7 tests + import failure).

## Green (Builder)

- Rust: `GithubPr` + `github_pr` on `HookEvent::AgentStatus`, `AgentStatusEmit`, emitted payload; `github_pr_status` in the feeder; `github_pr: None` at the other literals; `pr_signature` plus a per-session last-signature `OnceLock<Mutex<HashMap>>` logging `info!` only on change, cleared at SessionEnd beside `model_cache`.
- Frontend: `src/stores/prSignal.ts`; `signal` on `PrInfoCacheEntry`, `prSignalMapAtom`, a private session-scoped sequence map and `refreshPrInfoAction` in `stores/git.ts` (D6); `PR_INFO_TTL_MS` 60 s; `github_pr` on `AgentStatus`; listener writes the signature. GitPanel: `refreshPr` removed, one effect on `[sessionId, signal]` plus a 10 s interval via `shouldRefreshPr`, CI effect keyed on number and state; ShipDialog and PrViewer (cleanup failure branch) call the action.
- Deviation: adding `github_pr` tripped `clippy::large_enum_variant` on `HookEvent`; `prompt_cache` on `HookEvent::AgentStatus` is now `Option<Box<PromptCacheStatus>>` (one existing assertion in `events.rs` tests adapted with `.map(|b| *b)`). Locked tests untouched.
- Verify: clippy --all-targets -D warnings clean; cargo test 1297 passed, 0 failed; rustfmt --edition 2024 --check clean on the 6 edited .rs; tsc clean; pnpm test 54 files, 579 tests passed.
- Orchestrator note: Builder boxed `prompt_cache` on `HookEvent::AgentStatus` (`Option<Box<PromptCacheStatus>>`) to satisfy `clippy::large_enum_variant`, and adjusted one existing (unlocked) assertion in events.rs tests (`prompt_cache.map(|b| *b)`). Accepted: mechanical, no behaviour change.

## Revision 1 Red (test-writer)

- Reviews (REVIEW.md): adversarial 2 Important bugs (contradiction retry loop; no in-flight guard) + 3 compliance + 5 nits; frontend 2 Important + 5 nits. All folded into design Revision 1.
- Tests updated to the Revision 1 rule (failure keeps data + records signal/fetchedAt; other PR number cached; in-flight dedup; setPrInfoFromShip; named remount case; PR_FALLBACK_MS; Rust payload test). Red: vitest 7 failed; Rust E0425 `agent_status_payload`. Relocked (4 files).

## Red conflict ruling (orchestrator)

- The Builder stopped: the locked case "keeps the later call's result when responses resolve in reverse order" (two synchronous refreshes on one session) contradicts the Revision 1 in-flight guard pinned by "does not call invoke again while a refresh is pending". Ruling: the case is a pre-guard leftover; the spec's out-of-order scenario (fallback then Ship) is covered by the setPrInfoFromShip-vs-in-flight case. The test-writer deleted it; relocked. Builder resumes the rest of 3.2.

## Revision 1 Green (Builder)

- clippy `--all-targets -D warnings`: clean. `cargo test`: 1298 passed, 0 failed (plus 1+1 doc/bin passes). `rustfmt --edition 2024 --check src/hooks/server.rs`: ok.
- `npx tsc --noEmit`: clean. `pnpm test`: 54 files, 583 passed.
- `git.ts`: in-flight flag, keep-previous on failure or null under `pr:N`, `setPrInfoFromShip`, `PR_INFO_TTL_MS` removed; `PR_FALLBACK_MS` exported from `prSignal.ts`.
- `ShipDialog` uses `setPrInfoFromShip`; `PrViewer` keeps `refreshPrInfoAction` in the cleanup catch; `hooks.ts` writes the signal only on change.
- `server.rs`: module-level `AgentStatusPayload` + `agent_status_payload`; the `pr_signature_cache` removal is now at the top of `finalize_session_obsidian` (before `claim_finalization`), once.
- GitPanel docblock no longer says PR status is fetched there; `workspace.ts` import order fixed.

## Revision 2 Red (test-writer)

- Re-review: 1 Important (in-flight guard could swallow the post-merge refresh), 5 nits; 4 folded into design Revision 2, 1 out of scope and filed as FRI-606 (`gh` failure reaches the frontend as no PR; changes the MCP tool contract).
- 5 new cases in `prInfoRefresh.test.ts` (forced refresh supersedes; in-flight cleared only by its own call; bare form still works; Ship `#0` forces a refresh; pruned session leaves no in-flight key). Red: 5 failed. Relocked.

## Revision 2 Green (Builder)

- `npx tsc --noEmit` clean; `pnpm test`: 54 files, 588 passed. Rust untouched.
- `git.ts`: `prInFlightMapAtom` exported and holds the owning sequence number; `refreshPrInfoAction` takes a string or `{ sessionId, force }`; force bypasses the in-flight guard and bumps the sequence; `finally` deletes the key only when the entry still equals its own sequence (never sets false, never recreates a pruned session's key); `setPrInfoFromShip` with `number === 0` starts a forced refresh instead of caching.
- `PrViewer.tsx`: the merge cleanup `.catch` calls the forced refresh.

## Final gates (orchestrator)

- clippy clean; `cargo test` 1300 passed, 0 failed; fmt clean; tsc clean; `pnpm test` 588 passed; work-gates: 4 locks unchanged, gitleaks clean. Comment audit: WHY-only.
- Walks 4.3-4.6 wait for the checkpoint-1 `.deb`.
