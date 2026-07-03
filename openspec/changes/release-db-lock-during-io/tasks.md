## 1. delete_session

- [ ] 1.1 `src-tauri/src/commands.rs:986-1048` — re-scope: extract session row, repo path, obsidian config + run `write_session_log_footer`/`MocBuilder::build` under the first guard (unchanged for now, per design D2a), then **drop the guard** before `remove_worktree`; re-acquire for `db.delete_session`, re-validating the row (missing → Ok).
- [ ] 1.2 (Optional follow-up, design D2b) Split `MocBuilder::build` into DB-read collect + guard-free git-diff build; hoist the diff out of the guard.

## 2. create_pr

- [ ] 2.1 `commands.rs:2586-2620` — extract branch/cwd/repo_path/base under the guard, drop it, then run `list_branches` + `worktree::create_pr` guard-free.

## 3. git_ship

- [ ] 3.1 `commands.rs:2872+` — extract cwd/branch/base under the guard, drop it, then run `worktree::ship` (and the auto-merge tail) guard-free.

## 4. Sweep (visibility only)

- [ ] 4.1 Grep-audit remaining `db.lock()` callers for git/network work under the guard; list findings in the PR description (fix only if trivial; otherwise file follow-ups).

## 5. Verification

- [ ] 5.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 5.2 Manual: run a ship to a real remote while clicking through sessions/panels → UI data stays responsive during the push; delete a worktree session and confirm the sessions list refreshes instantly.
