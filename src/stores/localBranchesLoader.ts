import { invoke } from "@/lib/tauri";
import type { LocalBranchRow } from "@/lib/localBranches";

const inflight = new Map<string, Promise<LocalBranchRow[]>>();
const queued = new Map<string, Promise<LocalBranchRow[]>>();

function start(workspaceId: string): Promise<LocalBranchRow[]> {
  const p = invoke<LocalBranchRow[]>("list_local_branches", { workspaceId }).finally(() => {
    if (inflight.get(workspaceId) === p) inflight.delete(workspaceId);
  });
  inflight.set(workspaceId, p);
  return p;
}

/// `fresh` is for callers that just mutated branches: a request already in
/// flight may predate the mutation, so it gets one follow-up request instead
/// of being joined.
export function loadLocalBranches(
  workspaceId: string,
  opts: { fresh?: boolean } = {},
): Promise<LocalBranchRow[]> {
  const running = inflight.get(workspaceId);
  if (!running) return start(workspaceId);
  if (!opts.fresh) return running;
  const pending = queued.get(workspaceId);
  if (pending) return pending;
  const next = running
    .catch(() => undefined)
    .then(() => {
      queued.delete(workspaceId);
      return start(workspaceId);
    });
  queued.set(workspaceId, next);
  return next;
}
