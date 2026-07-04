## Context

Six session-scoped filesystem commands trust a caller-supplied path string. The webview IPC boundary is not a trust boundary (any injected script or agent-rendered content can invoke commands), so these must self-defend. The codebase already contains the correct pattern (`is_within_vault`, canonicalize + `starts_with`) applied to vault notes; the fix is to generalize it and apply it to the six unguarded commands.

## Goals

- Confine reads/writes to the resolved base with zero change to legitimate in-worktree access.
- One shared, tested guard rather than six ad-hoc checks (the ad-hoc route is how five of these six drifted from the vault pattern in the first place).
- Fail closed: any ambiguity (canonicalize error, path outside base) → error, no filesystem touch.

## Decision 1: Canonicalize-and-compare; absolute paths allowed iff inside base

**Chosen**: canonicalize both base and target, then `starts_with`. The only up-front lexical pre-check is **reject `..` components** (for clear errors and to avoid canonicalizing obviously-hostile input). Absolute paths are **not** rejected up front.

- **Why absolute must be allowed** (iprev round 1): the editor reads via `read_file_content` with the tab's `filePath` (`CodeEditor.tsx:179`), and tabs opened from Activities (`ActivityDrawer.tsx:239,305`) or the `open-file` deep link carry **absolute** paths. A blanket absolute-reject would break a live, legitimate flow. Confinement is by canonical containment: an absolute path that canonicalizes inside the base is fine; one that escapes is rejected — identical treatment to a relative path, because both end in the same `starts_with(base)` test.
- **Target resolution**: if `rel` is absolute, the target is `rel` itself (no join — `base.join(absolute)` would discard base anyway); if relative, `base.join(rel)`. Then canonicalize + `starts_with`.
- **Alternatives considered**:
  - *Lexical `..` stripping only* (no canonicalize): does not catch symlink escapes. Rejected: leaves a real hole.
  - *Reject absolute up front*: rejected — breaks the editor flow above.
- **Trade-off**: canonicalize requires the path (or an ancestor) to exist. Handled by Decision 2.
- **Precedent**: matches `is_within_vault` (`obsidian/pinned_notes.rs:13`) — same repo, same discipline.

## Decision 2: Not-yet-created targets (new files AND new directories)

`write_file_content` / `save_conflict_resolution` target files that do not exist yet; `write_openspec_artifact` legitimately `create_dir_all`s a not-yet-existing `specs/<cap>/` subtree (`commands.rs:1906-1908`). `canonicalize` fails on a missing leaf *and* on missing intermediate directories, so the earlier "canonicalize the parent" idea is insufficient (the parent may also not exist).

**Chosen**: walk up from the target to the **nearest existing ancestor**, canonicalize that ancestor, then lexically append the remaining (already `..`-free, verified in Decision 1's pre-check) components and confirm the composed path still starts with the canonical base. This admits a legitimate new file or new nested spec directory while still catching traversal (any `..` was already rejected; an absolute escape fails the ancestor's `starts_with`).

- **Alternative**: create-then-canonicalize (touch the file/dir, then validate) — rejected: would create outside the base before validating, the exact thing we prevent.
- **Alternative**: canonicalize only the parent — rejected: fails when the parent dir itself is new (the openspec `create_dir_all` case).

## Decision 3: Guard returns the vetted PathBuf (no re-join at the call site)

The guard returns the canonical `PathBuf` it validated, and callers use that value directly for the fs operation — they do not re-`join` afterward. This closes a TOCTOU/logic gap where a caller could validate one path and operate on another.

## Risks

- **Behavioral regression** if a legitimate flow relied on a symlink pointing outside the worktree (LOW — no known flow does; the vault commands already forbid it). Mitigation: the error is surfaced as a toast, not a silent failure, so any real dependency is immediately visible in a walk.
- **Windows path semantics** — canonicalize differs (UNC prefixes via `dunce` is already a dependency). Mitigation: use `dunce::canonicalize` (already in `Cargo.toml`) for cross-platform stable canonical forms, matching existing usage.

## Migration / rollout

Pure backend change, no schema, no data migration. Ship behind no flag; the guard is strictly more restrictive and legitimate access is unchanged.
