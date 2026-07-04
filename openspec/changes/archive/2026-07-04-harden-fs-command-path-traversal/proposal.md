## Why

Seven Tauri filesystem commands join a **caller-supplied path string** onto the session's cwd (or the openspec dir) with no sanitization, then read or write the result directly:

- `read_file_content` — `commands.rs:3105` (`cwd.join(&path)` → `fs::read_to_string`)
- `write_file_content` — `commands.rs:3118` (`cwd.join(&path)` → `fs::write`)
- `list_directory` — `commands.rs:2991` (`cwd.join(&path)` → `fs::read_dir`)
- `save_conflict_resolution` — `commands.rs:2770` (`cwd.join(&path)` → write)
- `get_file_conflict_versions` — `commands.rs:2751` → `worktree::file_conflict_versions:1067-1068` (`cwd.join(path)` → `read_to_string` for the `merged` side, and `path` passed raw to `git show :N:<path>`)
- `read_openspec_artifact` / `write_openspec_artifact` — around `commands.rs:1851-1912`; **two** caller-controlled inputs each — `change_name` (joined at `:1864-1873`, including the `archive/` fallback) and `artifact_path` — both joined under the resolved openspec dir.

`Path::join` on an **absolute** path silently discards the base, and `..` components pass straight through. A caller of `path: "/etc/passwd"` reads an arbitrary file; `path: "../../../.ssh/id_rsa"` escapes the worktree; the write variants get arbitrary-file **write**; `change_name: "../../.."` escapes the openspec dir. The caller here is the webview IPC surface — reachable by any injected script (see the CSP gap and the `dangerouslySetInnerHTML`/deep-link surfaces audited alongside this) and by content the agent renders.

**Note (iprev round 1): absolute paths are legitimately used and must not be blanket-rejected.** `CodeEditor.tsx:179` calls `read_file_content` with the tab's `filePath`, and both the `open-file` deep link (`deepLinkRouter.ts`) and `ActivityDrawer.tsx:239,305` (hook-event paths) store **absolute** paths in that tab. The guard therefore confines by *canonical containment*, not by rejecting absolute paths outright: an absolute path that canonicalizes inside the base is accepted; one that escapes is rejected.

This is a **consistency gap, not an unknown risk**: the vault-note commands in the same file already guard exactly this — `is_within_vault` (`obsidian/pinned_notes.rs:13`) canonicalizes both sides and checks `starts_with`, and `resolve_note_in_vault` rejects traversal. The team knows the pattern; six commands predate or missed it.

## What Changes

- **New shared guard `resolve_within_base(base: &Path, rel: &str) -> Result<PathBuf, String>`** (backend helper, colocated with `resolve_session_cwd` in `commands.rs` or a small `fs_guard` module). Contract: reject any `rel` containing a `ParentDir` (`..`) component up front; resolve the target — if `rel` is absolute, the target IS `rel` (do not join); if relative, `base.join(rel)`; canonicalize the target (or the nearest existing ancestor for a not-yet-created path — see below); verify the canonical target `starts_with` the canonicalized `base`; return the vetted `PathBuf` or a `String` error. **Absolute paths are allowed iff they canonicalize inside the base** (blanket-rejecting them breaks the editor-tab flow). Mirrors `is_within_vault`'s canonicalize+`starts_with` discipline but returns the path so callers use exactly the vetted value (no TOCTOU re-join).
- **Route all seven commands through the guard** — replace each raw `cwd.join(&path)` (and the openspec-dir joins, including `get_file_conflict_versions`'s `merged`-side read and the `git show :N:<path>` argument) with `resolve_within_base(&base, &rel)?`. For `list_directory`'s `path == "."` fast path, pass `"."` through the guard (canonicalizes to `cwd`, stays inside). For `read_openspec_artifact`/`write_openspec_artifact`, guard **both** `change_name` and `artifact_path` against the openspec base (a `..` in either escapes).
- **Non-existent-target handling (new files AND new dirs)** — `write_file_content` / `save_conflict_resolution` target a not-yet-created file; `write_openspec_artifact` legitimately `create_dir_all`s a not-yet-existing `specs/<cap>/` subtree (`commands.rs:1906-1908`). Canonicalize fails on a missing leaf *and* on missing intermediate dirs. The guard therefore **walks up to the nearest existing ancestor**, canonicalizes that, and lexically validates the remaining (already `..`-free) components stay within — so a legitimate new file or new nested spec dir resolves, while traversal is still caught.
- **Symlink stance** — canonicalize resolves symlinks, so a symlink inside the worktree pointing outside is caught by the `starts_with` check (fail-closed). Documented in the guard's doc comment as an intentional property.

## Capabilities

### New Capabilities

- `session-fs-access` — the contract that every session-scoped filesystem command confines reads/writes to the session's resolved cwd (or the resolved openspec dir), rejecting absolute paths, `..` traversal, and symlink escapes. Formalizes an invariant currently honored only by the vault commands.

### Modified Capabilities

(none — no existing spec owns these six commands today; this creates the missing contract.)

## Impact

- **`src-tauri/src/commands.rs`**: new `resolve_within_base` helper; call sites `read_file_content`, `write_file_content`, `list_directory`, `save_conflict_resolution`, `read_openspec_artifact` (both `change_name` + `artifact_path`), `write_openspec_artifact` (same) routed through it.
- **`src-tauri/src/worktree.rs`**: `file_conflict_versions` (`:1067-1068`) — guard the `merged`-side `cwd.join(path)` read and the `path` passed to `git show :N:<path>` (reachable via `get_file_conflict_versions`, `commands.rs:2751`).
- **Behavior**: legitimate in-worktree relative **and absolute** paths (editor tabs opened from Activities / `open-file`) are unaffected — they canonicalize inside the base. Only paths that escape the base return an error string to the frontend instead of touching the filesystem. The frontend already surfaces command errors as toasts, so no new UI is required.
- **Tests**: unit tests for the guard (absolute-inside-base accepted; absolute-outside-base rejected; `..` rejected; symlink-escape rejected; legitimate nested path accepted; new-file-in-base accepted; new nested dir under base accepted; `change_name` traversal rejected).
- **Out of scope**: the vault commands (already guarded); the CSP hardening and deep-link confirmation (separate Band-A changes that reduce *who can call* these commands — this change hardens the commands themselves regardless of caller).
