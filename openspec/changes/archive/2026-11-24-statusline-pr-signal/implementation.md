# Implementation Plan: statusline-pr-signal

> Grounded in current codebase, symbols verified 2026-11-23.

## Verified codebase facts (do not re-assume)

- CC status line input: `pr.number`, `pr.url`, `pr.review_state` (optional), `pr.kind` (`mr` for GitLab, absent for GitHub); `pr` absent outside a repo, before a PR is found, and after merge/close (code.claude.com/docs/en/statusline).
- Feeder `statusline_to_agent_status` (`src-tauri/src/hooks/cli.rs:719-755`); nested-object pattern `prompt_cache_status` (`:694-717`).
- `HookEvent::AgentStatus` (`src-tauri/src/hooks/events.rs:222-248`), `PromptCacheStatus` at `:7`. Other literal builders: `src-tauri/src/agents/pi/jsonl_tail.rs:210`, `src-tauri/src/agents/opencode/sse_client.rs:253`, `src-tauri/src/agents/codex/rollout_tail.rs:120` (+ tests `:425`, `:444`, `:464`), `src-tauri/src/hooks/server.rs:2065` (test). The server arm destructures every field without `..` (`server.rs:1418-1432`).
- `AgentStatusEmit` (`server.rs:1523-1537`), `emit_agent_status` (`:1542+`), legacy `StatusLine` arm (`:1381-1416`); `model_cache` cleared at SessionEnd (`server.rs:700`). Log filter defaults to `info` (`src-tauri/src/lib.rs:360-361`).
- Frontend `AgentStatus` and `defaultAgentStatus`: `src/stores/workspace.ts:196-224`, snake_case keys; the listener spreads the raw payload (`src/stores/hooks.ts:350-356`), so the field is `github_pr`.
- `PrInfoCacheEntry` + `prInfoMapAtom` (`sessionScopedMapAtom`): `src/stores/git.ts:167-171`; `PR_INFO_TTL_MS` `:188`; `refreshGitInfoAtom` pattern `:49-56`. Session-scoped maps register for pruning via `sessionScopedMapAtom` (`src/stores/sessionScope.ts:3-7`).
- GitPanel: `refreshPr` `:136-144`, mount effect + 5 s poll `:175-204`, CI checks effect `:236-251`, `prInfo` from the map `:116`.
- Ship `git_ship` then `refreshGit` (`src/components/git/ShipDialog.tsx:301-314`); merge `gh_pr_merge` (`src/components/git/PrViewer.tsx:520`, `owningSessionId` available).

## Execution order

1. Rust: `GithubPr { number: Option<u64>, url, review_state, kind: Option<String> }` next to `PromptCacheStatus`; `github_pr: Option<GithubPr>` (`serde(default)`) on `HookEvent::AgentStatus`, `AgentStatusEmit`, `AgentStatusPayload`; `github_pr_status(cc)` in the feeder; `github_pr: None` at every other literal; a per-session signature map in `server.rs` with an `info!` on change and cleanup at SessionEnd (a pure `pr_signature(&Option<GithubPr>) -> String` mirrors the frontend rule).
2. Frontend store: `github_pr: GithubPr | null` on `AgentStatus` and `null` in `defaultAgentStatus`; `src/stores/prSignal.ts` with `prSignature(githubPr)` and `shouldRefreshPr(entry, signal, now)`; `prSignalMapAtom = sessionScopedMapAtom<string>()` written by the listener; `signal` on `PrInfoCacheEntry`; `refreshPrInfoAction(sessionId)` in `src/stores/git.ts` per design D6: per-session sequence counter (a plain session-scoped map atom), drop stale responses, keep `data`/`fetchedAt` and clear `signal` on failure or on a response that contradicts a `pr:N` signature, skip the write when the session is gone (check `workspacesAtom`). `PR_INFO_TTL_MS` → `60_000`, the age `shouldRefreshPr` uses; GitPanel checks every 10 s.
3. GitPanel: drop `refreshPr` from the 5 s poll; one effect on `[sessionId, current signal]` and a 10 s interval both run `shouldRefreshPr` then the shared action; CI effect deps `[prInfo?.number, prInfo?.state, sessionId]`.
4. Ship calls `refreshPrInfoAction` after success; the PR viewer's merge calls it for the owning session in the `cleanup_merged_session` failure branch only.

## Per-phase risk

- `shouldRefreshPr` must treat `entry.signal === undefined` (entries fetched before any status update) as "no recorded signal": refresh only when a defined current signal exists and differs, so sessions without a status line do not refresh on every check.
- The CI effect must still stop when the PR leaves OPEN.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, `npx tsc --noEmit`, `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh mufdidev/fri-507-bracketed-paste-gate statusline-pr-signal`.
- Manual walk on the checkpoint-1 `.deb` (tasks group 3).
