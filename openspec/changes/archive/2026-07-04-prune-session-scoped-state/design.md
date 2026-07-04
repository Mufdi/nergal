## Context

Session-scoped UI state lives in per-store `Record<sessionId, T>` atoms (~69 `Record<string` atoms in `src/stores/`, most session-keyed). Sessions are permanently destroyed on two paths: the soft-close finalize timer (`sessionTabs.ts:137 → finalizeSessionCloseAction` at 176-184) and the grace-window delete (`pendingDeletes.ts:94-100`, which also calls `invoke("delete_session")`). Neither clears the maps. `terminalService` (outside React) already has a `destroy(sessionId)` lifecycle hook both paths call — the prune slots in beside it.

## Goals / Non-Goals

**Goals:**
- Every permanent session teardown reclaims all session-keyed map entries.
- New session-keyed atoms are prunable by construction, not by remembering a central list.

**Non-Goals:**
- Workspace-keyed maps; persistence-layer cleanup (see the separate `session-child-fk-cascade` change); soft-close-window behavior.

## Decisions

### D1: self-registration at atom definition site

```ts
// src/stores/sessionScope.ts
type SessionKeyedAtom = PrimitiveAtom<Record<string, unknown>>;
const registry: SessionKeyedAtom[] = [];
export function sessionScopedMapAtom<T>(initial: Record<string, T> = {}) {
  const a = atom(initial);
  registry.push(a as SessionKeyedAtom);
  return a;
}
export const pruneSessionStateAction = atom(null, (get, set, sessionId: string) => {
  for (const a of registry) {
    const m = get(a);
    if (sessionId in m) { const { [sessionId]: _drop, ...rest } = m; set(a, rest); }
  }
});
```
Stores switch `atom<Record<string, T>>({})` → `sessionScopedMapAtom<T>()`; the factory registers as a side effect, so the prune list lives at each atom's definition.

**Alternatives considered:**
- *Central manual list in `sessionScope.ts`*: simplest diff, but the list rots — a new session-keyed atom added in six months silently leaks again (the exact failure mode this change exists to end). Rejected.
- *Per-store `pruneX(sessionId)` exports composed in one action*: keeps stores self-contained but re-creates the manual list one level up (the composer must remember every store). Rejected for the same rot.
- *ESLint rule flagging `atom<Record<string,` in stores*: no custom lint infra exists in the repo (no eslint config at all today — see pending `ci-quality-gates` change); a convention comment in `sessionScope.ts` + review is the available guard. Revisit if eslint lands.

### D2: prune at finalize/grace, not at soft-close

Soft-closed sessions can be restored (Ctrl+Shift+T undo path — `hasPendingSessionCloseAtom`, `sessionTabs.ts:189`); their state must survive the window. Prune fires exactly where `terminalService.destroy` already fires: `finalizeSessionCloseAction` (`sessionTabs.ts:182`) and the grace-timer body (`pendingDeletes.ts:97-99`). Workspace grace-delete iterates its sessions and prunes each.

### D3: non-`Record` session state stays out of scope

Some session state is keyed differently (arrays filtered by `sessionId` field, e.g. `pendingSessionClosesAtom`) or lives outside Jotai (`terminalService` containers — already destroyed). The registry covers the `Record<sessionId, T>` shape only; the implementation sweep enumerates every store map and classifies it (registered / derived-excluded / differently-keyed-handled-ad-hoc), recording the classification as a table in the PR description.

## Risks / Trade-offs

- [A "session-keyed" map is actually keyed by something else (e.g. `${workspaceId}:${prNumber}` in PrViewer caches) and gets wrongly registered] → the sweep classifies by key semantics, not by `Record<string` shape; PR table makes the classification reviewable.
- [Factory registration makes atoms non-tree-shakable / registry grows unbounded] → registry size == number of session-keyed atoms (~50), fixed at module load; negligible.
- [A resumed session expects surviving UI state] → resume paths (`request_session_resume`) create fresh session ids for recalled sessions; same-id restore only exists inside the soft-close window, which is untouched (D2).

## Open Questions

- None blocking. If the sweep finds maps whose values hold OS resources (unlisten fns, timers), pruning must also dispose them — handle case-by-case in the sweep table.
