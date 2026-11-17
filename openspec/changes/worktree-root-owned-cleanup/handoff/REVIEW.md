---
type: review
feature: worktree-root-owned-cleanup
reviewer: adversarial + security (opus)
reviewed_at: 2026-11-17
verdict: FAIL
---

# Review — worktree-root-owned-cleanup

## Diff scope

- Files changed: 4 · LOC: +1359 / -53

## Adversarial — PASS_WITH_NOTES

- [Important] `src-tauri/src/worktree/reclaim.rs:502`, `src-tauri/src/commands/workspaces_sessions.rs:575-577` — one-shot reclaim: the first attempt drops the git registration and the commands delete the row anyway, so any reclaim failure leaves an orphan the guard later rejects. → Revision 1 (keep the session, recorded-path retry).
- [Nit] `reclaim.rs:323-328` — unbounded reader join after kill when `docker` is a non-`exec` wrapper. → process group kill + 5 s join limit.
- [Nit] `reclaim.rs:553-558` — `reclaim ineffective` loses the manual remedy on a non-`PermissionDenied` retry error. → always include it.
- [Nit] design text drift (`InProgress` does not prune; D5 off-Linux wording); tasks not marked. → Revision 1 text, task marks.
- Verified: step order matches the flowchart; rollbacks byte-identical; no guard across `.await`; `refuse_last_root_session` before side effects; cfg gating sound; child always reaped.

## Security — FAIL (needs_validation)

- [Important] `reclaim.rs:544-548`, `:155-158` — guard→mount TOCTOU: a sandboxed agent (no Docker socket) can force the Docker branch and swap the worktree for a symlink to `/etc`; dockerd mounts it and the container re-owns it. Boundary: sandboxed agent → root through Nergal as a confused deputy. → Revision 1: `dev:ino` checked inside the container (bind mount pins the inode).
- [Important] design D2 / proposal threat model omits the sandboxed child process. → Revision 1 text.
- [Nit] `unblock_own_dirs` mode replay can grant `0777` to a swapped symlink target. → `(mode & 0o1755) | 0o700`.
- [Nit] `chown` without `-h` in the container. → `chown -h`.
- [Nit] endpoint re-resolved at `docker run`. → `-H <endpoint>`.

## Verdict

FAIL — Revision 1 addresses both Important findings; re-review after the fix.

## Re-review of Revision 1 (opus, security + adversarial)

- Previous Important findings: both CLOSED (`dev:ino` pin inside the container; session kept with recorded-path retry).
- [Important] I-1 partial `delete_workspace` deleted repo-root sessions → fixed (`rows_to_delete_on_partial`, locked test).
- Nits: `O_NOFOLLOW` mode change (fixed), guard inode from the checked `lstat` (fixed), container capabilities (not applied, container-only), double-failure fallback tabs (accepted), mode residue on kept worktrees (documented).

Final verdict: PASS_WITH_NOTES.
