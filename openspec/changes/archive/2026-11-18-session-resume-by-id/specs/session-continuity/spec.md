## ADDED Requirements

### Requirement: Only the top-level agent process writes session identity

For `SessionStart` and `Stop`, the hook CLI SHALL attach `agent_chain`: its ancestor processes from its parent upward (at most 16), each as `{ pid, claude }`, where `claude` marks a Claude Code executable, plus `agent_chain_complete`, true when the walk reached pid 1 or a process without a parent. The backend SHALL find the PTY shell pid of the event's session in the chain and count the runs of consecutive `claude: true` entries below it. The event is nested when that count is two or more. A complete chain without the shell pid is nested: the process does not descend from the session's PTY. An event without a valid chain, with an incomplete chain that lacks the shell pid, or whose session has no known shell pid, is top-level. Session identity writes (conversation id, `Stop` transcript marker, background tasks, last assistant message) SHALL run only for top-level events.

#### Scenario: Agent launched directly by the PTY shell

- **GIVEN** session `s1` whose PTY shell has pid `500`
- **WHEN** a `SessionStart` arrives with chain `[{901, claude}, {500, no}]`
- **THEN** the event SHALL be top-level

#### Scenario: Agent behind a wrapper script

- **GIVEN** session `s1` whose PTY shell has pid `500`
- **WHEN** a `SessionStart` arrives with chain `[{902, claude}, {650, no (bash wrapper)}, {500, no}]`
- **THEN** the event SHALL be top-level

#### Scenario: npx launcher counts as one agent

- **GIVEN** session `s1` whose PTY shell has pid `500`
- **WHEN** a `SessionStart` arrives with chain `[{903, claude (node)}, {700, claude (npx)}, {500, no}]`
- **THEN** the event SHALL be top-level

#### Scenario: Nested claude launched from a tool shell

- **GIVEN** session `s1` whose PTY shell has pid `500`
- **WHEN** a `SessionStart` arrives with chain `[{7400, claude}, {7310, no (tool shell)}, {901, claude}, {500, no}]`
- **THEN** the event SHALL be nested

#### Scenario: Detached process outside the session's PTY

- **GIVEN** session `s1` whose PTY shell has pid `500`
- **WHEN** a `SessionStart` arrives with chain `[{8800, claude}, {8700, no (tmux server)}, {1, no}]` and `agent_chain_complete = true`
- **THEN** the event SHALL be nested

#### Scenario: Truncated chain or no chain

- **WHEN** a `SessionStart` for `s1` arrives with an incomplete chain that does not contain the PTY shell pid, or with no chain
- **THEN** the event SHALL be top-level

#### Scenario: Malformed chain

- **WHEN** `agent_chain` is not an array of objects with an integer `pid` in the `u32` range and a boolean `claude`
- **THEN** the backend SHALL treat the chain as absent

#### Scenario: Nested Stop does not move the transcript marker

- **GIVEN** session `s1` with `session_transcripts.transcript_path = "/p/-repo/aaaa-1111.jsonl"`
- **WHEN** a nested `Stop` arrives for `s1` with `transcript_path = "/p/-repo/zzzz-9999.jsonl"`
- **THEN** `transcript_path` for `s1` SHALL stay `"/p/-repo/aaaa-1111.jsonl"`

### Requirement: Claude Code conversation id is recorded per session

On a top-level `SessionStart` event for a Claude Code session, the backend SHALL store the event's `session_id` in `sessions.agent_internal_session_id` (existing `TEXT NULL` column). A later top-level `SessionStart` with another id SHALL overwrite it. Other agents' sessions SHALL NOT be written.

#### Scenario: SessionStart records the conversation id

- **GIVEN** a Claude Code session row `s1` with `agent_internal_session_id = NULL`
- **WHEN** a top-level `SessionStart` event arrives with `nergal_session_id = "s1"` and `session_id = "aaaa-1111"`
- **THEN** `agent_internal_session_id` for `s1` SHALL be `"aaaa-1111"`

#### Scenario: A new conversation overwrites the id

- **GIVEN** session `s1` with `agent_internal_session_id = "aaaa-1111"`
- **WHEN** a top-level `SessionStart` event arrives with `nergal_session_id = "s1"` and `session_id = "bbbb-2222"` (after `/clear`)
- **THEN** `agent_internal_session_id` for `s1` SHALL be `"bbbb-2222"`

#### Scenario: Stop does not write the id

- **GIVEN** session `s1` with `agent_internal_session_id = "aaaa-1111"`
- **WHEN** a top-level `Stop` event arrives with `nergal_session_id = "s1"` and `session_id = "cccc-3333"`
- **THEN** `agent_internal_session_id` for `s1` SHALL stay `"aaaa-1111"`

#### Scenario: Another agent's session is not written

- **GIVEN** session `p1` whose `agent_id` is `"pi"` and `agent_internal_session_id = "pi-uuid"`
- **WHEN** a top-level `SessionStart` event arrives with `nergal_session_id = "p1"` and `session_id = "dddd-4444"`
- **THEN** `agent_internal_session_id` for `p1` SHALL stay `"pi-uuid"`

### Requirement: Claude Code sessions resume their own conversation

When a Claude Code session reopens in `continue` mode, the backend SHALL pick at most one conversation id and SHALL NOT pass the `continue` sentinel to the adapter. When `agent_internal_session_id` is set, it is the only candidate. When it is `NULL`, the only candidate is the file stem of `session_transcripts.transcript_path`. That candidate is dropped when an older session of the same workspace has the same `transcript_path` or that stem as its id, and when the transcript lives in the repo root's project directory and the session is not the oldest of its workspace. A candidate is used only when `<id>.jsonl` exists in a direct subdirectory of a Claude Code projects directory. Without a usable candidate the session starts a fresh conversation.

The projects directories are `$CLAUDE_CONFIG_DIR/projects` (or `~/.claude/projects`) and the grandparent directory of the stored `transcript_path`.

#### Scenario: Stored id with a transcript on disk

- **GIVEN** session `s1` with `agent_internal_session_id = "aaaa-1111"` and a file `<projects>/-repo/aaaa-1111.jsonl`
- **WHEN** `s1` is reopened in `continue` mode
- **THEN** the agent launch command SHALL contain `--resume aaaa-1111`

#### Scenario: Stored id after /clear without a transcript yet

- **GIVEN** session `s1` with `agent_internal_session_id = "bbbb-2222"`, no file `bbbb-2222.jsonl`, and `transcript_path` ending in `aaaa-1111.jsonl` that exists
- **WHEN** `s1` is reopened in `continue` mode
- **THEN** the launch command SHALL contain neither `--resume` nor `--continue`

#### Scenario: Legacy session falls back to its last Stop transcript

- **GIVEN** session `s2` with `agent_internal_session_id = NULL` and an existing `transcript_path = "<projects>/-repo--worktrees-x/bbbb-2222.jsonl"`
- **WHEN** `s2` is reopened in `continue` mode
- **THEN** the launch command SHALL contain `--resume bbbb-2222`

#### Scenario: A shared legacy transcript goes to the oldest session only

- **GIVEN** sessions `root` (created 100) and `extra` (created 200) in one workspace, both with `agent_internal_session_id = NULL` and the same existing `transcript_path` ending in `cccc-3333.jsonl`
- **WHEN** `extra` is reopened in `continue` mode
- **THEN** its launch command SHALL contain no resume flag
- **AND** reopening `root` SHALL produce `--resume cccc-3333`

#### Scenario: Legacy repo-root transcript stays with the oldest session

- **GIVEN** workspace `w` at `/repo` with sessions `root` (created 100) and `demoted` (created 200), and `demoted` has `agent_internal_session_id = NULL` and an existing `transcript_path = "<projects>/-repo/m1.jsonl"` that no other row references
- **WHEN** `demoted` is reopened in `continue` mode
- **THEN** its launch command SHALL contain no resume flag

#### Scenario: Ids with path characters are rejected

- **GIVEN** session `s1` with `agent_internal_session_id = "../x"`
- **WHEN** `s1` is reopened in `continue` mode
- **THEN** the launch command SHALL contain no resume flag

### Requirement: Lost worktree keeps the session and its conversation

At startup, for each session whose `worktree_path` directory does not exist, the backend SHALL set `worktree_path` and `worktree_branch` to `NULL` and SHALL keep `agent_internal_session_id` and the `session_transcripts` row. The backend SHALL keep the names of the sessions it demoted. The frontend SHALL fetch that list once after its listeners mount and SHALL show one toast that names the sessions. A second fetch SHALL return an empty list.

#### Scenario: Missing worktree becomes a repo-root session with its own id

- **GIVEN** session `s3` named "Fixes" with `worktree_path = "/repo/.worktrees/nergal/x"` (missing on disk) and `agent_internal_session_id = "eeee-5555"`
- **WHEN** the startup reconciliation runs
- **THEN** `worktree_path` and `worktree_branch` of `s3` SHALL be `NULL`
- **AND** `agent_internal_session_id` of `s3` SHALL be `"eeee-5555"`
- **AND** the first fetch of demoted sessions SHALL return `["Fixes"]` and the second SHALL return `[]`

#### Scenario: Present worktrees are untouched

- **GIVEN** every session's `worktree_path` exists on disk
- **WHEN** the startup reconciliation runs
- **THEN** no session row SHALL change and the fetch SHALL return `[]`

### Requirement: The last repo-root session of a workspace cannot be deleted

A repo-root session has `worktree_path = NULL`. `delete_session` and `cleanup_merged_session` SHALL refuse the only repo-root session of its workspace and SHALL change no row. Each SHALL check this before any side effect, and `delete_session` again under the lock that deletes the row. Every other session SHALL be deletable. The sidebar SHALL offer delete on a session when it is a worktree session or when its workspace lists more than one repo-root session. When the backend refuses a delete, the frontend SHALL reload the workspaces and show an error toast.

New sessions keep the current rule: the first session of a workspace is a repo-root session, and later sessions of a git workspace get a worktree.

#### Scenario: Deleting one of two repo-root sessions

- **GIVEN** workspace `w` with repo-root sessions `r1` and `r2`
- **WHEN** `delete_session(r2)` runs
- **THEN** `r2` SHALL be removed and `r1` SHALL remain

#### Scenario: Deleting the only repo-root session is refused

- **GIVEN** workspace `w` with repo-root session `r1` and worktree session `t1`
- **WHEN** `delete_session(r1)` runs
- **THEN** it SHALL return an error and `r1` SHALL remain

#### Scenario: Merged-PR cleanup keeps the only repo-root session

- **GIVEN** workspace `w` with repo-root session `r1` (on a feature branch whose PR was merged) and worktree session `t1`
- **WHEN** `cleanup_merged_session(r1)` runs
- **THEN** it SHALL return an error and `r1` SHALL remain

#### Scenario: Sidebar hides delete on the last repo-root session

- **GIVEN** workspace `w` with repo-root session `r1` and worktree session `t1`
- **WHEN** the sidebar renders `w`
- **THEN** the row of `r1` SHALL have no delete action and the row of `t1` SHALL have one

#### Scenario: Sidebar shows delete on a second repo-root session

- **GIVEN** workspace `w` with repo-root sessions `r1` and `r2`
- **WHEN** the sidebar renders `w`
- **THEN** both rows SHALL have a delete action
