## 1. Shared guard

- [ ] 1.1 Add `resolve_within_base(base: &Path, rel: &str) -> Result<PathBuf, String>` (near `resolve_session_cwd`, `commands.rs:1640`, or a new `src-tauri/src/fs_guard.rs`): up-front reject any `Component::ParentDir`; resolve target (absolute `rel` → target is `rel`; relative → `base.join(rel)`); `dunce::canonicalize` the target, OR walk up to the nearest existing ancestor + lexically append the remaining components for a not-yet-created file/dir (design Decision 2); verify the canonical/composed target `starts_with` canonical base; return the vetted `PathBuf`. Absolute paths are accepted iff they land inside the base (NOT blanket-rejected — design Decision 1).
- [ ] 1.2 Doc comment states: absolute-allowed-iff-inside-base, the symlink-escape property (canonicalize resolves links, `starts_with` catches escapes), the nearest-existing-ancestor walk-up for new targets, and the fail-closed contract.

## 2. Route all seven commands through the guard

- [ ] 2.1 `read_file_content` (`commands.rs:3105`) — replace `cwd.join(&path)` with `resolve_within_base(&cwd, &path)?` (absolute editor-tab paths still accepted when inside cwd).
- [ ] 2.2 `write_file_content` (`commands.rs:3118`) — same, relying on the ancestor walk-up for the not-yet-existing leaf.
- [ ] 2.3 `list_directory` (`commands.rs:2991`) — route through the guard; keep the `path == "."` fast path by passing `"."` through it.
- [ ] 2.4 `save_conflict_resolution` (`commands.rs:2770`) — same.
- [ ] 2.5 `get_file_conflict_versions` (`commands.rs:2751` → `worktree::file_conflict_versions:1067-1068`) — guard the `merged`-side `cwd.join(path)` read AND validate `path` before it is passed to `git show :N:<path>`.
- [ ] 2.6 `read_openspec_artifact` / `write_openspec_artifact` (`commands.rs:1851-1912`) — base is the resolved openspec dir (`commands.rs:1678`); guard BOTH `change_name` (joins at `:1864-1873`, incl. the `archive/` fallback) AND `artifact_path` against that base. `write_openspec_artifact`'s `create_dir_all` (`:1906-1908`) relies on the ancestor walk-up.

## 3. Tests

- [ ] 3.1 Guard unit tests (tempdir): absolute-outside rejected; absolute-inside accepted; `..` rejected; symlink-escape rejected; legitimate nested path accepted; new-file-in-base accepted; new nested dir under base accepted; `change_name`/`artifact_path` traversal rejected.
- [ ] 3.2 Regression tests exercising `read_file_content` (traversal + legit absolute) and `write_openspec_artifact` (new nested spec dir) — assert error + no fs effect on traversal, success on legit.

## 4. Verification

- [ ] 4.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 4.2 `npx tsc --noEmit` (no frontend change expected; confirms nothing broke).
- [ ] 4.3 Manual: file browser + conflict save/versions + openspec artifact read/write on legitimate paths; **open a file tab from Activities and from an `open-file` deep link (absolute paths) and confirm it still reads**; confirm a crafted `../` path (and a `change_name: "../.."`) returns a toast error. Also open a tab whose hook-event path is a real file **outside** the worktree (e.g. `~/.claude/settings.json`) and confirm the guard's fail-closed toast is the intended behavior there (documented accepted regression — the guard confines to the session base by design).
