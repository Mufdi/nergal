# Evidence: worktree-root-owned-cleanup

## Planning (2026-11-17)

- User decision: Docker only. pkexec dropped after iprev R1 showed a same-user TOCTOU escalation (a path component swapped between the guard and a root `chown`).
- iprev: R1 REVISE (13), R2 REVISE (9, 1 critical: `git worktree remove --force` drops the registration even when it fails halfway, so the registration check needs a pre-removal snapshot), R3 REVISE (8), R4 REVISE (7, 1 blocking: Linux gating), R5 APPROVED with 5 lows folded into the artifacts.
- Smoke tests on this host (busybox in `alpine:3`): root-owned `0555` directories became user-owned `755` with both the first vector and the hardened one (`--user 0:0 --entrypoint find ... -xdev -type d -exec chown ... -exec chmod u+rwx ...`); the root-owned file inside stayed root-owned and the user deleted the tree. Temp dirs removed.

## Red (test-writer, sonnet)

- 1 locked file `src-tauri/src/worktree_reclaim_tests.rs` (Linux-gated include in `worktree.rs`); hash re-checked by the orchestrator.
- `cargo test --no-run`: only missing locked symbols (`Reclaimer`, `ReclaimIssue`, `ClaimError`, `claim_removal`, `reclaim_target`, `registered_worktrees`, `blocked_dirs`, `unblock_own_dirs`, `remove_worktree_with`, `docker_unblock_args`, `docker_supported`).
- Not unit-covered: non-`PermissionDenied` branch (no portable fixture), caller wiring 3.x (Tauri runtime), `DockerReclaimer` internals (would need Docker) → manual walks 4.2–4.3.

## Green (Builder, sonnet)

- Helpers in a Linux-only module `src-tauri/src/worktree/reclaim.rs` (re-exported); `remove_worktree` keeps the old body in `remove_worktree_plain`; callers moved to `spawn_blocking`, `delete_workspace`/`cleanup_merged_session` now `async`; rollbacks untouched.
- Lock ruling (orchestrator): the Builder's `cargo fmt` reordered one `use` line (`{symlink, MetadataExt, PermissionsExt}` → `{MetadataExt, PermissionsExt, symlink}`, edition-2024 import order). Restoring only that line in a scratch copy reproduces the original lock hash `08313303…` exactly, so the content is otherwise identical. Accepted as formatting and re-locked at `6b40b37f…`.

## Gates after Green (orchestrator run)

| Gate | Result |
|---|---|
| `cargo clippy --all-targets -- -D warnings` | Finished, no warnings |
| `cargo test` | 1003 passed, 0 failed, 1 ignored |
| `cargo fmt --check` | exit 0 |
| `npx tsc --noEmit` | no errors |
| `pnpm test` | 25 files, 266 passed |
| `work-gates.sh 5efed37 worktree-root-owned-cleanup` | Red lock PASS, Deps SKIP, 6a/6b/6c PASS |

## Review (opus, adversarial + security)

- Adversarial: PASS_WITH_NOTES, 1 Important (one-shot reclaim, no retry path). Security: FAIL, 1 Important needs_validation (a sandboxed agent without Docker socket access can race guard → bind mount and get a root `chown` over a host tree) + threat-model compliance gap. Full text summarized in `handoff/REVIEW.md`.
- User decision: keep the session when the reclaim fails (retry path).
- Revision 1 (design.md): inode pin inside the container, recorded-path retry, `ReclaimFailure`, smaller hardening.
- Smoke test of the Revision 1 vector on this host: `dev:ino` mismatch → exit 3 and the root-owned `0555` dir untouched; match → dirs re-owned to `755`; host/container `dev:ino` identical through the bind on ext4; endpoint `unix:///var/run/docker.sock`.

## Revision 1 Red (test-writer, sonnet)

- `worktree_reclaim_tests.rs` updated to the Revision 1 interfaces and extended; new `src/stores/pendingDeletesWorkspace.test.ts` (3 of 4 fail on assertions; the success case passes).
- Added locked seam `reclaim_snapshot(repo_path, recorded)` (recorded path retry), documented in implementation.md.
- `cargo test --no-run`: 19 errors, all missing/changed locked symbols (`ReclaimTarget`, `RECLAIM_SCRIPT`, `ReclaimFailure`, `is_reclaim_failure`, `reclaim_snapshot`, the 5-argument `docker_unblock_args`).
- The test-writer locked with sha256; the orchestrator re-locked with `git hash-object` (the format `work-gates.sh` checks).

## Revision 1 Green (Builder, sonnet)

- Implemented 5.3–5.5. Deviation: `reclaim_target` canonicalizes the worktree first and takes the slug from the canonical path under `<canonical repo>/.worktrees/nergal`; `symlink_metadata` still checks `.worktrees`, `nergal` and the slug lexically under the canonical repo, so a symlinked `.worktrees` or slug is rejected (locked tests). `remove_worktree_with` became `#[cfg(test)]`; production goes through `remove_worktree_snapshotting` with `reclaim_snapshot`.
- Ruling (orchestrator): locked test `docker_step_that_makes_dirs_writable_lets_the_retry_succeed` read `symlink_metadata` of the worktree after a successful removal (always panics). Test defect; the orchestrator moved that line before the removal (one line, no assertion changed) and re-locked at `2cdcded2…`.
- Accepted residue: a recorded path that is itself a symlink to another registered worktree canonicalizes to that worktree (same user, no privilege gain).

### Gates

| Gate | Result |
|---|---|
| `cargo clippy --all-targets -- -D warnings` | no issues |
| `cargo test` | 1019 passed, 0 failed, 1 ignored |
| `cargo fmt --check` | exit 0 |
| `npx tsc --noEmit` | no errors |
| `pnpm test` | 26 files, 270 passed |
| `work-gates.sh 5efed37 worktree-root-owned-cleanup` | 2 Red locks PASS, Deps SKIP, 6a/6b/6c PASS |

## Security re-review of Revision 1 (opus)

- Previous Important 1 (guard→mount TOCTOU): CLOSED by the in-container `dev:ino` check. Previous Important 2 (one-shot reclaim): CLOSED.
- New Important I-1: the partial `delete_workspace` path deleted repo-root sessions while keeping the workspace (breaks the "one repo-root session" invariant). Fixed with `rows_to_delete_on_partial` (Red test written by the orchestrator, locked `f2e902b7…`).
- Nits fixed: `unblock_own_dirs` via `O_PATH|O_NOFOLLOW|O_DIRECTORY` + `fstat` + mode change through `/proc/self/fd/<fd>`; `reclaim_target` records the inode from the same `lstat` that passed the checks; residue bullet added to design Revision 1.
- Nit 4 dropped (orchestrator): the double-failure fallback (`delete_workspace` and `get_workspaces` both failing) keeps restoring tab ids, as the locked test expects; accepted residue.
- Nit 2 (container capabilities) not applied: impact stays inside the container namespace (reviewer: no host effect).

### Gates (final)

| Gate | Result |
|---|---|
| `cargo clippy --all-targets -- -D warnings` | no issues |
| `cargo test` | 1021 passed, 0 failed, 1 ignored |
| `cargo fmt --check` | exit 0 |
| `npx tsc --noEmit` | no errors |
| `pnpm test` | 26 files, 270 passed |
| `work-gates.sh 5efed37 worktree-root-owned-cleanup` | 3 Red locks PASS, Deps SKIP, 6a/6b/6c PASS |

Pending: manual walks 4.2, 4.3, 5.6 on the installed `.deb`.

## Manual walks (2026-11-18)

The user confirmed on the installed `.deb` that the remaining manual walks pass (4.2 = walk B1, 4.3 and 5.6 = walks B2, D2, D3). Marked `[x]`.
