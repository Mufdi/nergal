## Why

A `nergal://` deep link — openable from any webpage, email, or chat message the user clicks — can spawn an agent CLI session and **auto-submit an attacker-authored prompt** with no confirmation inside Nergal:

- `handleSessionNew` (`deepLinkRouter.ts:145-199`) resolves/creates a workspace from the URL's `cwd`, calls `create_session`, then `queue_session_prompt` with the URL's `prompt` param, then activates the session. There is no `confirm()` on this path.
- The queued prompt is **auto-submitted on spawn**, not pre-filled for review: `pty.rs:626-633` documents the adapter folding it into the launch command "so it submits on spawn without a timing race."
- `create_workspace` (`commands.rs:748-753`) validates only `path.is_dir()` — any existing directory (including `$HOME`) is a valid target, so the auto-prompted agent can be rooted anywhere on disk (SECURITY-02, amplifier).
- Sessions support a persisted `--dangerously-skip-permissions` launch option (`agents/claude_code/adapter.rs:183-212`). Where a workspace/session has it configured, this path is effectively **one click → unattended command execution**, gated only by the OS/browser's dismissible "open in app?" protocol prompt.

The OS-level protocol prompt is not a sufficient boundary: users routinely tick "always allow" for a handler they installed, and it conveys nothing about *what* the link will do (which directory, what instruction).

## What Changes

- **In-app confirmation gate before any deep-link-initiated session spawn.** Before `create_session` + `queue_session_prompt` in `handleSessionNew`, show a confirmation (a dedicated variant of the `ConfirmHost` pattern — see the safe-default note below) that displays: the **resolved `cwd`** (absolute), a **"not a git repository" warning** when applicable, and the **literal prompt text** (escaped via the shared `escapeHtml` from the sibling `shared-escape-html-util` change). The spawn proceeds only on explicit confirm.
- **Gate on `handleOpenFile` too** (`deepLinkRouter.ts:230-292`). Correction (iprev round 1): `open-file` is NOT a benign tab-open — when the path is not a known workspace it calls `create_workspace` (`:240`), creates a session (`:266`), and activates it, and the code's own comment (`:285`) states "Activating the session spawns its PTY (empty session starts the agent)." So a crafted `open-file` link registers an arbitrary repo and spawns an agent there (with any persisted launch options, including `--dangerously-skip-permissions`). Its workspace/session-creation branch requires the same gate.
- **Gate on the workspace-creation branch of `handleOpenWorkspace`** (`deepLinkRouter.ts:56+`) when the path is not an already-known workspace — creating a workspace from an untrusted link is itself a state change worth confirming.
- **Pre-creation git-repo probe (not the post-creation return field).** Correction (iprev round 1): `create_workspace` already returns `is_git` (`commands.rs:773`), but that is only available *after* the workspace exists — too late to warn before creating, and a decline must create nothing. So add/reuse a **probe command** (e.g. `resolve_repo_root`/a lightweight `probe_workspace_path`) the router calls *before* `create_workspace` to drive the "not a git repository" warning, so the gate decides pre-creation.
- **No auto-submit of a deep-link prompt without confirmation** — the confirmation is the boundary; the existing "stash before activate" ordering (`deepLinkRouter.ts:181-185`) is preserved *after* confirm, so legitimate confirmed links behave exactly as today.

Deliberately **not** changing: the known-workspace `session` route that focuses an existing session without creating a workspace or injecting a prompt.

## Capabilities

### Modified Capabilities

- `session-launch-options`: adds the requirement that a session spawned from an **external deep link** carrying a prompt requires explicit in-app confirmation before the prompt is queued/submitted, and that the confirmation surfaces the resolved cwd + prompt. (The existing manual-launch flows, where the user is already in-app choosing options, are unchanged.)

- **Safe-default confirm variant (not cosmetic — iprev round 1).** `ConfirmHost` steals focus on open and confirms on Enter in the capture phase (`ConfirmHost.tsx:36-43,65-69`). A security dialog triggered by an *external* link whose default keyboard action is "proceed" can be accepted by a stray Enter while the user is typing in the terminal — exactly the timing an attacker controls. The deep-link confirm variant SHALL therefore **not** default to proceed on Enter: either the destructive/proceed action is not the Enter default (Enter cancels or is inert), or the confirm requires an explicit pointer/keyed action on the proceed control. This is a required property of the change, not a deferred polish.

## Impact

- **`src/lib/deepLinkRouter.ts`**: confirmation gate in `handleSessionNew`, `handleOpenFile` (workspace/session-creation branch), and the workspace-creation branch of `handleOpenWorkspace`; render prompt/cwd via the shared `escapeHtml` helper; call the pre-creation probe before `create_workspace`.
- **`src-tauri/src/commands.rs`**: a pre-creation probe (`resolve_repo_root` reuse or a lightweight `probe_workspace_path`) exposing `is_git_repo` for a path *without* creating a workspace; no behavioral block added.
- **`src/components/ui/ConfirmHost.tsx` / `lib/confirm.ts`**: a variant (or option) whose Enter default is not "proceed", used for the deep-link gate.
- **Tests**: `handleSessionNew` and `handleOpenFile` do not call `create_session`/`create_workspace`/`queue_session_prompt` until confirm resolves; a rejected confirm spawns/creates nothing; a second deep link arriving while the first confirm is pending does not bypass or coalesce (the `confirm.ts` queue is honored).
- **Out of scope**: removing `--dangerously-skip-permissions` as a feature (separate policy question); the CSP hardening (sibling change); the fs-command hardening (sibling change) — together these narrow the blast radius but this change closes the specific 1-click-spawn vector.
