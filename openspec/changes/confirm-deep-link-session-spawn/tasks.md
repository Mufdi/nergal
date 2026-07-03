## 1. Backend: pre-creation git-repo probe

- [ ] 1.1 Add/reuse a side-effect-free probe the router calls BEFORE `create_workspace`: `resolve_repo_root(path)` reuse, or a lightweight `probe_workspace_path(path) -> { is_dir, is_git_repo, resolved }`. Do NOT rely on `create_workspace`'s post-creation `is_git` (`commands.rs:773`) — the gate must warn before anything is created. Additive; no block.

## 2. Frontend: confirmation gate (all creation routes)

- [ ] 2.1 `handleSessionNew` (`deepLinkRouter.ts:145-199`) — before `create_session`, call the probe, then `await` the deep-link confirm variant showing resolved `cwd`, the git-repo warning when applicable, and the escaped prompt text. Proceed only on confirm; on decline, toast "Deep link cancelled" and return without creating session/workspace/prompt.
- [ ] 2.2 `handleOpenFile` (`deepLinkRouter.ts:230-292`) — gate the workspace/session-creation branch (`:240` create_workspace, `:266` create_session, `:285` activate/spawn) behind the same confirm for an unknown path. A known-workspace file open (existing session focus) is unchanged.
- [ ] 2.3 Workspace-creation branch of `handleOpenWorkspace` (`deepLinkRouter.ts:56+`) — confirm before `create_workspace` for an unknown path.
- [ ] 2.4 Escape the prompt/cwd via the shared `escapeHtml` helper (sibling `shared-escape-html-util`).
- [ ] 2.5 Preserve the existing stash-before-activate ordering after confirm (`deepLinkRouter.ts:181-185`).

## 3. Safe-default confirm variant

- [ ] 3.1 Add a `ConfirmHost`/`lib/confirm.ts` variant (or option) whose Enter default is NOT "proceed" (Enter cancels/inert; proceed requires an explicit pointer/keyed action). Use it for the deep-link gate. Reference `ConfirmHost.tsx:36-43,65-69` (current focus-steal + Enter-in-capture behavior).

## 4. Tests

- [ ] 4.1 `handleSessionNew` and `handleOpenFile` do not call `create_session`/`create_workspace`/`queue_session_prompt` until the confirm resolves; a declined confirm results in zero backend calls.
- [ ] 4.2 `handleOpenWorkspace` workspace-creation branch confirms before `create_workspace`.
- [ ] 4.3 A stray Enter does not confirm the deep-link variant; a second deep link while one confirm is pending is queued (not coalesced/auto-confirmed).

## 5. Verification

- [ ] 5.1 `npx tsc --noEmit`.
- [ ] 5.2 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` (for the probe).
- [ ] 5.3 Manual: `nergal://session/new?cwd=<dir>&prompt=<text>` → gated (cwd + prompt shown; decline spawns nothing; confirm spawns + auto-submits). `nergal://open-file?path=<unknown>/f.rs` → gated (decline creates nothing). Known-workspace file open → no confirmation. Confirm the terminal's Enter does not accept the dialog.
