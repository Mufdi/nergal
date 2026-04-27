## ADDED Requirements

### Requirement: Plans archive into main repo before worktree deletion
Before the cleanup capability deletes a session's worktree, the system SHALL copy all `.md` files under `<worktree>/.claude/plans/` into `<main_repo>/.claude/plans/archive/YYYY-MM/<session_id>/`. The archive directory SHALL be created if it does not exist. The archive SHALL be performed as a copy (not a move) so that if the subsequent worktree deletion fails, the original plan files remain intact in the worktree.

#### Scenario: Plans copied into archive before worktree removal
- **WHEN** session-cleanup runs for a session whose worktree contains 3 plan files
- **THEN** the system creates `<main_repo>/.claude/plans/archive/YYYY-MM/<session_id>/` and copies all 3 `.md` files into it; only after the copy succeeds does it proceed to delete the worktree

#### Scenario: Empty plans directory creates no archive
- **WHEN** session-cleanup runs for a session whose worktree has no `.claude/plans/` directory or it is empty
- **THEN** no archive directory is created; cleanup proceeds without error

#### Scenario: Archive collision appends suffix
- **WHEN** the target archive directory `<main_repo>/.claude/plans/archive/YYYY-MM/<session_id>/` already exists
- **THEN** the system appends `-N` (where N is the lowest non-colliding integer) to the directory name and copies into the deduplicated path

### Requirement: Archive failure does not block cleanup
If the plan archive copy fails (filesystem permissions, disk full, etc.), the system SHALL log a warning, surface it as part of the cleanup result's warnings list, and proceed with the rest of the cleanup steps. Plans are best-effort; user-controlled cleanup of the rest of the session must not be blocked.

#### Scenario: Permission denied on archive directory creation
- **WHEN** the system cannot create the archive directory due to filesystem permissions
- **THEN** a warning is added to the cleanup result; worktree, branch, and DB row deletion proceed as normal

### Requirement: Specs and OpenSpec changes are not separately archived
The system SHALL NOT separately archive the session's `openspec/` artifacts. Specs and changes are git-tracked and committed as part of the Ship flow; they ride with the PR and live permanently in the repository history. No additional archive step is required.

#### Scenario: openspec changes are present in the worktree
- **WHEN** session-cleanup runs for a session that committed openspec changes
- **THEN** the cleanup does NOT touch `<worktree>/openspec/`; those files are part of the worktree and disappear with it, but their content is preserved in git history via the merged PR
