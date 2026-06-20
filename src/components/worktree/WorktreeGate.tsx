import { useAtomValue, useStore } from "jotai";
import { useMemo, useState } from "react";
import { AlertTriangle, Check, GitBranch, Pencil, X } from "lucide-react";
import {
  worktreeRequestsAtom,
  approveWorktreeRequest,
  denyWorktreeRequest,
  type WorktreeRequestView,
} from "@/stores/worktreeGate";
import { workspacesAtom } from "@/stores/workspace";

function formatBytes(n: number): string {
  if (n <= 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${units[i]}`;
}

/// Native, structurally un-bypassable approval gate for agent-spawned worktree
/// sessions. Renders as a non-blocking floating stack (work continues behind
/// it) listing every pending request with Approve / Edit / Deny. The requested
/// agent + permission preset are broken out explicitly — a `bypass` preset is
/// flagged, since that is exactly the escalation this gate exists to catch.
export function WorktreeGate() {
  const requests = useAtomValue(worktreeRequestsAtom);
  const workspaces = useAtomValue(workspacesAtom);
  const store = useStore();

  const nameOf = useMemo(() => {
    const sessions = new Map<string, string>();
    const wss = new Map<string, string>();
    for (const ws of workspaces) {
      wss.set(ws.id, ws.name);
      for (const s of ws.sessions) sessions.set(s.id, s.name);
    }
    return {
      session: (id: string) => sessions.get(id) ?? id.slice(0, 8),
      workspace: (id: string) => wss.get(id) ?? id.slice(0, 8),
    };
  }, [workspaces]);

  if (requests.length === 0) return null;

  return (
    <div className="pointer-events-none fixed bottom-3 right-3 z-[60] flex max-h-[80vh] w-[360px] flex-col gap-2 overflow-y-auto">
      <div className="pointer-events-auto flex items-center gap-2 rounded-md bg-card/95 px-2.5 py-1.5 text-[11px] font-medium text-foreground shadow-lg ring-1 ring-border/60 backdrop-blur">
        <GitBranch size={13} className="text-primary" />
        Worktree request{requests.length === 1 ? "" : "s"}
        <span className="ml-auto rounded bg-primary/15 px-1.5 text-[10px] text-primary">
          {requests.length}
        </span>
      </div>
      {requests.map((req) => (
        <RequestCard key={req.id} req={req} nameOf={nameOf} store={store} />
      ))}
    </div>
  );
}

function RequestCard({
  req,
  nameOf,
  store,
}: {
  req: WorktreeRequestView;
  nameOf: { session: (id: string) => string; workspace: (id: string) => string };
  store: ReturnType<typeof useStore>;
}) {
  const [editing, setEditing] = useState(false);
  const [prompt, setPrompt] = useState(req.prompt);
  const [branch, setBranch] = useState(req.branch_name ?? "");
  const [busy, setBusy] = useState(false);

  const bypass = req.permission_preset === "bypass";

  async function approve(useEdits: boolean) {
    setBusy(true);
    await approveWorktreeRequest(
      store,
      req,
      useEdits ? prompt : undefined,
      useEdits ? branch : undefined,
    );
    // The card unmounts when the queue re-fetch drops this request.
  }

  async function deny() {
    setBusy(true);
    await denyWorktreeRequest(store, req.id);
  }

  return (
    <div className="pointer-events-auto flex flex-col gap-1.5 rounded-lg bg-card p-2.5 text-[12px] text-foreground shadow-lg ring-1 ring-border/60">
      <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        <span className="font-medium text-foreground/90">{nameOf.session(req.requesting_session)}</span>
        <span>wants a worktree in</span>
        <span className="truncate font-medium text-foreground/90">{nameOf.workspace(req.workspace_id)}</span>
      </div>

      {editing ? (
        <textarea
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          rows={4}
          className="w-full resize-y rounded border border-border/50 bg-background px-2 py-1 text-[12px] leading-snug outline-none focus:border-primary/60"
          aria-label="Edit prompt"
        />
      ) : (
        <p className="max-h-24 overflow-y-auto whitespace-pre-wrap break-words rounded bg-secondary/20 px-2 py-1 text-[12px] leading-snug">
          {req.prompt}
        </p>
      )}

      <div className="flex items-center gap-1.5 text-[10px]">
        <GitBranch size={11} className="text-muted-foreground" />
        {editing ? (
          <input
            value={branch}
            onChange={(e) => setBranch(e.target.value)}
            placeholder="branch name"
            className="flex-1 rounded border border-border/50 bg-background px-1.5 py-0.5 text-[11px] outline-none focus:border-primary/60"
            aria-label="Edit branch name"
          />
        ) : (
          <span className="text-muted-foreground">{req.branch_name ?? "(auto)"}</span>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-1.5 text-[10px]">
        <span className="rounded bg-muted/50 px-1.5 py-0.5 text-muted-foreground">
          agent: {req.agent ?? "default"}
        </span>
        <span
          className={
            bypass
              ? "flex items-center gap-1 rounded bg-destructive/15 px-1.5 py-0.5 font-medium text-destructive"
              : "rounded bg-muted/50 px-1.5 py-0.5 text-muted-foreground"
          }
        >
          {bypass && <AlertTriangle size={10} />}
          preset: {req.permission_preset ?? "default"}
        </span>
        {req.allow_skip_in_cycle && (
          <span className="flex items-center gap-1 rounded bg-destructive/15 px-1.5 py-0.5 font-medium text-destructive">
            <AlertTriangle size={10} /> bypass-in-cycle
          </span>
        )}
      </div>

      {req.startup_command && (
        <div className="flex flex-col gap-0.5">
          <span className="flex items-center gap-1 text-[10px] font-medium text-destructive">
            <AlertTriangle size={10} /> runs this shell command before the agent starts:
          </span>
          <code className="max-h-16 overflow-y-auto whitespace-pre-wrap break-all rounded border border-destructive/40 bg-destructive/10 px-2 py-1 text-[11px] text-destructive">
            {req.startup_command}
          </code>
        </div>
      )}

      <div className="flex items-center gap-2 text-[10px] text-muted-foreground">
        <span>{req.resources.worktree_count} worktrees</span>
        <span>·</span>
        <span>{formatBytes(req.resources.free_disk_bytes)} free</span>
        {req.resources.over_soft_cap && (
          <span className="flex items-center gap-0.5 text-amber-500">
            <AlertTriangle size={10} /> over soft cap ({req.resources.soft_cap})
          </span>
        )}
      </div>

      <div className="mt-0.5 flex items-center gap-1.5">
        {editing ? (
          <>
            <button
              type="button"
              disabled={busy}
              onClick={() => approve(true)}
              className="flex items-center gap-1 rounded bg-primary px-2 py-1 text-[11px] font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              <Check size={12} /> Approve edited
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => setEditing(false)}
              className="rounded px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent disabled:opacity-50"
            >
              Cancel
            </button>
          </>
        ) : (
          <>
            <button
              type="button"
              disabled={busy}
              onClick={() => approve(false)}
              className="flex items-center gap-1 rounded bg-primary px-2 py-1 text-[11px] font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              <Check size={12} /> Approve
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => setEditing(true)}
              className="flex items-center gap-1 rounded px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent disabled:opacity-50"
            >
              <Pencil size={12} /> Edit
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={deny}
              className="ml-auto flex items-center gap-1 rounded px-2 py-1 text-[11px] text-destructive hover:bg-destructive/10 disabled:opacity-50"
            >
              <X size={12} /> Deny
            </button>
          </>
        )}
      </div>
    </div>
  );
}
