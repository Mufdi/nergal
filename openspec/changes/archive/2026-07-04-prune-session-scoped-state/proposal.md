## Why

Per-session Jotai maps are never pruned when a session is permanently closed, so a long-lived app instance that spawns many worktree sessions keeps an in-memory footprint for every session it has ever opened. `finalizeSessionCloseAction` (`src/stores/sessionTabs.ts:176-184`, the only path after the 5s soft-close window) calls only `terminalService.destroy(sessionId)` and removes the `pendingSessionClosesAtom` entry — it touches none of the dozens of session-keyed `Record<string, …>` maps (~69 `Record<string` atoms across `src/stores/`, the majority session-keyed; spot-verified: `activity.ts:7` `activityMapAtom` — up to 200 entries per session — `git.ts:37` and its ~14 sibling maps, `clickup.ts`, `linear.ts`, `plan.ts:9` and siblings, `rightPanel.ts`, `quake.ts`, `scratchpad.ts`). The grace-deletion path (`src/stores/pendingDeletes.ts:98`, which invokes `delete_session`) has the same gap.

## What Changes

- **A session-atom registry + shared prune action**: session-keyed writable atoms register themselves (module-level `registerSessionScopedAtom(atom)` at definition site); a new `pruneSessionStateAction(sessionId)` iterates the registry and deletes the session's key from each map.
- **Wire the prune into both permanent-delete paths**: `finalizeSessionCloseAction` (`sessionTabs.ts:176`) and the grace-timer callback in `deleteSessionWithGraceAction` (`pendingDeletes.ts:94-100`); the workspace grace-delete (`deleteWorkspaceWithGraceAction`) prunes each of the workspace's sessions.
- **Registry over manual list** so new session-keyed atoms cannot silently opt out (the design's registration-at-definition pattern makes the prune list live next to each atom; a manual central list rots — see design.md D1).
- **Exclusions**: derived read-only atoms (e.g. `sessionToWorkspaceMapAtom`, `workspace.ts:116`) recompute from their sources and hold no per-session storage — not registered.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `tab-system`: permanently closing a session reclaims all its session-keyed in-memory state (the close lifecycle's final step is a registered-atom prune).

## Impact

- **New**: `src/stores/sessionScope.ts` (registry + prune action, ~40 LOC).
- **Modified**: `src/stores/sessionTabs.ts` (finalize wiring), `src/stores/pendingDeletes.ts` (grace-delete wiring), and each store defining session-keyed maps (`activity.ts`, `git.ts`, `clickup.ts`, `linear.ts`, `plan.ts`, `rightPanel.ts`, `quake.ts`, `scratchpad.ts`, plus any others found in the implementation sweep) — one `registerSessionScopedAtom(...)` line per atom.
- **Risk**: LOW-MEDIUM — pruning a session that later "reappears" (e.g. resume of a recently-closed session) starts it with empty panel state, which is the same as a fresh app launch for that session; no data loss (durable state lives in SQLite). The soft-close window is untouched (prune fires only at finalize).
- **Out of scope**: pruning workspace-keyed maps (smaller cardinality, workspaces are few); backend memory; changing the soft-close/grace mechanics.
