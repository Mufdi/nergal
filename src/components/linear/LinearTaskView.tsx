import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent as ReactMouseEvent } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { open as openShell } from "@tauri-apps/plugin-shell";
import {
  CheckSquare,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  GitBranch,
  GitBranchPlus,
  Link2,
  MessageSquarePlus,
  Paperclip,
  Pin,
  PinOff,
  RefreshCw,
  Send,
  Unlink,
  UserCheck,
  UserMinus,
} from "lucide-react";
import { invoke } from "@/lib/tauri";
import { MarkdownView } from "@/components/plan/MarkdownView";
import { LinearStatusIcon } from "@/components/linear/LinearStatusIcon";
import { PriorityIcon } from "@/components/clickup/PriorityIcon";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  activeSessionLinearIssueAtom,
  activeSessionLinearPinsAtom,
  clearLinearOverlayEntry,
  copyLinearIssueAction,
  LINEAR_ACTION_LABELS,
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

function formatDate(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

/// Compact relative time ("3mo ago", "2d ago") for the activity feed.
function formatRelative(ms: number): string {
  const diff = Date.now() - ms;
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

/// The human verb for an activity entry, derived from its kind + from/to.
function activityVerb(e: LinearActivityEntry): string {
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
    default:
      return "updated the issue";
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
  const [detail, setDetail] = useState<LinearDetailData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
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

  // The cursor is state-driven (no DOM focus), so the scroll container won't
  // follow it — bring the highlighted element into view.
  useEffect(() => {
    if (!navKey) return;
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
  const issue = detail?.issue ?? mirrorIssue;
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
  const { detail, issue, loading, error, comments, attachments, relations, activity, subIssues, childMap } = c;

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

  const propertiesEl = (
    <div className="flex flex-col gap-2">
      <SectionCaps label="Properties" />
      <div className="flex flex-col gap-1.5 text-[11px]">
        {issue.stateName && (
          <div className="flex items-center gap-1.5">
            <LinearStatusIcon stateType={issue.stateType} color={issue.stateColor ?? null} size={13} className="shrink-0" />
            <span className="text-foreground/80">{issue.stateName}</span>
          </div>
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
          <div className="flex items-center gap-1.5 text-foreground/80">
            <span className="flex size-4 shrink-0 items-center justify-center rounded bg-secondary text-[9px] font-semibold tabular-nums">
              {issue.estimate}
            </span>
            <span className="text-muted-foreground">points</span>
          </div>
        )}
        {issue.dueDate != null && (
          <div className="flex items-center gap-1.5">
            <span
              className={`text-[10px] tabular-nums ${issue.dueDate < now ? "text-red-400" : "text-foreground/80"}`}
            >
              Due {formatDate(issue.dueDate)}
            </span>
          </div>
        )}
        {issue.cycleName && (
          <div className="flex items-center gap-1.5 text-foreground/80">
            <GitBranch size={12} className="shrink-0 text-muted-foreground" />
            <span className="truncate">{issue.cycleName}</span>
          </div>
        )}
        {issue.projectName && (
          <div className="flex items-center gap-1.5 text-foreground/80">
            <span className="size-2 shrink-0 rounded-sm bg-primary/40" aria-hidden />
            <span className="truncate">{issue.projectName}</span>
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

      {issue.labels.length > 0 && (
        <div className="flex flex-col gap-1">
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
    </div>
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

  const commentsEl = (
    <>
      <SectionCaps label={`Comments · ${comments.length}`} />
      {comments.length === 0 ? (
        <p className="text-xs text-muted-foreground">No comments</p>
      ) : (
        <div className="flex flex-col gap-2">
          {comments.map((comment) => (
            <div key={comment.id} className="rounded border border-border/50 bg-secondary/20">
              <div className="flex items-center gap-1.5 px-2 pt-1.5 text-[10px] text-muted-foreground">
                <span
                  className="flex size-4 items-center justify-center rounded-full bg-secondary text-[8px] font-medium text-foreground/70"
                >
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
          ))}
        </div>
      )}
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
      <SectionCaps label="Activity" />
      <div className="flex flex-col gap-1.5">
        {activity.map((e) => (
          <div key={e.id} className="flex items-baseline gap-1.5 text-[11px] text-muted-foreground">
            <span className="font-medium text-foreground/70">{e.actor ?? "Linear"}</span>
            <span className="min-w-0 flex-1">{activityVerb(e)}</span>
            {e.createdAt != null && (
              <span className="shrink-0 text-[10px] text-muted-foreground/60">{formatRelative(e.createdAt)}</span>
            )}
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
      <aside className="order-2 flex w-44 shrink-0 flex-col gap-3 border-l border-border/50 px-2.5 py-2">
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

/// State picker dropdown for the toolbar write controls.
function StatePickerToolbar({
  issueId,
  teamId,
  currentStateId,
}: {
  issueId: string;
  teamId: string;
  currentStateId?: string;
}) {
  const [states, setStates] = useState<WorkflowStateView[]>([]);
  const [open, setOpen] = useState(false);
  const setOverlay = useSetAtom(linearOverlayAtom);
  const addToast = useSetAtom(toastsAtom);

  useEffect(() => {
    if (!teamId) return;
    invoke<WorkflowStateView[]>("linear_read_team_states", { teamId })
      .then(setStates)
      .catch(() => {});
  }, [teamId]);

  const currentState = states.find((s) => s.id === currentStateId);

  async function handleSelect(stateId: string) {
    setOpen(false);
    if (stateId === currentStateId) return;
    // Optimistic overlay (Decision 1)
    setLinearOverlayEntry(setOverlay, issueId, "state", stateId);
    try {
      await invoke("linear_set_issue_state", { issueId, stateId });
      // Poll reconcile will clear the overlay on the next mirror refresh.
    } catch (err) {
      clearLinearOverlayEntry(setOverlay, issueId, "state");
      addToast({ message: "State change failed", description: String(err), type: "error" });
    }
  }

  if (states.length === 0) return null;

  return (
    <div className="relative">
      <Tooltip>
        <TooltipTrigger
          render={
            <button
              type="button"
              aria-label="Change state"
              onClick={() => setOpen((o) => !o)}
              className="flex size-5 items-center justify-center gap-1 rounded text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
            />
          }
        >
          {currentState ? (
            <LinearStatusIcon stateType={currentState.type} color={currentState.color ?? null} size={12} className="shrink-0" />
          ) : (
            <LinearStatusIcon stateType={undefined} color={null} size={12} className="shrink-0" />
          )}
        </TooltipTrigger>
        <TooltipContent side="bottom">Change state</TooltipContent>
      </Tooltip>

      {open && (
        <div className="absolute right-0 top-full z-50 mt-1 min-w-36 rounded-md border border-border bg-popover shadow-md">
          <div className="max-h-48 overflow-y-auto py-0.5">
            {states.map((s) => (
              <button
                key={s.id}
                type="button"
                onClick={() => void handleSelect(s.id)}
                className={`flex w-full items-center gap-2 px-2 py-1 text-left text-[11px] transition-colors hover:bg-secondary/50 ${s.id === currentStateId ? "text-foreground" : "text-muted-foreground"}`}
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

/// Comment composer (token-gated): textarea + send button.
/// Routes through linear_request_comment_token → linear_execute_gated_write
/// with close_out=false — posting a comment must NOT close out the issue.
function CommentComposerToolbar({ issueId }: { issueId: string }) {
  const [open, setOpen] = useState(false);
  const [comment, setComment] = useState("");
  const [sending, setSending] = useState(false);
  const addToast = useSetAtom(toastsAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);

  async function handleSend() {
    if (!comment.trim() || sending || !activeSessionId) return;
    setSending(true);
    try {
      // Token-gated: routes through request_comment_token (close_out=false)
      // → execute_gated_write. This path posts the comment ONLY — it does NOT
      // close out the issue (discriminant enforced by the backend token).
      const token = await invoke<string>("linear_request_comment_token", {
        issueId,
        comment: comment.trim(),
      });
      await invoke("linear_execute_gated_write", { token, sessionId: activeSessionId });
      addToast({ message: "Comment posted", description: comment.trim().slice(0, 60), type: "success" });
      setComment("");
      setOpen(false);
    } catch (err) {
      addToast({ message: "Comment failed", description: String(err), type: "error" });
    } finally {
      setSending(false);
    }
  }

  return (
    <div className="relative">
      <Tooltip>
        <TooltipTrigger
          render={
            <button
              type="button"
              aria-label="Post comment"
              onClick={() => setOpen((o) => !o)}
              className="flex size-5 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
            />
          }
        >
          <MessageSquarePlus size={12} />
        </TooltipTrigger>
        <TooltipContent side="bottom">Post comment</TooltipContent>
      </Tooltip>

      {open && (
        <div className="absolute right-0 top-full z-50 mt-1 w-64 rounded-md border border-border bg-popover p-2 shadow-md">
          <textarea
            autoFocus
            value={comment}
            onChange={(e) => setComment(e.target.value)}
            placeholder="Write a comment…"
            rows={3}
            className="w-full resize-none rounded border border-border bg-background px-2 py-1.5 text-[11px] leading-relaxed outline-none placeholder:text-muted-foreground/60 focus:ring-1 focus:ring-foreground/30"
            onKeyDown={(e) => {
              if (e.key === "Enter" && e.ctrlKey) {
                e.preventDefault();
                void handleSend();
              }
              if (e.key === "Escape") {
                e.preventDefault();
                setOpen(false);
              }
            }}
          />
          <div className="mt-1.5 flex justify-end gap-1">
            <button
              type="button"
              onClick={() => setOpen(false)}
              className="rounded px-2 py-0.5 text-[10px] text-muted-foreground hover:text-foreground"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => void handleSend()}
              disabled={!comment.trim() || sending}
              className="rounded bg-primary px-2 py-0.5 text-[10px] text-primary-foreground disabled:opacity-50"
            >
              {sending ? "Sending…" : "Send"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/// Issue → agent verbs for the floating-detail toolbar: send / spawn / pin /
/// bind, plus an explicit re-inject when the issue is bound or pinned.
/// Write controls: state picker, assign-to-me/unassign, comment composer, close-out.
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
      {/* Write controls: state picker, assign/unassign, comment composer, close-out */}
      {issue && (
        <StatePickerToolbar
          issueId={issueId}
          teamId={issue.teamId}
          currentStateId={issue.stateId}
        />
      )}
      {viewerId && (
        <ToolbarAction
          label={isAssignedToMe ? "Unassign (assigned to you)" : "Assign to me"}
          onClick={() => void (isAssignedToMe ? handleUnassign() : handleAssignToMe())}
          active={isAssignedToMe}
        >
          {isAssignedToMe ? <UserMinus size={12} /> : <UserCheck size={12} />}
        </ToolbarAction>
      )}
      <CommentComposerToolbar issueId={issueId} />
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
