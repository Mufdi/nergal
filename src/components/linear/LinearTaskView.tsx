import React, { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent as ReactMouseEvent } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { open as openShell } from "@tauri-apps/plugin-shell";
import {
  AlignLeft,
  Calendar,
  CheckSquare,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Circle,
  CircleDot,
  ExternalLink,
  Pencil,
  GitBranchPlus,
  IterationCw,
  Link2,
  Paperclip,
  Pin,
  PinOff,
  RefreshCw,
  Send,
  SignalHigh,
  Tag,
  Unlink,
  UserCheck,
  UserMinus,
  UserPlus,
} from "lucide-react";
import { invoke } from "@/lib/tauri";
import { MarkdownView } from "@/components/plan/MarkdownView";
import { LinearStatusIcon } from "@/components/linear/LinearStatusIcon";
import { LinearEstimateIcon } from "@/components/linear/LinearEstimateIcon";
import { PriorityIcon } from "@/components/clickup/PriorityIcon";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  activeSessionLinearIssueAtom,
  activeSessionLinearPinsAtom,
  clearLinearOverlayEntry,
  copyLinearIssueAction,
  LINEAR_ACTION_LABELS,
  linearClosedOutAtom,
  linearClosureOfferAtom,
  linearIssuesAtom,
  linearOverlayAtom,
  linearSyncStatusAtom,
  reinjectIssueAction,
  requestBindIssueAction,
  requestSendIssueAction,
  setLinearOverlayEntry,
  spawnWorktreeWithIssueAction,
  togglePinIssueAction,
  type IssueView,
  type LinearAttachment,
  type LinearActivityEntry,
  type LinearComment,
  type LinearIssueDetail,
  type LinearRelation,
  type WorkflowStateView,
  type CycleView,
} from "@/stores/linear";
import { activeSessionIdAtom } from "@/stores/workspace";
import { toastsAtom } from "@/stores/toast";

// ── Shared helpers ──

export function linearStateToIconType(stateType: string | undefined): string | null {
  switch (stateType) {
    case "triage":
    case "backlog":
    case "unstarted":
      return "open";
    case "started":
      return "custom";
    case "completed":
      return "done";
    case "cancelled":
    case "canceled":
      return "closed";
    default:
      return null;
  }
}

export function linearPriorityStr(p: number): string | null {
  switch (p) {
    case 1: return "urgent";
    case 2: return "high";
    case 3: return "normal";
    case 4: return "low";
    default: return null;
  }
}

// Backend timestamps are epoch SECONDS; multiply by 1000 for JS Date/Date.now().
function formatDate(secs: number): string {
  return new Date(secs * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

/// Compact relative time ("3mo ago", "2d ago") for the activity feed.
function formatRelative(secs: number): string {
  const diff = Date.now() - secs * 1000;
  const s = Math.max(0, Math.floor(diff / 1000));
  if (s < 60) return "just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 30) return `${d}d ago`;
  const mo = Math.floor(d / 30);
  if (mo < 12) return `${mo}mo ago`;
  return `${Math.floor(mo / 12)}y ago`;
}

/// t-shirt estimate scale (matches the rail's TSHIRT_LABELS); maps the raw
/// numeric estimate carried by an "estimate" activity to its size letter.
const TSHIRT_ESTIMATE: Record<string, string> = { "1": "XS", "2": "S", "3": "M", "4": "L", "5": "XL", "6": "XXL" };

/// Format a Linear `TimelessDate` (YYYY-MM-DD) as "Nov 18, 2025".
function formatDateStr(s: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(s);
  if (!m) return s;
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

/// The human verb for an activity entry, derived from its kind + from/to.
/// `tshirt` maps estimate numbers to size letters when the team uses t-shirt sizing.
function activityVerb(e: LinearActivityEntry, tshirt = false): string {
  switch (e.kind) {
    case "created":
      return "created the issue";
    case "state":
      return e.from && e.to ? `changed status ${e.from} → ${e.to}` : e.to ? `set status to ${e.to}` : "changed status";
    case "assignee":
      if (e.to && e.from) return `reassigned ${e.from} → ${e.to}`;
      if (e.to) return `assigned to ${e.to}`;
      return "unassigned the issue";
    case "label": {
      const parts: string[] = [];
      if (e.added.length) parts.push(`added label ${e.added.join(", ")}`);
      if (e.removed.length) parts.push(`removed label ${e.removed.join(", ")}`);
      return parts.join("; ") || "changed labels";
    }
    case "cycle":
      if (e.to && e.from) return `moved ${e.from} → ${e.to}`;
      if (e.to) return `added to ${e.to}`;
      if (e.from) return `removed from ${e.from}`;
      return "changed cycle";
    case "priority":
      return e.from && e.to ? `changed priority ${e.from} → ${e.to}` : e.to ? `set priority to ${e.to}` : "changed priority";
    case "estimate": {
      const lbl = (v?: string) => (v == null ? null : tshirt ? TSHIRT_ESTIMATE[v] ?? v : v);
      const to = lbl(e.to);
      const from = lbl(e.from);
      if (to && from) return `changed estimate ${from} → ${to}`;
      if (to) return `estimated complexity as ${to}`;
      return "removed the estimate";
    }
    case "dueDate":
      if (e.to) return `set the due date to ${formatDateStr(e.to)}`;
      return "removed the due date";
    case "title":
      return e.to ? `renamed to “${e.to}”` : "renamed the issue";
    case "description":
      return "updated the description";
    default:
      return "updated the issue";
  }
}

/// The timeline node icon for an activity entry, keyed by its kind (and, for
/// assignee, the add/remove direction). Matches Linear's activity-feed glyphs.
function activityIcon(e: LinearActivityEntry, size = 11): React.ReactNode {
  switch (e.kind) {
    case "created":
      return <CircleDot size={size} />;
    case "assignee":
      if (e.to && e.from) return <UserCheck size={size} />;
      return e.to ? <UserPlus size={size} /> : <UserMinus size={size} />;
    case "label":
      return <Tag size={size} />;
    case "cycle":
      return <IterationCw size={size} />;
    case "priority":
      return <SignalHigh size={size} />;
    case "estimate":
      return <LinearEstimateIcon size={size} />;
    case "dueDate":
      return <Calendar size={size} />;
    case "title":
      return <Pencil size={size} />;
    case "description":
      return <AlignLeft size={size} />;
    case "state":
    default:
      return <Circle size={size} />;
  }
}

/// Validates a URL through the backend (browser_validate_url) before opening.
async function openValidatedUrl(url: string | undefined): Promise<void> {
  if (!url) return;
  try {
    const safe = await invoke<string>("browser_validate_url", { url });
    await openShell(safe);
  } catch {
    // URL rejected by backend — silently drop.
  }
}

// ── Controller ──

interface LinearDetailData {
  issue: IssueView | null;
  comments: LinearComment[];
  attachments: LinearAttachment[];
  relations: LinearRelation[];
  activity: LinearActivityEntry[];
}

/// SWR cache for fetched detail payloads. Module-level so it survives a
/// modal/tab unmount — revisiting an issue renders instantly from cache.
const detailCache = new Map<string, LinearDetailData>();

export type LinearIssueController = ReturnType<typeof useLinearIssueController>;

export function useLinearIssueController({
  issueId,
  setIssueId,
}: {
  issueId: string | null;
  setIssueId: (id: string | null) => void;
}) {
  const issues = useAtomValue(linearIssuesAtom);
  const copyIssue = useSetAtom(copyLinearIssueAction);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const addToast = useSetAtom(toastsAtom);
  const [closedOutSet, setClosedOut] = useAtom(linearClosedOutAtom);
  const [detail, setDetail] = useState<LinearDetailData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [commentDraft, setCommentDraft] = useState("");
  const [postingComment, setPostingComment] = useState(false);
  const [statusPickerOpen, setStatusPickerOpen] = useState(false);
  const [cyclePickerOpen, setCyclePickerOpen] = useState(false);
  const commentRef = useRef<HTMLTextAreaElement | null>(null);
  // State-driven index cursor over actionable elements (mirrors ClickUp's
  // detail nav: ↑/↓ move, Enter/Space activate, highlight via data-nav-selected).
  const [navKey, setNavKey] = useState<string | null>(null);
  // Keep setIssueId stable for the detail's relation click handler
  const setIssueIdRef = useRef(setIssueId);
  setIssueIdRef.current = setIssueId;
  const contentRef = useRef<HTMLDivElement | null>(null);
  const mainRef = useRef<HTMLDivElement | null>(null);

  // Drill-in history: opening a sub-issue pushes onto the stack; ←/→ navigate.
  // Mirrors useClickUpTaskController's histNav pattern exactly.
  const detailHistory = useRef<{ stack: string[]; index: number }>({ stack: [], index: -1 });
  const navInternal = useRef(false);
  const [histNav, setHistNav] = useState({ canBack: false, canFwd: false });

  // Reset + grab focus when issue opens (clear the index cursor on drill-in)
  useEffect(() => {
    setNavKey(null);
    if (issueId === null) return;
    const t = setTimeout(() => contentRef.current?.focus({ preventScroll: true }), 60);
    return () => clearTimeout(t);
  }, [issueId]);

  // When a rail dropdown (state/cycle) closes, return focus to the nav container
  // so arrow nav resumes. Mirrors ClickUp.
  useEffect(() => {
    if (issueId === null || statusPickerOpen || cyclePickerOpen) return;
    contentRef.current?.focus({ preventScroll: true });
  }, [statusPickerOpen, cyclePickerOpen, issueId]);

  // Status and due are in the properties rail (always at top) — scroll the main
  // content area to the top instead of scrollIntoView (which would scroll the
  // rail's aside, not the main pane). Everything else gets smooth scrollIntoView.
  useEffect(() => {
    if (!navKey) return;
    if (navKey === "status" || navKey === "due" || navKey === "cycle") {
      mainRef.current?.scrollTo({ top: 0, behavior: "smooth" });
      return;
    }
    contentRef.current
      ?.querySelector<HTMLElement>(`[data-nav-key="${CSS.escape(navKey)}"]`)
      ?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [navKey]);

  // Maintain drill-in history stack
  useEffect(() => {
    const h = detailHistory.current;
    if (issueId === null) {
      detailHistory.current = { stack: [], index: -1 };
      setHistNav({ canBack: false, canFwd: false });
      return;
    }
    if (navInternal.current) {
      navInternal.current = false;
    } else if (h.index >= 0 && h.stack[h.index] === issueId) {
      // already current — no-op
    } else if (h.index === -1) {
      h.stack = [issueId];
      h.index = 0;
    } else {
      h.stack = [...h.stack.slice(0, h.index + 1), issueId];
      h.index = h.stack.length - 1;
    }
    setHistNav({ canBack: h.index > 0, canFwd: h.index < h.stack.length - 1 });
  }, [issueId]);

  function stepHistory(delta: number) {
    const h = detailHistory.current;
    const next = h.index + delta;
    if (next < 0 || next >= h.stack.length) return;
    h.index = next;
    navInternal.current = true;
    setIssueId(h.stack[next]);
    setHistNav({ canBack: next > 0, canFwd: next < h.stack.length - 1 });
  }

  useEffect(() => {
    if (!issueId) {
      setDetail(null);
      setError(null);
      return;
    }
    let cancelled = false;
    const cached = detailCache.get(issueId);
    if (cached) {
      setDetail(cached);
      setLoading(false);
    } else {
      setDetail(null);
      setLoading(true);
    }
    setError(null);

    Promise.all([
      invoke<IssueView[]>("linear_read_issues", { includeStale: true }),
      invoke<LinearIssueDetail>("linear_issue_detail", { issueId }),
    ])
      .then(([issueList, detailPayload]) => {
        if (cancelled) return;
        const issue = issueList.find((i) => i.id === issueId) ?? null;
        const data: LinearDetailData = {
          issue,
          comments: detailPayload.comments,
          attachments: detailPayload.attachments,
          relations: detailPayload.relations,
          activity: detailPayload.activity ?? [],
        };
        setDetail(data);
        detailCache.set(issueId, data);
      })
      .catch((err) => {
        if (!cancelled && !detailCache.has(issueId)) setError(String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => { cancelled = true; };
  }, [issueId]);

  const mirrorIssue = issueId ? issues.find((i) => i.id === issueId) ?? null : null;
  // Prefer the live mirror (refreshed by the poll's `linear:changed`) over the
  // detail's open-time snapshot so write-backs (state/assignee/cycle) reflect in
  // the modal once the next reconcile lands, instead of staying stale until reopen.
  const issue = mirrorIssue ?? detail?.issue;
  const comments = detail?.comments ?? [];
  const attachments = detail?.attachments ?? [];
  const relations = detail?.relations ?? [];
  const activity = detail?.activity ?? [];

  // Sub-issues from the mirror (parentId === issueId)
  const subIssues = issueId ? issues.filter((i) => i.parentId === issueId) : [];

  // Build a full parent→children map for recursive sub-issue tree (§9 canon).
  const childMap = new Map<string, IssueView[]>();
  for (const i of issues) {
    if (i.parentId) {
      const arr = childMap.get(i.parentId);
      if (arr) arr.push(i);
      else childMap.set(i.parentId, [i]);
    }
  }

  const closedOut = issue ? closedOutSet.has(issue.id) : false;

  async function handleUncloseOut(targetId: string) {
    try {
      await invoke("linear_unmark_closed_out", { issueId: targetId });
      setClosedOut((prev) => {
        const next = new Set(prev);
        next.delete(targetId);
        return next;
      });
    } catch {
      // Badge stays — silently ignore.
    }
  }

  // Visible actionable elements in DOM order (the nav-cursor source of truth).
  function navKeysInOrder(): string[] {
    const content = contentRef.current;
    if (!content) return [];
    return Array.from(content.querySelectorAll<HTMLElement>("[data-nav-key]"))
      .filter((el) => el.offsetParent !== null)
      .map((el) => el.dataset.navKey ?? "")
      .filter(Boolean);
  }

  function activateNav(key: string) {
    if (key === "copyid") {
      if (issue?.identifier) copyIssue(issue.identifier);
    } else if (key === "open") {
      void openValidatedUrl(issue?.url);
    } else if (key === "status") {
      setStatusPickerOpen(true);
    } else if (key === "cycle") {
      setCyclePickerOpen(true);
    } else if (key === "unclose") {
      if (issue) void handleUncloseOut(issue.id);
    } else if (key === "comment") {
      commentRef.current?.focus();
    } else if (key.startsWith("sub:")) {
      setIssueId(key.slice(4));
    } else if (key.startsWith("rel:")) {
      setIssueId(key.slice(4));
    } else if (key.startsWith("att:")) {
      const att = attachments.find((a) => a.id === key.slice(4));
      if (att) void openValidatedUrl(att.url);
    }
  }

  function handleNavKeyDown(e: ReactKeyboardEvent) {
    // While a rail dropdown (state/cycle) is open it owns ↑/↓/Enter/Esc via its
    // own window-capture handler; don't let the cursor steal those keys.
    if (statusPickerOpen || cyclePickerOpen) return;
    const target = e.target as HTMLElement | null;
    if (target?.tagName === "TEXTAREA" || target?.tagName === "INPUT") return;
    if (e.code === "ArrowDown" || e.code === "ArrowUp") {
      const keys = navKeysInOrder();
      if (keys.length === 0) return;
      e.preventDefault();
      e.stopPropagation();
      const idx = navKey ? keys.indexOf(navKey) : -1;
      const next =
        e.code === "ArrowDown" ? Math.min(idx + 1, keys.length - 1) : Math.max(idx - 1, 0);
      setNavKey(keys[idx === -1 ? 0 : next] ?? keys[0]);
    } else if ((e.code === "Enter" || e.code === "Space") && navKey) {
      e.preventDefault();
      e.stopPropagation();
      activateNav(navKey);
    }
  }

  // Clicking an actionable element moves the cursor there; refocus the container
  // (WebKitGTK doesn't focus buttons on click) so arrow-nav resumes.
  function handleContainerClick(e: ReactMouseEvent) {
    const target = e.target as HTMLElement | null;
    const el = target?.closest<HTMLElement>("[data-nav-key]");
    if (el?.dataset.navKey) setNavKey(el.dataset.navKey);
    if (target && target.tagName !== "TEXTAREA" && target.tagName !== "INPUT") {
      contentRef.current?.focus({ preventScroll: true });
    }
  }

  async function refreshDetail(id: string) {
    try {
      const [issueList, detailPayload] = await Promise.all([
        invoke<IssueView[]>("linear_read_issues", { includeStale: true }),
        invoke<LinearIssueDetail>("linear_issue_detail", { issueId: id }),
      ]);
      const refreshedIssue = issueList.find((i) => i.id === id) ?? null;
      const data: LinearDetailData = {
        issue: refreshedIssue,
        comments: detailPayload.comments,
        attachments: detailPayload.attachments,
        relations: detailPayload.relations,
        activity: detailPayload.activity ?? [],
      };
      setDetail(data);
      detailCache.set(id, data);
    } catch {
      // Stale cache stays — comment was still posted.
    }
  }

  async function handlePostComment() {
    const trimmed = commentDraft.trim();
    if (!trimmed || postingComment || !activeSessionId || !issueId) return;
    setPostingComment(true);
    try {
      // Token-gated path: close_out=false — posting a comment must NOT close the issue.
      const token = await invoke<string>("linear_request_comment_token", {
        issueId,
        comment: trimmed,
      });
      await invoke("linear_execute_gated_write", { token, sessionId: activeSessionId });
      addToast({ message: "Comment posted", description: trimmed.slice(0, 60), type: "success" });
      setCommentDraft("");
      await refreshDetail(issueId);
    } catch (err) {
      addToast({ message: "Comment failed", description: String(err), type: "error" });
    } finally {
      setPostingComment(false);
    }
  }

  return {
    issueId,
    setIssueId,
    detail,
    loading,
    error,
    issue,
    comments,
    attachments,
    relations,
    activity,
    subIssues,
    childMap,
    closedOut,
    handleUncloseOut,
    contentRef,
    mainRef,
    histNav,
    stepHistory,
    navKey,
    setNavKey,
    navKeysInOrder,
    activateNav,
    handleNavKeyDown,
    handleContainerClick,
    commentDraft,
    setCommentDraft,
    postingComment,
    commentRef,
    handlePostComment,
    statusPickerOpen,
    setStatusPickerOpen,
    cyclePickerOpen,
    setCyclePickerOpen,
    refreshDetail,
  };
}

// ── Sub-issue tree (expandable, default expanded in modal) ──

function SubIssueTree({
  parentId,
  childMap,
  depth,
  seen,
  onOpen,
  navKey,
}: {
  parentId: string;
  childMap: ReadonlyMap<string, IssueView[]>;
  depth: number;
  seen: ReadonlySet<string>;
  onOpen: (id: string) => void;
  navKey: string | null;
}) {
  const children = seen.has(parentId) ? [] : (childMap.get(parentId) ?? []);
  if (children.length === 0) return null;

  return (
    <>
      {children.map((sub) => (
        <SubIssueNode
          key={sub.id}
          issue={sub}
          childMap={childMap}
          depth={depth}
          seen={new Set([...seen, parentId])}
          onOpen={onOpen}
          navKey={navKey}
        />
      ))}
    </>
  );
}

function SubIssueNode({
  issue,
  childMap,
  depth,
  seen,
  onOpen,
  navKey,
}: {
  issue: IssueView;
  childMap: ReadonlyMap<string, IssueView[]>;
  depth: number;
  seen: ReadonlySet<string>;
  onOpen: (id: string) => void;
  navKey: string | null;
}) {
  const hasChildren = !seen.has(issue.id) && (childMap.get(issue.id)?.length ?? 0) > 0;
  // Default expanded in the detail modal (contrast with the panel which defaults collapsed).
  const [expanded, setExpanded] = useState(true);

  return (
    <div>
      <button
        type="button"
        onClick={() => onOpen(issue.id)}
        data-nav-key={`sub:${issue.id}`}
        data-nav-selected={navKey === `sub:${issue.id}` || undefined}
        style={{ paddingLeft: 4 + depth * 16 }}
        className="flex w-full items-center gap-1.5 rounded py-0.5 pr-1 text-left text-[11px] text-foreground/80 outline-none transition-colors hover:bg-secondary/40"
      >
        {/* Chevron slot (§9) */}
        {hasChildren ? (
          <span
            role="button"
            tabIndex={-1}
            aria-label={expanded ? "Collapse" : "Expand"}
            onClick={(e) => {
              e.stopPropagation();
              setExpanded((p) => !p);
            }}
            className="flex size-3.5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
          >
            {expanded ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          </span>
        ) : (
          <span className="w-3.5 shrink-0" aria-hidden />
        )}
        <LinearStatusIcon
          stateType={issue.stateType}
          color={issue.stateColor ?? null}
          size={12}
          className="shrink-0"
        />
        {issue.identifier && (
          <span className="shrink-0 font-mono text-[10px] text-muted-foreground/60">{issue.identifier}</span>
        )}
        <span className="truncate">{issue.title}</span>
      </button>
      {hasChildren && expanded && (
        <SubIssueTree
          parentId={issue.id}
          childMap={childMap}
          depth={depth + 1}
          seen={new Set([...seen, issue.id])}
          onOpen={onOpen}
          navKey={navKey}
        />
      )}
    </div>
  );
}

// ── State picker in the properties rail (keyboard-reachable, data-nav-key="status") ──

/// Replaces the read-only state icon in the Properties rail with a keyboard-reachable
/// pill. Enter opens the dropdown (own ↑/↓/Enter/Esc capture). Writes via
/// linear_set_issue_state + the optimistic overlay (reversible, un-token-gated).
function StatePickerRail({
  issueId,
  teamId,
  currentStateId,
  currentStateType,
  currentStateColor,
  currentStateName,
  navKey,
  open,
  onOpenChange,
}: {
  issueId: string;
  teamId: string;
  currentStateId?: string;
  currentStateType?: string;
  currentStateColor?: string;
  currentStateName?: string;
  navKey: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [states, setStates] = useState<WorkflowStateView[]>([]);
  const setOverlay = useSetAtom(linearOverlayAtom);
  const addToast = useSetAtom(toastsAtom);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [activeIdx, setActiveIdx] = useState(0);

  useEffect(() => {
    if (!teamId) return;
    invoke<WorkflowStateView[]>("linear_read_team_states", { teamId })
      .then(setStates)
      .catch(() => {});
  }, [teamId]);

  // When dropdown opens, reset cursor to the current state.
  useEffect(() => {
    if (!open) return;
    const idx = states.findIndex((s) => s.id === currentStateId);
    setActiveIdx(idx >= 0 ? idx : 0);
  }, [open, currentStateId, states]);

  // Keep the keyboard-highlighted option scrolled into view in the dropdown.
  useEffect(() => {
    if (!open) return;
    wrapRef.current
      ?.querySelector<HTMLElement>(`[data-opt-idx="${activeIdx}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [open, activeIdx]);

  function choose(stateId: string) {
    onOpenChange(false);
    if (stateId === currentStateId) return;
    setLinearOverlayEntry(setOverlay, issueId, "state", stateId);
    invoke("linear_set_issue_state", { issueId, stateId }).catch((err) => {
      clearLinearOverlayEntry(setOverlay, issueId, "state");
      addToast({ message: "State change failed", description: String(err), type: "error" });
    });
  }

  // Exact copy of ClickUp's StatusPicker keyboard model: while open, a
  // window-capture listener owns ↑/↓/Enter/Esc; stopImmediatePropagation beats
  // the cursor's handleNavKeyDown and the FloatingPanel Esc-to-close.
  useEffect(() => {
    if (!open) return;
    function onOutside(e: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) onOpenChange(false);
    }
    function onKey(e: KeyboardEvent) {
      if (states.length === 0) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p + 1) % states.length);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p - 1 + states.length) % states.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        e.stopImmediatePropagation();
        const s = states[activeIdx];
        if (s) choose(s.id);
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        onOpenChange(false);
      }
    }
    document.addEventListener("mousedown", onOutside);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onOutside);
      window.removeEventListener("keydown", onKey, true);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, states, activeIdx]);

  return (
    <div ref={wrapRef} className="relative">
      <button
        type="button"
        data-nav-key="status"
        data-nav-selected={navKey === "status" || undefined}
        onClick={() => onOpenChange(!open)}
        className={`flex items-center gap-1.5 rounded px-1 outline-none transition-colors hover:bg-secondary/40 ${
          navKey === "status" ? "ring-1 ring-foreground/50" : ""
        }`}
      >
        <LinearStatusIcon stateType={currentStateType} color={currentStateColor ?? null} size={13} className="shrink-0" />
        <span className="text-foreground/80">{currentStateName ?? "No state"}</span>
      </button>
      {open && states.length > 0 && (
        <div
          data-floating-popup
          className="absolute left-0 top-full z-50 mt-1 min-w-40 rounded-md border border-border bg-card shadow-md"
        >
          <div className="max-h-48 overflow-y-auto py-0.5">
            {states.map((s, i) => (
              <button
                key={s.id}
                type="button"
                data-opt-idx={i}
                data-nav-selected={i === activeIdx || undefined}
                onMouseEnter={() => setActiveIdx(i)}
                onClick={() => void choose(s.id)}
                className={`flex w-full items-center gap-2 px-2 py-1 text-left text-[11px] transition-colors data-[nav-selected=true]:bg-accent data-[nav-selected=true]:text-accent-foreground ${
                  s.id === currentStateId ? "text-foreground" : "text-muted-foreground"
                }`}
              >
                <LinearStatusIcon stateType={s.type} color={s.color ?? null} size={12} className="shrink-0" />
                {s.name}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ── Cycle picker in the properties rail (keyboard-reachable, data-nav-key="cycle") ──

/// "Add to cycle" / change-cycle control. Lists the team's cycles + a "No cycle"
/// option. Writes via linear_set_issue_cycle + the optimistic overlay. Keyboard
/// model is identical to StatePickerRail (window-capture while open).
function CyclePicker({
  issueId,
  teamId,
  currentCycleId,
  currentCycleName,
  navKey,
  open,
  onOpenChange,
}: {
  issueId: string;
  teamId: string;
  currentCycleId?: string;
  currentCycleName?: string;
  navKey: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [cycles, setCycles] = useState<CycleView[]>([]);
  const setOverlay = useSetAtom(linearOverlayAtom);
  const addToast = useSetAtom(toastsAtom);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [activeIdx, setActiveIdx] = useState(0);
  // Optimistic label shown until the mirror prop reflects the chosen cycle (the
  // global overlay isn't applied to the displayed issue — Decision parity gap).
  const [pending, setPending] = useState<{ target: string | null; label: string } | null>(null);

  useEffect(() => {
    if (!teamId) return;
    invoke<CycleView[]>("linear_read_team_cycles", { teamId })
      .then(setCycles)
      .catch(() => {});
  }, [teamId]);

  // "No cycle" (id "") leads so the issue can be removed from its cycle.
  const options = useMemo(
    () => [
      { id: "", label: "No cycle" },
      ...cycles.map((c) => ({
        id: c.id,
        label: c.name && c.name.length > 0 ? c.name : c.number != null ? `Cycle ${c.number}` : "Cycle",
      })),
    ],
    [cycles],
  );

  useEffect(() => {
    if (!open) return;
    const idx = options.findIndex((o) => o.id === (currentCycleId ?? ""));
    setActiveIdx(idx >= 0 ? idx : 0);
  }, [open, currentCycleId, options]);

  useEffect(() => {
    if (!open) return;
    wrapRef.current
      ?.querySelector<HTMLElement>(`[data-opt-idx="${activeIdx}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [open, activeIdx]);

  // Clear the optimistic label once the mirror catches up to the chosen cycle.
  useEffect(() => {
    if (pending && (currentCycleId ?? null) === pending.target) setPending(null);
  }, [currentCycleId, pending]);

  function choose(cycleId: string) {
    onOpenChange(false);
    const target = cycleId === "" ? null : cycleId;
    // Compare against the optimistic target when one is pending, so removing a
    // just-added cycle isn't swallowed as a no-op before the mirror catches up.
    const effectiveCurrent = pending ? pending.target : currentCycleId ?? null;
    if (effectiveCurrent === target) return;
    const label = options.find((o) => o.id === cycleId)?.label ?? "";
    setPending({ target, label });
    setLinearOverlayEntry(setOverlay, issueId, "cycle", target);
    invoke("linear_set_issue_cycle", { issueId, cycleId: target }).catch((err) => {
      setPending(null);
      clearLinearOverlayEntry(setOverlay, issueId, "cycle");
      addToast({ message: "Cycle change failed", description: String(err), type: "error" });
    });
  }

  // What the trigger shows: optimistic choice (cycle name, or none → "Add to
  // cycle") while pending, otherwise the mirror's current cycle name.
  const displayName = pending ? (pending.target === null ? undefined : pending.label) : currentCycleName;

  useEffect(() => {
    if (!open) return;
    function onOutside(e: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) onOpenChange(false);
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
        const o = options[activeIdx];
        if (o) choose(o.id);
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        onOpenChange(false);
      }
    }
    document.addEventListener("mousedown", onOutside);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onOutside);
      window.removeEventListener("keydown", onKey, true);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, options, activeIdx]);

  return (
    <div ref={wrapRef} className="relative">
      <button
        type="button"
        data-nav-key="cycle"
        data-nav-selected={navKey === "cycle" || undefined}
        onClick={() => onOpenChange(!open)}
        className={`flex items-center gap-1.5 rounded px-1 outline-none transition-colors hover:bg-secondary/40 ${
          navKey === "cycle" ? "ring-1 ring-foreground/50" : ""
        }`}
      >
        <IterationCw
          size={12}
          className={`shrink-0 ${displayName ? "text-muted-foreground" : "text-muted-foreground/60"}`}
        />
        <span className={displayName ? "text-foreground/80" : "text-muted-foreground/70"}>
          {displayName ?? "Add to cycle"}
        </span>
      </button>
      {open && (
        <div
          data-floating-popup
          className="absolute left-0 top-full z-50 mt-1 min-w-40 rounded-md border border-border bg-card shadow-md"
        >
          <div className="max-h-48 overflow-y-auto py-0.5">
            {options.map((o, i) => (
              <button
                key={o.id || "none"}
                type="button"
                data-opt-idx={i}
                data-nav-selected={i === activeIdx || undefined}
                onMouseEnter={() => setActiveIdx(i)}
                onClick={() => void choose(o.id)}
                className={`flex w-full items-center gap-2 px-2 py-1 text-left text-[11px] transition-colors data-[nav-selected=true]:bg-accent data-[nav-selected=true]:text-accent-foreground ${
                  o.id === (currentCycleId ?? "") ? "text-foreground" : "text-muted-foreground"
                }`}
              >
                <IterationCw size={11} className="shrink-0" />
                {o.label}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ── Body helpers ──

/// Renders a small round avatar: img when `avatarUrl` is present and loads,
/// falls back to initials from `name`. Size is in pixels.
function AssigneeAvatar({ name, avatarUrl, size = 16 }: { name: string; avatarUrl?: string; size?: number }) {
  const [imgFailed, setImgFailed] = useState(false);
  const initials = name.slice(0, 2).toUpperCase();

  if (avatarUrl && !imgFailed) {
    return (
      <img
        src={avatarUrl}
        alt={name}
        width={size}
        height={size}
        onError={() => setImgFailed(true)}
        className="shrink-0 rounded-full object-cover"
        style={{ width: size, height: size }}
      />
    );
  }
  return (
    <span
      className="flex shrink-0 items-center justify-center rounded-full bg-secondary text-foreground/70"
      style={{ width: size, height: size, fontSize: Math.max(8, size * 0.5) }}
    >
      {initials}
    </span>
  );
}

/// Maps raw Linear relation types to user-facing labels.
function friendlyRelationType(type: string): string {
  switch (type) {
    case "blocks": return "blocks";
    case "blocked_by": return "blocked by";
    case "duplicate_of": return "duplicate of";
    case "duplicated_by": return "duplicated by";
    case "related": return "related to";
    default: return type.replace(/_/g, " ");
  }
}

// ── Body (shared by modal + future tab) ──

function SectionCaps({ label }: { label: string }) {
  return (
    <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
      {label}
    </span>
  );
}

export function LinearIssueBody({
  c,
  layout,
}: {
  c: LinearIssueController;
  layout: "modal" | "tab";
}) {
  const isTab = layout === "tab";
  const { detail, issue, loading, error, comments, attachments, relations, activity, subIssues, childMap, closedOut } = c;

  const setOuterRef = (el: HTMLDivElement | null) => {
    c.contentRef.current = el;
    if (isTab) c.mainRef.current = el;
  };

  const placeholder =
    loading && !detail ? (
      <div className="flex h-full w-full flex-col items-center justify-center gap-2 px-6">
        <ProgressBar className="max-w-32" />
        <span className="text-xs text-muted-foreground">Loading…</span>
      </div>
    ) : error ? (
      <div className="flex h-full w-full items-center justify-center px-6 text-center text-xs text-red-400">
        {error}
      </div>
    ) : null;

  if (placeholder || !issue) {
    return (
      <div
        ref={setOuterRef}
        tabIndex={0}
        className={`outline-none ${isTab ? "h-full overflow-y-auto" : "flex h-full"}`}
      >
        {placeholder}
      </div>
    );
  }

  const priorityStr = linearPriorityStr(issue.priority);

  const now = Date.now();

  const TSHIRT_LABELS: Record<number, string> = { 1: "XS", 2: "S", 3: "M", 4: "L", 5: "XL", 6: "XXL" };

  const cardClass =
    "flex flex-col gap-1.5 rounded-lg border border-border/40 bg-secondary/20 px-2.5 py-2";

  // Linear's rail is three stacked cards: Properties (status/priority/assignee/
  // estimate/cycle + meta), Labels, Project.
  const propertiesEl = (
    <>
      <div className={cardClass}>
        <SectionCaps label="Properties" />
        <div className="flex flex-col gap-1.5 text-[11px]">
        {closedOut && issue && (
          <div className="flex items-center gap-1.5">
            <span className="rounded-full bg-green-500/15 px-1.5 text-[9px] font-medium text-green-400">done</span>
            <button
              type="button"
              data-nav-key="unclose"
              data-nav-selected={c.navKey === "unclose" || undefined}
              onClick={() => void c.handleUncloseOut(issue.id)}
              className="rounded px-1 text-[9px] text-muted-foreground outline-none hover:text-foreground"
              title="Unmark closed out"
            >
              Mark undone
            </button>
          </div>
        )}
        {issue.teamId && (
          <StatePickerRail
            issueId={issue.id}
            teamId={issue.teamId}
            currentStateId={issue.stateId}
            currentStateType={issue.stateType}
            currentStateColor={issue.stateColor}
            currentStateName={issue.stateName}
            navKey={c.navKey}
            open={c.statusPickerOpen}
            onOpenChange={c.setStatusPickerOpen}
          />
        )}
        {priorityStr && (
          <div className="flex items-center gap-1.5">
            <PriorityIcon priority={priorityStr} size={13} className="shrink-0" />
            <span className="capitalize text-foreground/80">{priorityStr}</span>
          </div>
        )}
        {issue.assigneeName && (
          <div className="flex items-center gap-1.5">
            <AssigneeAvatar name={issue.assigneeName} avatarUrl={issue.assigneeAvatarUrl} size={16} />
            <span className="truncate text-foreground/80">{issue.assigneeName}</span>
          </div>
        )}
        {issue.estimate != null && (
          issue.estimationType === "tShirt" ? (
            // Linear shows t-shirt estimates as its estimate glyph + size letter.
            <div className="flex items-center gap-1.5 text-foreground/80">
              <LinearEstimateIcon size={12} className="shrink-0 text-muted-foreground" />
              <span className="font-medium">{TSHIRT_LABELS[issue.estimate] ?? issue.estimate}</span>
            </div>
          ) : (
            <div className="flex items-center gap-1.5 text-foreground/80">
              <span className="flex size-4 shrink-0 items-center justify-center rounded bg-secondary text-[9px] font-semibold tabular-nums">
                {issue.estimate}
              </span>
              <span className="text-muted-foreground">points</span>
            </div>
          )
        )}
        {issue.teamId && (
          <CyclePicker
            issueId={issue.id}
            teamId={issue.teamId}
            currentCycleId={issue.cycleId}
            currentCycleName={issue.cycleName}
            navKey={c.navKey}
            open={c.cyclePickerOpen}
            onOpenChange={c.setCyclePickerOpen}
          />
        )}
        {issue.dueDate != null && (
          <div className="flex items-center gap-1.5">
            <span
              className={`text-[10px] tabular-nums ${issue.dueDate * 1000 < now ? "text-red-400" : "text-foreground/80"}`}
              data-nav-key="due"
              data-nav-selected={c.navKey === "due" || undefined}
            >
              Due {formatDate(issue.dueDate)}
            </span>
          </div>
        )}
        {issue.createdAt && (
          <div className="text-[10px] text-muted-foreground">
            Created {formatDate(issue.createdAt)}
          </div>
        )}
        {issue.updatedAt && (
          <div className="text-[10px] text-muted-foreground">
            Updated {formatDate(issue.updatedAt)}
          </div>
        )}
        {issue.identifier && (
          <button
            type="button"
            data-nav-key="copyid"
            data-nav-selected={c.navKey === "copyid" || undefined}
            onClick={() => c.activateNav("copyid")}
            className="self-start rounded px-1 font-mono text-[10px] text-muted-foreground/60 outline-none hover:text-foreground"
            title="Copy issue id"
          >
            {issue.identifier}
          </button>
        )}
        {issue.url && (
          <button
            type="button"
            data-nav-key="open"
            data-nav-selected={c.navKey === "open" || undefined}
            onClick={() => void openValidatedUrl(issue.url)}
            className="self-start rounded px-1 text-[10px] text-accent underline outline-none hover:no-underline"
          >
            Open in Linear
          </button>
        )}
        </div>
      </div>

      {issue.labels.length > 0 && (
        <div className={cardClass}>
          <SectionCaps label="Labels" />
          <div className="flex flex-wrap gap-1">
            {issue.labels.map((label) => (
              <span
                key={label.id}
                className="rounded-full px-1.5 text-[10px] leading-4"
                style={{
                  background: label.color ? `${label.color}33` : "var(--color-secondary)",
                  color: label.color ?? "var(--color-secondary-foreground)",
                }}
              >
                {label.name}
              </span>
            ))}
          </div>
        </div>
      )}

      {issue.projectName && (
        <div className={cardClass}>
          <SectionCaps label="Project" />
          <div className="flex items-center gap-1.5 text-[11px] text-foreground/80">
            <span className="size-2 shrink-0 rounded-sm bg-primary/40" aria-hidden />
            <span className="truncate">{issue.projectName}</span>
          </div>
        </div>
      )}
    </>
  );

  const descEl = (
    <>
      <SectionCaps label="Description" />
      {issue.description && issue.description.trim() ? (
        <div className="-mx-3 -my-2 rounded">
          <MarkdownView content={issue.description} gateRemoteImages linearAssets />
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">No description</p>
      )}
    </>
  );

  // Sub-issues with full recursive tree (default expanded in modal, §9).
  const subIssuesEl = subIssues.length > 0 ? (
    <>
      <SectionCaps label={`Sub-issues · ${subIssues.length}`} />
      <div className="flex flex-col gap-0.5">
        {subIssues.map((sub) => (
          <SubIssueNode
            key={sub.id}
            issue={sub}
            childMap={childMap}
            depth={0}
            seen={new Set<string>([issue.id])}
            onOpen={(id) => c.setIssueId(id)}
            navKey={c.navKey}
          />
        ))}
      </div>
    </>
  ) : null;

  // Build comment thread map: top-level + replies by parentId.
  const topLevelComments = comments.filter((comment) => !comment.parentId);
  const repliesMap = new Map<string, LinearComment[]>();
  for (const comment of comments) {
    if (comment.parentId) {
      const arr = repliesMap.get(comment.parentId) ?? [];
      arr.push(comment);
      repliesMap.set(comment.parentId, arr);
    }
  }

  function renderComment(comment: LinearComment, isReply: boolean) {
    return (
      <div
        key={comment.id}
        className={`rounded border bg-secondary/20 ${isReply ? "border-border/30 bg-secondary/10" : "border-border/50"}`}
      >
        <div className="flex items-center gap-1.5 px-2 pt-1.5 text-[10px] text-muted-foreground">
          <span className="flex size-4 items-center justify-center rounded-full bg-secondary text-[8px] font-medium text-foreground/70">
            {(comment.author ?? "?").slice(0, 2).toUpperCase()}
          </span>
          <span className="font-medium text-foreground/70">{comment.author ?? "Unknown"}</span>
          {comment.createdAt != null && (
            <span>{formatDate(comment.createdAt)}</span>
          )}
        </div>
        <div className="-my-1.5">
          <MarkdownView content={comment.body ?? ""} gateRemoteImages linearAssets />
        </div>
      </div>
    );
  }

  const commentsEl = (
    <>
      <SectionCaps label={`Comments · ${comments.length}`} />
      {comments.length === 0 ? (
        <p className="text-xs text-muted-foreground">No comments</p>
      ) : (
        <div className="flex flex-col gap-2">
          {topLevelComments.map((comment) => (
            <div key={comment.id}>
              {renderComment(comment, false)}
              {(repliesMap.get(comment.id) ?? []).map((reply) => (
                <div key={reply.id} className="ml-4 mt-1">
                  {renderComment(reply, true)}
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
      {/* Inline comment composer — token-gated, close_out=false (posting must NOT close the issue) */}
      <div className="mt-2 flex flex-col gap-1.5">
        <textarea
          ref={c.commentRef}
          data-nav-key="comment"
          value={c.commentDraft}
          onChange={(e) => c.setCommentDraft(e.target.value)}
          placeholder="Write a comment… (Ctrl+Enter to post)"
          rows={2}
          className={`w-full resize-none rounded border bg-background px-2 py-1.5 text-[11px] leading-relaxed outline-none placeholder:text-muted-foreground/50 focus:ring-1 focus:ring-foreground/30 ${
            c.navKey === "comment" ? "border-foreground/40" : "border-border"
          }`}
          onKeyDown={(e) => {
            if (e.key === "Enter" && e.ctrlKey) {
              e.preventDefault();
              void c.handlePostComment();
            }
            // Escape backs out to the cursor (keeps the draft) so the keyboard
            // can return to the elements above — mirrors ClickUp's composer.
            if (e.key === "Escape") {
              e.preventDefault();
              e.stopPropagation();
              c.setNavKey("comment");
              c.contentRef.current?.focus({ preventScroll: true });
            }
          }}
        />
        {c.commentDraft.trim() && (
          <div className="flex justify-end gap-1">
            <button
              type="button"
              onClick={() => {
                c.setCommentDraft("");
                c.contentRef.current?.focus({ preventScroll: true });
              }}
              className="rounded px-2 py-0.5 text-[10px] text-muted-foreground hover:text-foreground"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => void c.handlePostComment()}
              disabled={c.postingComment}
              className="rounded bg-primary px-2 py-0.5 text-[10px] text-primary-foreground disabled:opacity-50"
            >
              {c.postingComment ? "Posting…" : "Post"}
            </button>
          </div>
        )}
      </div>
    </>
  );

  const attachmentsEl = attachments.length > 0 ? (
    <>
      <SectionCaps label={`Attachments · ${attachments.length}`} />
      <div className="flex flex-wrap gap-1.5">
        {attachments.map((att) => (
          <button
            key={att.id}
            type="button"
            data-nav-key={`att:${att.id}`}
            data-nav-selected={c.navKey === `att:${att.id}` || undefined}
            onClick={() => void openValidatedUrl(att.url)}
            className="flex items-center gap-1 rounded border border-border/60 bg-secondary/30 px-1.5 py-0.5 text-[10px] text-foreground/80 outline-none transition-colors hover:bg-secondary/60 hover:text-foreground"
          >
            <Paperclip size={9} className="shrink-0 text-muted-foreground" />
            <span className="max-w-32 truncate">{att.title ?? att.subtitle ?? att.url}</span>
            <ExternalLink size={9} className="shrink-0 text-muted-foreground/60" />
          </button>
        ))}
      </div>
    </>
  ) : null;

  const relationsEl = relations.length > 0 ? (
    <>
      <SectionCaps label={`Relations · ${relations.length}`} />
      <div className="flex flex-col gap-1">
        {relations.map((rel) => (
          <button
            key={`${rel.relationType}:${rel.relatedId}`}
            type="button"
            data-nav-key={`rel:${rel.relatedId}`}
            data-nav-selected={c.navKey === `rel:${rel.relatedId}` || undefined}
            onClick={() => c.setIssueId(rel.relatedId)}
            className="flex items-center gap-1.5 rounded px-1 py-0.5 text-left text-[11px] outline-none transition-colors hover:bg-secondary/40"
          >
            <span className="shrink-0 text-[10px] capitalize text-muted-foreground">
              {friendlyRelationType(rel.relationType)}
            </span>
            {rel.relatedIdentifier && (
              <span className="shrink-0 font-mono text-[10px] text-muted-foreground/60">
                {rel.relatedIdentifier}
              </span>
            )}
            <span className="min-w-0 truncate text-foreground/80">
              {rel.relatedTitle ?? rel.relatedId}
            </span>
          </button>
        ))}
      </div>
    </>
  ) : null;

  const activityEl = activity.length > 0 ? (
    <>
      <div className="border-t border-border/50" />
      <SectionCaps label="Activity" />
      {/* Timeline: actor avatar (create/assign) or action-icon nodes on a vertical
          spine — mirrors Linear's activity feed. */}
      <div className="flex flex-col">
        {activity.map((e, i) => (
          <div key={e.id} className="relative flex gap-2">
            {i < activity.length - 1 && (
              <span className="absolute left-[8px] top-[16px] bottom-0 w-px bg-border/50" aria-hidden />
            )}
            {e.kind === "created" || e.kind === "assignee" ? (
              <span className="relative z-10 shrink-0">
                <AssigneeAvatar name={e.actor ?? "Linear"} avatarUrl={e.actorAvatarUrl} size={16} />
              </span>
            ) : (
              <span className="relative z-10 flex size-4 shrink-0 items-center justify-center text-muted-foreground">
                {activityIcon(e, 11)}
              </span>
            )}
            <div className="min-w-0 flex-1 pb-3 pt-px text-[10px] leading-snug">
              <span className="font-medium text-foreground/80">{e.actor ?? "Linear"}</span>
              <span className="ml-1 text-muted-foreground">{activityVerb(e, issue.estimationType === "tShirt")}</span>
              {e.createdAt != null && (
                <span className="ml-1 text-muted-foreground/50">· {formatRelative(e.createdAt)}</span>
              )}
            </div>
          </div>
        ))}
      </div>
    </>
  ) : null;

  if (isTab) {
    return (
      <div
        ref={setOuterRef}
        tabIndex={0}
        onKeyDown={c.handleNavKeyDown}
        onClick={c.handleContainerClick}
        className="h-full overflow-y-auto outline-none"
      >
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-3 px-5 py-4">
          {propertiesEl}
          <div className="border-t border-border" />
          {descEl}
          {subIssuesEl}
          {attachmentsEl}
          {relationsEl}
          {activityEl}
          {commentsEl}
        </div>
      </div>
    );
  }

  // Modal: properties rail on the right (order-2), main content scrolls left
  return (
    <div
      ref={c.contentRef}
      tabIndex={0}
      onKeyDown={c.handleNavKeyDown}
      onClick={c.handleContainerClick}
      className="flex h-full outline-none"
    >
      <aside className="order-2 flex w-48 shrink-0 flex-col gap-2 overflow-y-auto border-l border-border/40 px-2 py-2">
        {propertiesEl}
      </aside>
      <main ref={c.mainRef} className="order-1 flex min-w-0 flex-1 flex-col gap-3 overflow-y-auto px-3 py-2">
        {descEl}
        {subIssuesEl}
        {attachmentsEl}
        {relationsEl}
        {activityEl}
        {commentsEl}
      </main>
    </div>
  );
}

/// Drill-in history chevrons for the floating modal title bar (mirrors TaskHistoryNav).
export function LinearIssueHistoryNav({ c }: { c: LinearIssueController }) {
  return (
    <>
      <button
        type="button"
        aria-label="Back"
        disabled={!c.histNav.canBack}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={() => c.stepHistory(-1)}
        className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground disabled:opacity-25 disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
      >
        <ChevronLeft size={14} />
      </button>
      <button
        type="button"
        aria-label="Forward"
        disabled={!c.histNav.canFwd}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={() => c.stepHistory(1)}
        className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground disabled:opacity-25 disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
      >
        <ChevronRight size={14} />
      </button>
    </>
  );
}

/// Title bar content for the floating modal.
export function LinearIssueTitleContent({ c }: { c: LinearIssueController }) {
  return (
    <>
      <LinearStatusIcon
        stateType={c.issue?.stateType}
        color={c.issue?.stateColor ?? null}
        size={13}
        className="shrink-0"
        title={c.issue?.stateName}
      />
      <span className="truncate text-sm font-medium text-foreground">
        {c.issue?.title ?? "Linear issue"}
      </span>
    </>
  );
}

/// A single toolbar verb button with a tooltip (mirrors ClickUp's ToolbarAction).
export function ToolbarAction({
  label,
  onClick,
  active = false,
  children,
}: {
  label: string;
  onClick: () => void;
  active?: boolean;
  children: React.ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            aria-label={label}
            onClick={onClick}
            className={`flex size-5 items-center justify-center rounded transition-colors ${
              active
                ? "text-primary hover:bg-secondary/60"
                : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
            }`}
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent side="bottom">{label}</TooltipContent>
    </Tooltip>
  );
}

/// Issue → agent verbs for the floating-detail toolbar: send / spawn / pin /
/// bind, plus an explicit re-inject when the issue is bound or pinned.
/// Write controls: assign-to-me/unassign, close-out (state lives in the rail).
export function LinearVerbToolbar({ issueId }: { issueId: string }) {
  const boundIssueId = useAtomValue(activeSessionLinearIssueAtom);
  const pinnedIssueIds = useAtomValue(activeSessionLinearPinsAtom);
  const requestSend = useSetAtom(requestSendIssueAction);
  const spawnWorktree = useSetAtom(spawnWorktreeWithIssueAction);
  const togglePin = useSetAtom(togglePinIssueAction);
  const requestBind = useSetAtom(requestBindIssueAction);
  const reinject = useSetAtom(reinjectIssueAction);
  const issues = useAtomValue(linearIssuesAtom);
  const syncStatus = useAtomValue(linearSyncStatusAtom);
  const setOverlay = useSetAtom(linearOverlayAtom);
  const addToast = useSetAtom(toastsAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const [, setClosureOffer] = useAtom(linearClosureOfferAtom);

  const isPinned = pinnedIssueIds.includes(issueId);
  const isBound = issueId === boundIssueId;

  const issue = issues.find((i) => i.id === issueId);
  const viewerId = syncStatus?.viewerId;
  const isAssignedToMe = viewerId ? issue?.assigneeId === viewerId : false;

  async function handleAssignToMe() {
    if (!viewerId) {
      addToast({
        message: "Cannot assign",
        description: "Viewer identity unresolved — check Linear sync status.",
        type: "error",
      });
      return;
    }
    setLinearOverlayEntry(setOverlay, issueId, "assignee", viewerId);
    try {
      await invoke("linear_set_assignee", { issueId, assigneeId: viewerId });
    } catch (err) {
      clearLinearOverlayEntry(setOverlay, issueId, "assignee");
      addToast({ message: "Assign failed", description: String(err), type: "error" });
    }
  }

  async function handleUnassign() {
    setLinearOverlayEntry(setOverlay, issueId, "assignee", null);
    try {
      await invoke("linear_set_assignee", { issueId, assigneeId: null });
    } catch (err) {
      clearLinearOverlayEntry(setOverlay, issueId, "assignee");
      addToast({ message: "Unassign failed", description: String(err), type: "error" });
    }
  }

  function handleCloseOut() {
    if (!activeSessionId) {
      addToast({ message: "Close out", description: "No active session.", type: "info" });
      return;
    }
    setClosureOffer({ issueId, sessionId: activeSessionId });
  }

  return (
    <>
      {/* Write controls: assign/unassign, close-out. State lives in the
          Properties rail (StatePickerRail) — no duplicate picker in the header. */}
      {viewerId && (
        <ToolbarAction
          label={isAssignedToMe ? "Unassign (assigned to you)" : "Assign to me"}
          onClick={() => void (isAssignedToMe ? handleUnassign() : handleAssignToMe())}
          active={isAssignedToMe}
        >
          {isAssignedToMe ? <UserMinus size={12} /> : <UserCheck size={12} />}
        </ToolbarAction>
      )}
      <ToolbarAction label={LINEAR_ACTION_LABELS.closeOut} onClick={handleCloseOut}>
        <CheckSquare size={12} />
      </ToolbarAction>

      {/* Separator */}
      <span className="mx-0.5 h-3 w-px shrink-0 bg-border" aria-hidden />

      {/* Existing agent verbs */}
      <ToolbarAction label={LINEAR_ACTION_LABELS.send} onClick={() => requestSend(issueId)}>
        <Send size={12} />
      </ToolbarAction>
      <ToolbarAction label={LINEAR_ACTION_LABELS.spawn} onClick={() => void spawnWorktree(issueId)}>
        <GitBranchPlus size={12} />
      </ToolbarAction>
      <ToolbarAction
        label={isPinned ? LINEAR_ACTION_LABELS.unpin : LINEAR_ACTION_LABELS.pin}
        onClick={() => void togglePin(issueId)}
        active={isPinned}
      >
        {isPinned ? <PinOff size={12} /> : <Pin size={12} />}
      </ToolbarAction>
      <ToolbarAction
        label={isBound ? LINEAR_ACTION_LABELS.unbind : LINEAR_ACTION_LABELS.bind}
        onClick={() => void requestBind(issueId)}
        active={isBound}
      >
        {isBound ? <Unlink size={12} /> : <Link2 size={12} />}
      </ToolbarAction>
      {(isBound || isPinned) && (
        <ToolbarAction label={LINEAR_ACTION_LABELS.reinject} onClick={() => void reinject(issueId)}>
          <RefreshCw size={12} />
        </ToolbarAction>
      )}
    </>
  );
}
