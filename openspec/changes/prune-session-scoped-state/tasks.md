## 1. Registry

- [ ] 1.1 Create `src/stores/sessionScope.ts`: `sessionScopedMapAtom<T>()` factory (registers into a module-level list) + `pruneSessionStateAction(sessionId)` write-atom (design D1). Convention comment: every `Record<sessionId, T>` atom must be created via the factory.

## 2. Sweep + registration

- [ ] 2.1 Enumerate every `Record<string, …>` atom in `src/stores/` and classify: session-keyed writable (register), derived read-only (exclude, e.g. `workspace.ts:116` `sessionToWorkspaceMapAtom`), non-session key (exclude, e.g. `${workspaceId}:${prNumber}` caches), differently-shaped session state (handle ad-hoc). Record the classification table in the PR description.
- [ ] 2.2 Convert the session-keyed writable atoms to `sessionScopedMapAtom` across `activity.ts`, `git.ts`, `clickup.ts`, `linear.ts`, `plan.ts`, `rightPanel.ts`, `quake.ts`, `scratchpad.ts`, and any others the sweep finds. For entries holding disposables (unlisten fns, timers), dispose in the prune path.

## 3. Wiring

- [ ] 3.1 `src/stores/sessionTabs.ts` — call `pruneSessionStateAction` from `finalizeSessionCloseAction` (176-184), after `terminalService.destroy`.
- [ ] 3.2 `src/stores/pendingDeletes.ts` — call it in the session grace-timer body (94-100) and for each session of a workspace in `deleteWorkspaceWithGraceAction`.

## 4. Verification

- [ ] 4.1 `npx tsc --noEmit`
- [ ] 4.2 Manual: open a worktree session, generate activity/git/plan state, close it, wait out the 5s window, and confirm via React devtools (or a debug dump of the registry) that no map retains the session id; undo-close within the window keeps state.
