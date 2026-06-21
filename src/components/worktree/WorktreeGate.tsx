import { useAtomValue, useStore } from "jotai";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, Check, GitBranch, Pencil, RefreshCw, X } from "lucide-react";
import {
  worktreeRequestsAtom,
  approveWorktreeRequest,
  denyWorktreeRequest,
  type WorktreeRequestView,
} from "@/stores/worktreeGate";
import { workspacesAtom } from "@/stores/workspace";
import * as terminalService from "@/components/terminal/terminalService";
import { invoke } from "@/lib/tauri";
import type { AvailableAgent } from "@/lib/types";
import { Kbd } from "@/components/ui/kbd";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";

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
  agent: string;
}

/// Keyboard-reachable dropdown — same model as the ClickUp / Linear status
/// pickers (patterns.md §11): a Tab-focusable trigger; while open a window-capture
/// listener owns ↑/↓/Enter/Esc (stopImmediatePropagation beats the gate's own nav
/// handler); click-outside closes. Reused for the permission preset and the agent.
interface PickerOption {
  value: string;
  label: string;
  danger?: boolean;
}

function GatePicker({
  value,
  options,
  prefix,
  ariaLabel,
  onChange,
}: {
  value: string;
  options: PickerOption[];
  prefix: string;
  ariaLabel: string;
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [activeIdx, setActiveIdx] = useState(0);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);

  // Commit a choice and return focus to the trigger so Tab keeps moving
  // through the edit fields (no focus trap on the just-clicked option).
  const choose = useCallback(
    (v: string) => {
      onChange(v);
      setOpen(false);
      triggerRef.current?.focus();
    },
    [onChange],
  );

  useEffect(() => {
    if (!open) return;
    const idx = options.findIndex((o) => o.value === value);
    setActiveIdx(idx >= 0 ? idx : 0);
  }, [open, value, options]);

  useEffect(() => {
    if (!open) return;
    wrapRef.current
      ?.querySelector<HTMLElement>(`[data-opt-idx="${activeIdx}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [open, activeIdx]);

  useEffect(() => {
    if (!open) return;
    function onOutside(e: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (options.length === 0) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p + 1) % options.length);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p - 1 + options.length) % options.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        e.stopImmediatePropagation();
        choose(options[activeIdx].value);
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setOpen(false);
        triggerRef.current?.focus();
      }
    }
    document.addEventListener("mousedown", onOutside);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onOutside);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [open, activeIdx, choose, options]);

  const current = options.find((o) => o.value === value);
  const danger = current?.danger ?? false;
  return (
    <div ref={wrapRef} className="relative">
      <button
        ref={triggerRef}
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-label={ariaLabel}
        className={`flex items-center gap-1 rounded border px-1.5 py-0.5 text-[10px] outline-none focus:ring-1 focus:ring-primary ${
          danger
            ? "border-destructive/50 bg-destructive/10 font-medium text-destructive"
            : "border-border/60 bg-background text-muted-foreground"
        }`}
      >
        {danger && <AlertTriangle size={10} />}
        {prefix} {current?.label ?? value}
      </button>
      {open && (
        <div
          data-floating-popup
          className="absolute bottom-full left-0 z-50 mb-1 min-w-36 rounded-md border border-border bg-card shadow-md"
        >
          <div className="max-h-44 overflow-y-auto py-0.5">
            {options.map((o, i) => (
              <button
                key={o.value}
                type="button"
                data-opt-idx={i}
                data-nav-selected={i === activeIdx || undefined}
                onMouseEnter={() => setActiveIdx(i)}
                onClick={() => choose(o.value)}
                className={`flex w-full items-center gap-1.5 px-2 py-1 text-left text-[11px] transition-colors data-[nav-selected=true]:bg-accent data-[nav-selected=true]:text-accent-foreground ${
                  o.danger ? "text-destructive" : "text-muted-foreground"
                }`}
              >
                {o.danger && <AlertTriangle size={10} />}
                {o.label}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

/// Native, structurally un-bypassable approval gate for agent-spawned worktree
/// sessions (create) and reviving inactive ones (resume). One solid floating
/// panel (work continues behind it) that captures keyboard focus when a request
/// arrives — its border turns accent while focused. ↑/↓ select, A approve,
/// E edit, D deny, Esc returns focus to the terminal (patterns.md §5/§8). Every
/// agent-chosen escalation input (agent, permission preset, startup_command,
/// bypass-in-cycle) is shown flagged.
export function WorktreeGate() {
  const requests = useAtomValue(worktreeRequestsAtom);
  const workspaces = useAtomValue(workspacesAtom);
  const store = useStore();
  const rootRef = useRef<HTMLDivElement>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<EditDraft | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [agents, setAgents] = useState<AvailableAgent[]>([]);
  const prevCount = useRef(0);

  // The installed agents the human can pick at the gate, plus a "default"
  // (keep the requested/project default). Fetched lazily on first request.
  // Pi is excluded: it has no MCP-server mechanism, so a Pi worktree session
  // could not participate in the cross-session/MCP coordination this gate exists
  // for (it can't call the cluihud tools or be revived via them).
  const agentOptions = useMemo<PickerOption[]>(
    () => [
      { value: "default", label: "default" },
      ...agents
        .filter((a) => a.installed && a.id !== "pi")
        .map((a) => ({ value: a.id, label: a.display_name })),
    ],
    [agents],
  );

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

  const focusGate = useCallback(() => {
    let tries = 0;
    const attempt = () => {
      const el = rootRef.current;
      if (!el) return;
      el.focus();
      // WebKitGTK occasionally drops the first programmatic focus; retry across
      // frames until the active element is actually inside the gate.
      if (!el.contains(document.activeElement) && tries++ < 6) {
        requestAnimationFrame(attempt);
      }
    };
    requestAnimationFrame(() => requestAnimationFrame(attempt));
  }, []);

  // Load the installed agents once there's something to approve (so the agent
  // picker is ready when the human opens Edit).
  useEffect(() => {
    if (requests.length === 0 || agents.length > 0) return;
    invoke<AvailableAgent[]>("list_available_agents")
      .then(setAgents)
      .catch(() => {});
  }, [requests.length, agents.length]);

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

  // Capture focus when a new request arrives so the panel owns the keyboard;
  // hand focus back to the terminal once the queue empties (gate closes).
  useEffect(() => {
    const prev = prevCount.current;
    prevCount.current = requests.length;
    if (requests.length > prev) focusGate();
    else if (requests.length === 0 && prev > 0) terminalService.focusActive();
  }, [requests.length, focusGate]);

  const approve = useCallback(
    async (req: WorktreeRequestView, useDraft: boolean) => {
      setBusyId(req.id);
      const d = useDraft && draft?.id === req.id ? draft : null;
      await approveWorktreeRequest(store, req, d?.prompt, d?.branch, d?.preset, d?.agent);
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
      agent: req.agent ?? "default",
    });
  }, []);

  // Window-level keyboard nav scoped to the gate zone (patterns.md §8), capture
  // phase so it claims its keys before the terminal textarea (which stops
  // propagation). Uses `e.key` — native WebKitGTK keydown does not populate
  // `code` (matches the cross-session / ClickUp panels).
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      // An open floating popup (e.g. the preset dropdown) owns ALL keys — defer.
      if (document.querySelector("[data-floating-popup]")) return;
      const t = e.target as HTMLElement | null;
      const inGate = !!t?.closest("[data-focus-zone='worktree-gate']");
      // Esc: cancel an edit, else release focus to the terminal.
      if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.metaKey && inGate) {
        e.preventDefault();
        e.stopPropagation();
        if (draft) {
          setDraft(null);
          focusGate();
        } else {
          terminalService.focusActive();
        }
        return;
      }
      // Ctrl+Enter commits an edit (works from inside the textarea/inputs).
      if (e.key === "Enter" && e.ctrlKey && draft && inGate) {
        e.preventDefault();
        e.stopPropagation();
        const r = requests.find((x) => x.id === draft.id);
        if (r && busyId !== r.id) void approve(r, true);
        return;
      }
      // While editing, let the fields + the preset picker own all other keys
      // (Tab moves prompt → branch → preset → buttons natively).
      if (draft) return;
      if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      const inField =
        t?.tagName === "INPUT" ||
        t?.tagName === "TEXTAREA" ||
        t?.tagName === "SELECT" ||
        t?.getAttribute("contenteditable") === "true";
      if (inField || !inGate) return;
      const ids = requests.map((r) => r.id);
      if (ids.length === 0) return;
      const idx = Math.max(0, ids.indexOf(selectedId ?? ids[0]));
      const sel = requests.find((r) => r.id === selectedId);
      if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopPropagation();
        setSelectedId(ids[Math.min(idx + 1, ids.length - 1)]);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        setSelectedId(ids[Math.max(idx - 1, 0)]);
      } else if (e.key === "a" && sel && busyId !== sel.id) {
        e.preventDefault();
        e.stopPropagation();
        void approve(sel, false);
      } else if (e.key === "e" && sel) {
        e.preventDefault();
        e.stopPropagation();
        startEdit(sel);
      } else if (e.key === "d" && sel && busyId !== sel.id) {
        e.preventDefault();
        e.stopPropagation();
        void deny(sel.id);
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [requests, selectedId, draft, busyId, approve, deny, startEdit, focusGate]);

  if (requests.length === 0) return null;

  return (
    <TooltipProvider delay={0}>
      <div
        ref={rootRef}
        data-focus-zone="worktree-gate"
        tabIndex={0}
        onMouseDown={focusGate}
        className="pointer-events-auto fixed bottom-3 right-3 z-[60] flex max-h-[80vh] w-[368px] flex-col overflow-hidden rounded-lg border-2 border-border bg-card text-card-foreground shadow-2xl outline-none focus-within:border-primary"
      >
        <div className="flex items-center gap-2 border-b border-border/60 bg-card px-3 py-2 text-[11px] font-medium">
          <GitBranch size={13} className="text-primary" />
          Worktree request{requests.length === 1 ? "" : "s"}
          <span className="ml-auto rounded bg-primary/15 px-1.5 text-[10px] text-primary">
            {requests.length}
          </span>
        </div>
        <div className="flex min-h-0 flex-col divide-y divide-border/50 overflow-y-auto">
          {requests.map((req) => (
            <RequestRow
              key={req.id}
              req={req}
              nameOf={nameOf}
              agents={agents}
              agentOptions={agentOptions}
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
      </div>
    </TooltipProvider>
  );
}

function RequestRow({
  req,
  nameOf,
  agents,
  agentOptions,
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
  agents: AvailableAgent[];
  agentOptions: PickerOption[];
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
  const isResume = req.kind.type === "resume";
  const bypass = (editing ? draft.preset : req.permission_preset) === "bypass";
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Entering edit mode moves focus into the prompt so the user can type at once
  // (and Tab from there reaches branch → preset → buttons).
  useEffect(() => {
    if (editing) requestAnimationFrame(() => textareaRef.current?.focus());
  }, [editing]);

  // The permission presets a given agent actually supports (each adapter
  // declares its own); "default"/unknown falls back to the full set. Mirrors the
  // session-creation modal: the preset dropdown follows the chosen agent.
  const presetsForAgent = useCallback(
    (agentId: string): string[] => {
      if (agentId === "default") return [...PRESETS];
      return agents.find((a) => a.id === agentId)?.permission_presets ?? [...PRESETS];
    },
    [agents],
  );
  const presetOptions = useMemo<PickerOption[]>(
    () =>
      (editing ? presetsForAgent(draft.agent) : [...PRESETS]).map((p) => ({
        value: p,
        label: p,
        danger: p === "bypass",
      })),
    [editing, draft, presetsForAgent],
  );

  return (
    <div
      data-nav-item
      data-nav-selected={selected ? "true" : undefined}
      data-request-id={req.id}
      onMouseDown={onSelect}
      className={`flex flex-col gap-1.5 px-3 py-2.5 text-[12px] ${
        selected ? "bg-secondary/40" : "bg-card"
      }`}
    >
      <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        {isResume ? <RefreshCw size={11} className="text-primary" /> : null}
        <span className="font-medium text-foreground/90">{nameOf.session(req.requesting_session)}</span>
        <span>{isResume ? "wants to revive" : "wants a worktree in"}</span>
        <span className="truncate font-medium text-foreground/90">
          {req.kind.type === "resume"
            ? nameOf.session(req.kind.target_session_id)
            : nameOf.workspace(req.workspace_id)}
        </span>
      </div>

      {/* prompt / relayed message */}
      {editing ? (
        <textarea
          ref={textareaRef}
          value={draft.prompt}
          onChange={(e) => onDraftChange({ prompt: e.target.value })}
          rows={4}
          className="w-full resize-y rounded border border-border/50 bg-background px-2 py-1 text-[12px] leading-snug outline-none focus:border-primary/60"
          aria-label="Edit prompt"
        />
      ) : req.prompt ? (
        <p className="max-h-24 overflow-y-auto whitespace-pre-wrap break-words rounded border border-border/50 bg-muted/40 px-2 py-1 text-[12px] leading-snug">
          {req.prompt}
        </p>
      ) : (
        <p className="text-[11px] italic text-muted-foreground">(no message — just revive it)</p>
      )}

      {/* branch (create) / escalation surface (create only) */}
      {!isResume && (
        <>
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
            {editing ? (
              <GatePicker
                value={draft.agent}
                options={agentOptions}
                prefix="agent:"
                ariaLabel="Edit agent"
                onChange={(a) => {
                  // Keep the preset valid for the newly-chosen agent.
                  const presets = presetsForAgent(a);
                  const preset = presets.includes(draft.preset)
                    ? draft.preset
                    : (presets[0] ?? "default");
                  onDraftChange({ agent: a, preset });
                }}
              />
            ) : (
              <span className="rounded bg-muted/50 px-1.5 py-0.5 text-muted-foreground">
                agent: {req.agent ?? "default"}
              </span>
            )}
            {editing ? (
              <GatePicker
                value={draft.preset}
                options={presetOptions}
                prefix="permissions:"
                ariaLabel="Edit permission mode"
                onChange={(p) => onDraftChange({ preset: p })}
              />
            ) : (
              <Tooltip>
                <TooltipTrigger
                  render={
                    <span
                      className={
                        bypass
                          ? "flex items-center gap-1 rounded bg-destructive/15 px-1.5 py-0.5 font-medium text-destructive"
                          : "rounded bg-muted/50 px-1.5 py-0.5 text-muted-foreground"
                      }
                    />
                  }
                >
                  {bypass && <AlertTriangle size={10} />}
                  permissions: {req.permission_preset ?? "default"}
                </TooltipTrigger>
                <TooltipContent side="top">
                  Permission mode the agent requested for the new session
                </TooltipContent>
              </Tooltip>
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

          <Tooltip>
            <TooltipTrigger
              render={<div className="flex items-center gap-2 text-[10px] text-muted-foreground" />}
            >
              <span>{req.resources.worktree_count} worktrees in repo</span>
              <span>·</span>
              <span>{formatBytes(req.resources.free_disk_bytes)} free</span>
              {req.resources.over_soft_cap && (
                <span className="flex items-center gap-0.5 text-amber-500">
                  <AlertTriangle size={10} /> over soft cap ({req.resources.soft_cap})
                </span>
              )}
            </TooltipTrigger>
            <TooltipContent side="top">
              Existing git worktrees in this repository, and free disk on its volume
            </TooltipContent>
          </Tooltip>
        </>
      )}

      {/* actions */}
      <div className="mt-0.5 flex items-center gap-1.5">
        {editing ? (
          <>
            <button
              type="button"
              disabled={busy}
              onClick={() => onApprove(true)}
              className="flex items-center gap-1 rounded bg-primary px-2 py-1 text-[11px] font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              <Check size={12} /> {isResume ? "Resume edited" : "Approve edited"}
              <Kbd keys="ctrl+enter" tone="onPrimary" className="ml-0.5" />
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={onCancelEdit}
              className="flex items-center gap-1 rounded px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent disabled:opacity-50"
            >
              Cancel <Kbd keys="esc" className="ml-0.5" />
            </button>
          </>
        ) : (
          <>
            <button
              type="button"
              disabled={busy}
              onClick={() => onApprove(false)}
              className="flex items-center gap-1 rounded bg-primary px-2 py-1 text-[11px] font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              {isResume ? <RefreshCw size={12} /> : <Check size={12} />}
              {isResume ? "Revive" : "Approve"} <Kbd keys="a" tone="onPrimary" className="ml-0.5" />
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={onStartEdit}
              className="flex items-center gap-1 rounded px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent disabled:opacity-50"
            >
              <Pencil size={12} /> Edit <Kbd keys="e" className="ml-0.5" />
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={onDeny}
              className="ml-auto flex items-center gap-1 rounded px-2 py-1 text-[11px] text-destructive hover:bg-destructive/10 disabled:opacity-50"
            >
              <X size={12} /> Deny <Kbd keys="d" className="ml-0.5" />
            </button>
          </>
        )}
      </div>
    </div>
  );
}
