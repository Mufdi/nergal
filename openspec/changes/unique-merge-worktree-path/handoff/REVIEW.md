# REVIEW — unique-merge-worktree-path

## Reviewer: code-quality-reviewer (single-sequential, sonnet) · 2026-07-03

**Verdict: PASS** — zero findings on worktree.rs.

- RAII guard covers all 5 exit paths (add-fail, merge-conflict, nothing-to-commit,
  commit-fail, update-ref-fail, success) — traced each; the 5 duplicated inline
  `worktree remove --force` cleanups are gone, `tmp_dir` is `.clone()`d into the guard
  so the body keeps its copy.
- Guard on worktree-add-failure: `worktree remove --force` on an unregistered path is a
  benign no-op (exit 128, `let _ =`-discarded); same for `remove_dir_all` on a missing dir.
- Stale-sweep safety (crux): live pid → `continue` (never swept); pid parse
  `strip_prefix("_merge_tmp.").split('.').next().parse::<u32>()`; malformed → left alone
  (conservative, correct asymmetric risk); own dir doesn't exist yet at sweep time.
  Pid-reuse edge is the safe under-sweep direction.
- `pid_is_alive` matches `platform/mod.rs::process_start_time`'s sysinfo idiom — no cfg
  seam, no /proc, no libc; cross-platform CI stays green.
- Tests: `squash_merge_concurrent_calls_do_not_interfere` — real Barrier-synced 2-thread
  run on a real temp git repo, 2 targets, asserts content isolation + no residue (would
  race/corrupt under the old shared path); + sequential-residue + pid-liveness +
  path-uniqueness unit tests. All real git plumbing.

## Non-blocking observation (reviewer)

- A literal `_merge_tmp` dir (the OLD pre-change fixed name) won't match
  `strip_prefix("_merge_tmp.")` so the sweeper skips it — harmless upgrade-residue, out
  of scope, user can `rm -rf` manually.

## Adjacent finding surfaced by the reviewer's `--all-targets` run (NOT this change)

Reviewer's `cargo clippy --all-targets` flagged `db.rs:1906`
(`unnecessary_literal_unwrap`, in the 2.1 fresh-cost test's mirror idiom). db.rs is
untouched here. Root cause: the `ci-quality-gates` `lint-and-test` job runs
`cargo clippy -- -D warnings` WITHOUT `--all-targets`, so test-code lints aren't gated.
→ Fixed in a follow-up commit (lint + `--all-targets` added to the gate). See the
`chore` commit after this change's archive.

## Gates

- Gate 1-3: PASS (clippy -D warnings clean on worktree.rs, 22 worktree tests incl. 4
  new + threaded, fmt clean).
- Gate 6 (scope): 1 file = files_estimate.
