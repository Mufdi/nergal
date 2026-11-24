Linear: FRI-473

## Why

The git panel calls `get_pr_status` (`gh pr view`, a network round trip) every 5 seconds while it is mounted (`src/components/git/GitPanel.tsx:194-198`). Claude Code's status line input already carries the open pull request for the current branch (`pr.number`, `pr.url`, `pr.review_state`, `pr.kind`; present while a PR or GitLab merge request is open, removed once it merges or closes; code.claude.com/docs/en/statusline). That is a cheap signal that the PR changed.

## What Changes

- The `nergal hook statusline` feeder forwards CC's `pr` object as `github_pr` (`number`, `url`, `review_state`, `kind`, each optional) in the `AgentStatus` payload, and `null` when CC sends no `pr`. The backend passes it through `agent:status-update` and logs the session's PR signature at `info` level once per change.
- The PR signature is the open GitHub PR number, or `none` (no PR, or a GitLab merge request, which `gh pr view` cannot fetch). Review state does not count: `PrInfo` (`number`, `title`, `state`, `url`) shows nothing of it.
- One refresh rule replaces the 5-second poll of `get_pr_status`: refresh when the session's current signature differs from the signature recorded with the cached `PrInfo`, or when the cache is older than 60 seconds. It runs on mount and on every status update, so a PR created while Claude Code was idle, or while another session was shown, still refreshes.
- Ship (`git_ship`) and the PR viewer's merge (`gh_pr_merge`) refresh that session's PR status at once through one shared action, since Claude Code is often idle right after them.
- The CI checks poll keys on the PR number and state instead of the `PrInfo` object, so it keeps its 20-second cadence (`ship-flow` spec) instead of restarting on every 5-second PR refresh. Visible effect: the CI indicator updates every 20 seconds instead of about every 5.
- The 60-second fallback covers what the status line does not signal: title edits, PR changes while Claude Code is idle, users with their own status line command, and Claude Code versions without `pr`.

## Capabilities

### New Capabilities

### Modified Capabilities
- `agent-status-telemetry`: the statusline feeder forwards the PR object.
- `git-panel-v2`: the PR status refreshes on the status-line PR signal plus a 60-second fallback instead of a 5-second poll.

## Impact

- `src-tauri/src/hooks/cli.rs` (`statusline_to_agent_status`), `src-tauri/src/hooks/events.rs` (`AgentStatus`), `src-tauri/src/hooks/server.rs` (`AgentStatusEmit`, `emit_agent_status`).
- `src-tauri/src/agents/{pi/jsonl_tail.rs,opencode/sse_client.rs,codex/rollout_tail.rs}` and the `server.rs` test that build `HookEvent::AgentStatus` literals (`github_pr: None`, mechanical; frozen agents only compile).
- `src/stores/workspace.ts` (`AgentStatus`, `defaultAgentStatus`), `src/stores/hooks.ts` (listener), `src/stores/git.ts` (`PrInfoCacheEntry`, shared refresh action), `src/stores/prSignal.ts` (new helpers), `src/components/git/GitPanel.tsx`, `ShipDialog.tsx`, `PrViewer.tsx`.
- No config, schema or command change.
