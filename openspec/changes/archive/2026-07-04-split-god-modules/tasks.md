## 1. Sequencing gate

- [x] 1.1 Confirm the pending surgical changes touching these files (`release-db-lock-during-io`, `remove-dead-askuser-slice`, `session-child-fk-cascade`) have landed, or explicitly schedule them after and accept the rebase (design D5).

## 2. db.rs → db/

- [x] 2.1 Create `db/mod.rs`: move struct, `open()` (db.rs:206), migration list/runner, connection + shared row-mapping helpers (`pub(super)` where needed).
- [x] 2.2 Extract one domain per commit — `workspaces`, `sessions`, `transcripts`, `cross_session`, `tasks_costs`, `annotations`, `scratchpad`, `obsidian`, `panels_misc` — each a verbatim-move `impl Database` block; `cargo check && cargo test` green per commit.

## 3. commands.rs → commands/

- [x] 3.1 Create `commands/shared.rs` with the cross-domain `pub(crate)` helpers (`resolve_session_cwd`, `resolve_session_base`, others the compiler surfaces).
- [x] 3.2 Create `commands/mod.rs` with `pub use` re-exports; extract the FIRST domain (`config_misc`, smallest) and verify `generate_handler!` (lib.rs:367) still compiles before continuing.
- [x] 3.3 Extract the remaining domains per design D1's table, one commit each, `cargo check` between; document membership deviations in the PR.

## 4. History

- [x] 4.1 Create `.git-blame-ignore-revs` with every split commit hash; add the one-line usage note (`git config blame.ignoreRevsFile .git-blame-ignore-revs`) to the contributing docs surface.

## 5. Verification

- [x] 5.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [x] 5.2 `npx tsc --noEmit`
- [ ] 5.3 Manual smoke: one command per new domain file (session open, git stage, PR view, conflict view, plan open, annotation save, task list, obsidian note, openspec read, file browse, config save).
- [x] 5.4 **Cross-platform gates green (D6):** confirm the `windows-check` and `macos-cross-check` CI jobs pass on the split PR — the Linux `cargo check` alone cannot prove the moved `#[cfg]`-gated commands/handlers kept their gates + `cfg(not(...))` stubs. A red cross-check here means a gate was dropped in the move.
