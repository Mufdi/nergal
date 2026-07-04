## 1. delete_session

- [x] 1.1 `src-tauri/src/commands.rs:986-1048` — re-scope: extract session row, repo path, obsidian config + run `write_session_log_footer`/`MocBuilder::build` under the first guard (unchanged for now, per design D2a), then **drop the guard** before `remove_worktree`; re-acquire for `db.delete_session`, re-validating the row (missing → Ok).
- [~] 1.2 (Optional follow-up, design D2b) Split `MocBuilder::build` into DB-read collect + guard-free git-diff build; hoist the diff out of the guard.
  - DROPPED as moot: review found `MocBuilder::build` does NO git I/O — it parses the session log file (`extract_session_files`), a deliberate choice OVER a git diff (`obsidian/moc.rs` comment). The Obsidian block under the first guard is file+DB I/O only. `delete_session`'s only git op (`remove_worktree`) is already hoisted, so it's fully clean. design.md D2's "MOC does a worktree git diff" premise was stale.

## 2. create_pr

- [x] 2.1 `commands.rs:2586-2620` — extract branch/cwd/repo_path/base under the guard, drop it, then run `list_branches` + `worktree::create_pr` guard-free.

## 3. git_ship

- [x] 3.1 `commands.rs:2872+` — extract cwd/branch/base under the guard, drop it, then run `worktree::ship` (and the auto-merge tail) guard-free.

## 4. Sweep (visibility only)

- [x] 4.1 Grep-audit remaining `db.lock()` callers for git/network work under the guard; list findings in the PR description (fix only if trivial; otherwise file follow-ups).

## 5. Verification

- [x] 5.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 5.2 Manual: run a ship to a real remote while clicking through sessions/panels → UI data stays responsive during the push; delete a worktree session and confirm the sessions list refreshes instantly.

## 6. Follow-up (out of scope here — filed for a later sweep)

- [ ] 6.1 Sweep the remaining `db.lock()`-across-git/network offenders (task 4.1 list). **Highest priority: `delete_workspace`** (MocBuilder + remove_worktree in a LOOP over every session — worse blast radius than the 3 fixed here). Then git_push, get_pr_preview_data, poll_pr_checks (polled → repeat offender), enable_pr_auto_merge, list_prs, get_pr_diff, get_pr_checks, gh_pr_merge, merge_session.
