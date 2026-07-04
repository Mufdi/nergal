# ship-flow

Concurrent squash-merges use isolated temporary worktrees.

## ADDED Requirements

### Requirement: Merge temp worktrees are per-call unique

`squash_merge` SHALL create its temporary detached worktree at a per-call unique path (pid + process-local sequence suffix), SHALL remove its own temp worktree on both success and error exits, and SHALL NOT remove another in-flight call's temp worktree. Stale leftovers from crashed runs SHALL be cleaned opportunistically without touching live ones.

#### Scenario: two concurrent merges do not interfere

- **GIVEN** two ship operations targeting the same repository run concurrently
- **WHEN** both reach `squash_merge`
- **THEN** each uses its own temp worktree path and both merges complete with the correct per-branch content

#### Scenario: temp worktree is cleaned on exit

- **WHEN** a `squash_merge` completes (success or error)
- **THEN** its temp worktree directory is removed and `git worktree list` does not accumulate `_merge_tmp` entries

#### Scenario: crashed leftover does not block the next merge

- **GIVEN** a previous process died leaving a `_merge_tmp.*` worktree
- **WHEN** a new merge runs
- **THEN** the stale leftover is swept (it belongs to no live merge) and the new merge proceeds
