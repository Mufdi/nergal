## 1. Unique path + lifecycle

- [ ] 1.1 `src-tauri/src/worktree.rs` `squash_merge`: derive `tmp_dir` as `<repo>/.worktrees/nergal/_merge_tmp.<pid>.<seq>` with a module-local `static SEQ: AtomicU64` (copy the `atomic_write.rs:18-35` technique).
- [ ] 1.2 Remove the call's own temp worktree on ALL exit paths (success + every early error return — wrap in a small guard struct whose `Drop` runs `git worktree remove --force` + `remove_dir_all` best-effort).
- [ ] 1.3 Replace the fixed-path pre-remove (64-70) with a stale-sweep: enumerate `_merge_tmp.*` siblings, remove those whose embedded pid is not alive (plus `git worktree prune`); never remove a live pid's entry. Pid liveness via `sysinfo` (already in `Cargo.toml:107`) — cross-platform, no `cfg` seam, no `/proc`/`libc` (CLAUDE.md cross-platform invariant; the `macos-cross-check`/`windows-check` CI gates must stay green).

## 2. Tests

- [ ] 2.1 Unit test on the path derivation (uniqueness across calls); integration-style test in a temp git repo: run two `squash_merge` calls on distinct source branches sequentially-overlapped (spawn threads with a barrier) and assert both target commits contain the right content and no `_merge_tmp` leftovers remain.

## 3. Verification

- [ ] 3.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 3.2 Manual: ship two worktree sessions of the same workspace back-to-back quickly; both merges land correct content; `.worktrees/nergal/` has no `_merge_tmp*` residue.
