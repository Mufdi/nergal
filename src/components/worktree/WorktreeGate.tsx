import { useAtomValue, useStore } from "jotai";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, Check, GitBranch, Pencil, X } from "lucide-react";
import {
  worktreeRequestsAtom,
  approveWorktreeRequest,
  denyWorktreeRequest,
  type WorktreeRequestView,
} from "@/stores/worktreeGate";
import { workspacesAtom } from "@/stores/workspace";
import * as terminalService from "@/components/terminal/terminalService";

/// Permission presets the human can pick when editing (kebab-case matches the
/// Rust `PermissionPreset` serde). `bypass` is the escalation the gate guards.
const PRESETS = ["default", "plan", "accept-edits", "auto", "bypass"] as const;

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

interface EditDraft {
  id: string;
  prompt: string;
  branch: string;
  preset: string;
}

/// Native, structurally un-bypassable approval gate for agent-spawned worktree
/// sessions. A non-blocking floating stack (work continues behind it) that
/// captures keyboard focus when a request arrives so the human can decide
/// without the mouse: ↑/↓ select a card, A approve, E edit, D deny, Esc returns
/// focus to the terminal (patterns.md §5 + §8). Every agent-chosen escalation
/// input (agent, permission preset, startup_command, bypass-in-cycle) is shown
/// flagged — a `bypass` preset or a shell prelude is exactly what this catches.
export function WorktreeGate() {
  const requests = useAtomValue(worktreeRequestsAtom);
  const workspaces = useAtomValue(workspacesAtom);
  const store = useStore();
  const rootRef = useRef<HTMLDivElement>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<EditDraft | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const prevCount = useRef(0);

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

  // Keep the cursor valid as the queue changes; drop a stale edit draft.
  useEffect(() => {
    const ids = requests.map((r) => r.id);
    if (ids.length === 0) {
      if (selectedId !== null) setSelectedId(null);
    } else if (!selectedId || !ids.includes(selectedId)) {
      setSelectedId(ids[0]);
    }
    if (draft && !ids.includes(draft.id)) setDraft(null);
  }, [requests, selectedId, draft]);

  // Capture focus when a request first appears (the user asked the card to own
  // the keyboard so a decision can be made hands-on-keyboard).
  useEffect(() => {
    if (requests.length > 0 && prevCount.current === 0) {
      rootRef.current?.focus();
    }
    prevCount.current = requests.length;
  }, [requests.length]);

  const approve = useCallback(
    async (req: WorktreeRequestView, useDraft: boolean) => {
      setBusyId(req.id);
      const d = useDraft && draft?.id === req.id ? draft : null;
      await approveWorktreeRequest(store, req, d?.prompt, d?.branch, d?.preset);
      setDraft(null);
      setBusyId(null);
    },
    [store, draft],
  );

  const deny = useCallback(
    async (id: string) => {
      setBusyId(id);
      await denyWorktreeRequest(store, id);
      setBusyId(null);
    },
    [store],
  );

  const startEdit = useCallback((req: WorktreeRequestView) => {
    setDraft({
      id: req.id,
      prompt: req.prompt,
      branch: req.branch_name ?? "",
      preset: req.permission_preset ?? "default",
    });
  }, []);

  // Window-level keyboard nav scoped to the gate zone (patterns.md §8). Uses
  // `e.key` not `e.code` — native WebKitGTK keydown does not populate `code`
  // (matches the cross-session / ClickUp panels).
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const t = e.target as HTMLElement | null;
      // Esc works even from an edit field: cancel the edit, else release focus.
      if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.metaKey) {
        if (draft) {
          e.preventDefault();
          setDraft(null);
          rootRef.current?.focus();
          return;
        }
        if (t?.closest("[data-focus-zone='worktree-gate']")) {
          e.preventDefault();
          terminalService.focusActive();
          return;
        }
      }
      if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      const inField =
        t?.tagName === "INPUT" ||
        t?.tagName === "TEXTAREA" ||
        t?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (!t?.closest("[data-focus-zone='worktree-gate']")) return;
      const ids = requests.map((r) => r.id);
      if (ids.length === 0) return;
      const idx = Math.max(0, ids.indexOf(selectedId ?? ids[0]));
      const sel = requests.find((r) => r.id === selectedId);
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedId(ids[Math.min(idx + 1, ids.length - 1)]);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedId(ids[Math.max(idx - 1, 0)]);
      } else if (e.key === "a" && sel && busyId !== sel.id) {
        e.preventDefault();
        void approve(sel, false);
      } else if (e.key === "e" && sel) {
        e.preventDefault();
        startEdit(sel);
      } else if (e.key === "d" && sel && busyId !== sel.id) {
        e.preventDefault();
        void deny(sel.id);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [requests, selectedId, draft, busyId, approve, deny, startEdit]);

  if (requests.length === 0) return null;

  return (
    <div
      ref={rootRef}
      data-focus-zone="worktree-gate"
      tabIndex={0}
      className="pointer-events-auto fixed bottom-3 right-3 z-[60] flex max-h-[80vh] w-[360px] flex-col gap-2 overflow-y-auto outline-none"
    >
      <div className="flex items-center gap-2 rounded-md bg-card/95 px-2.5 py-1.5 text-[11px] font-medium text-foreground shadow-lg ring-1 ring-border/60 backdrop-blur">
        <GitBranch size={13} className="text-primary" />
        Worktree request{requests.length === 1 ? "" : "s"}
        <span className="ml-auto rounded bg-primary/15 px-1.5 text-[10px] text-primary">
          {requests.length}
        </span>
      </div>
      {requests.map((req) => (
        <RequestCard
          key={req.id}
          req={req}
          nameOf={nameOf}
          selected={req.id === selectedId}
          draft={draft?.id === req.id ? draft : null}
          busy={busyId === req.id}
          onSelect={() => setSelectedId(req.id)}
          onApprove={(useDraft) => approve(req, useDraft)}
          onDeny={() => deny(req.id)}
          onStartEdit={() => startEdit(req)}
          onCancelEdit={() => setDraft(null)}
          onDraftChange={(patch) => setDraft((d) => (d ? { ...d, ...patch } : d))}
        />
      ))}
    </div>
  );
}

function RequestCard({
  req,
  nameOf,
  selected,
  draft,
  busy,
  onSelect,
  onApprove,
  onDeny,
  onStartEdit,
  onCancelEdit,
  onDraftChange,
}: {
  req: WorktreeRequestView;
  nameOf: { session: (id: string) => string; workspace: (id: string) => string };
  selected: boolean;
  draft: EditDraft | null;
  busy: boolean;
  onSelect: () => void;
  onApprove: (useDraft: boolean) => void;
  onDeny: () => void;
  onStartEdit: () => void;
  onCancelEdit: () => void;
  onDraftChange: (patch: Partial<EditDraft>) => void;
}) {
  const editing = draft !== null;
  const bypass = (editing ? draft.preset : req.permission_preset) === "bypass";

  return (
    <div
      data-nav-item
      data-nav-selected={selected ? "true" : undefined}
      data-request-id={req.id}
      onMouseDown={onSelect}
      className={`flex flex-col gap-1.5 rounded-lg bg-card p-2.5 text-[12px] text-foreground shadow-lg ring-1 ${
        selected ? "ring-primary/70" : "ring-border/60"
      }`}
    >
      <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        <span className="font-medium text-foreground/90">{nameOf.session(req.requesting_session)}</span>
        <span>wants a worktree in</span>
        <span className="truncate font-medium text-foreground/90">{nameOf.workspace(req.workspace_id)}</span>
      </div>

      {editing ? (
        <textarea
          value={draft.prompt}
          onChange={(e) => onDraftChange({ prompt: e.target.value })}
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
            value={draft.branch}
            onChange={(e) => onDraftChange({ branch: e.target.value })}
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
        {editing ? (
          <select
            value={draft.preset}
            onChange={(e) => onDraftChange({ preset: e.target.value })}
            aria-label="Edit permission mode"
            className={`rounded border px-1 py-0.5 text-[10px] outline-none ${
              bypass
                ? "border-destructive/50 bg-destructive/10 font-medium text-destructive"
                : "border-border/50 bg-background text-muted-foreground"
            }`}
          >
            {PRESETS.map((p) => (
              <option key={p} value={p}>
                permissions: {p}
              </option>
            ))}
          </select>
        ) : (
          <span
            title="Permission mode the agent requested for the new session"
            className={
              bypass
                ? "flex items-center gap-1 rounded bg-destructive/15 px-1.5 py-0.5 font-medium text-destructive"
                : "rounded bg-muted/50 px-1.5 py-0.5 text-muted-foreground"
            }
          >
            {bypass && <AlertTriangle size={10} />}
            permissions: {req.permission_preset ?? "default"}
          </span>
        )}
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

      <div
        className="flex items-center gap-2 text-[10px] text-muted-foreground"
        title="Existing git worktrees in this repository, and free disk on its volume"
      >
        <span>{req.resources.worktree_count} worktrees in repo</span>
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
              onClick={() => onApprove(true)}
              className="flex items-center gap-1 rounded bg-primary px-2 py-1 text-[11px] font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              <Check size={12} /> Approve edited
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={onCancelEdit}
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
              onClick={() => onApprove(false)}
              title="Approve (A)"
              className="flex items-center gap-1 rounded bg-primary px-2 py-1 text-[11px] font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              <Check size={12} /> Approve
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={onStartEdit}
              title="Edit prompt, branch and permissions (E)"
              className="flex items-center gap-1 rounded px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent disabled:opacity-50"
            >
              <Pencil size={12} /> Edit
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={onDeny}
              title="Deny (D)"
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
