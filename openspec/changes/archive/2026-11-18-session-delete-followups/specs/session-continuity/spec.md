## ADDED Requirements

### Requirement: Deleting a worktree session drops its branch when it is safe

When `delete_session` removes a worktree session's directory, it SHALL delete the session's `worktree_branch` only when every commit of the branch is reachable from a remote-tracking ref or from the session's `merge_target` (or, when that is unset, the repo root's `HEAD`). A `git` failure while checking SHALL count as not safe. The deletion SHALL be a compare-and-delete on the commit it checked. Before deleting, the branch name, repository and old commit SHALL be appended to a durable record that is never truncated; if that append fails the branch SHALL be kept. A `branch_deleted` notice SHALL carry the restore command `git branch <name> <sha>`. A branch checked out in any worktree, or being rebased or bisected in one, SHALL be kept. A failure to check whether the branch exists SHALL keep it. Otherwise it SHALL keep the branch and return a `branch_kept` notice that names it. A branch that no longer exists is not an error. A session kept after a failed reclaim keeps its branch, and so does a session whose directory could not be removed.

`delete_session` SHALL return typed notices (`worktree_left` for a directory that stayed on disk, `branch_kept` for a kept branch, `branch_deleted` for a deleted branch). The frontend SHALL show `worktree_left` as an error toast titled "Worktree left on disk", `branch_kept` as an info toast titled "Branch kept", and `branch_deleted` as an info toast titled "Branch deleted" whose description contains the restore command.

#### Scenario: Pushed branch is deleted

- **GIVEN** a worktree session on branch `nergal/x` whose tip is also on `origin/nergal/x`
- **WHEN** the session is deleted
- **THEN** the local branch `nergal/x` SHALL no longer exist, a `branch_deleted` notice SHALL contain `git branch nergal/x <sha>`, and the durable record SHALL gain one line with the repository, `nergal/x` and `<sha>`

#### Scenario: Branch merged into the merge target is deleted

- **GIVEN** a worktree session on branch `nergal/y` with `merge_target = "dev"`, and every commit of `nergal/y` is an ancestor of `dev`
- **WHEN** the session is deleted
- **THEN** `nergal/y` SHALL no longer exist

#### Scenario: Branch with local-only commits is kept

- **GIVEN** a worktree session on branch `nergal/z` with one commit that is on no remote-tracking ref and not in its merge target
- **WHEN** the session is deleted
- **THEN** `nergal/z` SHALL still exist and a `branch_kept` notice SHALL name `nergal/z`

#### Scenario: Branch checked out somewhere is kept

- **GIVEN** a worktree session on branch `nergal/w` that is checked out in the repo root (or any other worktree)
- **WHEN** the session is deleted
- **THEN** `nergal/w` SHALL still exist and a `branch_kept` notice SHALL name it

#### Scenario: Unknown merge target is not safe

- **GIVEN** a worktree session with `merge_target = "gone"` (no such branch) and a branch with commits on no remote
- **WHEN** the session is deleted
- **THEN** the branch SHALL be kept

#### Scenario: Branch moved after the check

- **GIVEN** a branch checked at commit `A` that gains commit `B` before the delete
- **WHEN** the compare-and-delete runs against `A`
- **THEN** the branch SHALL be kept

### Requirement: A half-deleted worktree session is pending delete

A worktree session whose `worktree_path` is no longer a working git worktree SHALL be reported with `worktree_pending_delete = true` (computed on every workspace load, never persisted). It is not working when the `.git` entry is missing, or when `.git` is a file whose `gitdir:` target is missing or has no `HEAD` (a failed `git worktree remove --force` drops the registration in the main repository and can leave the `.git` file behind). A `.git` directory counts as working. The sidebar SHALL mark such a session. `start_claude_session` and `spawn_aux_shell` SHALL refuse to start anything for it, whatever path requested the launch, and `start_claude_session` SHALL also refuse a missing working directory. Selecting it SHALL show a panel that explains the worktree is half deleted, shows the manual remedy with the path, and offers only the delete action. A refused launch SHALL remove the empty terminal container and show an error toast.

#### Scenario: Pending delete is computed from disk

- **GIVEN** a session with `worktree_path = "<repo>/.worktrees/nergal/x"` where the directory exists without `.git`
- **WHEN** the workspaces are loaded
- **THEN** that session SHALL have `worktree_pending_delete = true`

#### Scenario: Removed directory is still pending

- **GIVEN** a session with `worktree_path = "<repo>/.worktrees/nergal/x"` where the directory no longer exists
- **WHEN** the workspaces are loaded
- **THEN** that session SHALL have `worktree_pending_delete = true`

#### Scenario: Dropped registration is pending

- **GIVEN** a session whose worktree directory still has a `.git` file, but the `gitdir:` directory it names no longer exists
- **WHEN** the workspaces are loaded
- **THEN** that session SHALL have `worktree_pending_delete = true`

#### Scenario: Healthy worktree is not pending

- **GIVEN** a session whose worktree directory contains a `.git` file whose `gitdir:` directory exists and has a `HEAD`
- **WHEN** the workspaces are loaded
- **THEN** that session SHALL have `worktree_pending_delete = false`

#### Scenario: Pending session never launches the agent

- **GIVEN** a session with `worktree_pending_delete = true`
- **WHEN** `start_claude_session` is called for it (sidebar, shortcut, resume modal, deep link or MCP)
- **THEN** it SHALL return an error and no agent SHALL start

### Requirement: Resume follows Claude Code continuations

When the stored conversation's transcript ends with a `continued-in` record whose `continuedInSessionId` is a valid conversation id with its own transcript, the session SHALL resume that continuation instead, following a chain of continuations to its last link. A missing or invalid continuation, or a cycle, SHALL fall back to the last conversation reached that has a transcript.

#### Scenario: Continued conversation resumes its continuation

- **GIVEN** a session whose stored conversation `A` ends with `continued-in` → `B`, and `B` has a transcript
- **WHEN** the session is launched in `continue` mode
- **THEN** it SHALL resume `B`

### Requirement: The fresh-session flag lasts for one launch

A session created in the current run starts its first agent terminal in `new` mode. The first launch SHALL remove the session from the fresh set, so every later launch of the same session uses `continue` mode, whether it is reopened from the sidebar, a shortcut, the resume modal, or after a refused delete.

#### Scenario: Reopening a session created in this run resumes it

- **GIVEN** a session created in the current run whose first terminal was started and then destroyed
- **WHEN** the user selects the session again
- **THEN** its terminal SHALL start in `continue` mode
