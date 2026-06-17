import { useEffect, useMemo, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { ChevronDown, ChevronRight } from "lucide-react";
import { focusIfPanelZone } from "@/lib/panelFocus";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { StatusIcon } from "@/components/clickup/StatusIcon";
import { PriorityIcon } from "@/components/clickup/PriorityIcon";
import { zenModeAtom } from "@/stores/zenMode";
import {
  GROUP_BY_ORDER,
  linearAssignedToMeAtom,
  linearDetailIssueIdAtom,
  linearGroupByAtom,
  linearIssuesAtom,
  linearShowCompletedAtom,
  linearSyncStatusAtom,
  linearTeamFilterAtom,
  linearTeamsAtom,
  type IssueView,
  type LinearGroupBy,
} from "@/stores/linear";

// ── Priority mapping: Linear int → PriorityIcon string ──
// 0=none, 1=urgent, 2=high, 3=medium, 4=low
function linearPriorityStr(p: number): string | null {
  switch (p) {
    case 1: return "urgent";
    case 2: return "high";
    case 3: return "normal";
    case 4: return "low";
    default: return null;
  }
}

// ── State-type mapping: Linear stateType → StatusIcon type ──
function linearStateToIconType(stateType: string | undefined): string | null {
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

// ── View chip strip ──
type LinearView = "mine" | LinearGroupBy;
const VIEW_ORDER: LinearView[] = ["mine", ...GROUP_BY_ORDER];
const VIEW_LABEL: Record<LinearView, string> = {
  mine: "My issues",
  state: "State",
  project: "Project",
  assignee: "Assignee",
};

interface IssueGroup {
  key: string;
  label: string;
  stateType: string | null;
  color: string | null;
  issues: IssueView[];
}

function isTerminal(stateType: string | undefined): boolean {
  return stateType === "completed" || stateType === "cancelled" || stateType === "canceled";
}

function groupIssues(issues: IssueView[], groupBy: LinearGroupBy): IssueGroup[] {
  const groups = new Map<string, IssueGroup>();
  const push = (key: string, label: string, stateType: string | null, color: string | null, issue: IssueView) => {
    let g = groups.get(key);
    if (!g) {
      g = { key, label, stateType, color, issues: [] };
      groups.set(key, g);
    }
    g.issues.push(issue);
  };
  for (const issue of issues) {
    if (groupBy === "state") {
      const label = issue.stateName ?? "No state";
      push(`state:${label}`, label, issue.stateType ?? null, issue.stateColor ?? null, issue);
    } else if (groupBy === "project") {
      const label = issue.projectName ?? "No project";
      push(`project:${issue.projectId ?? "none"}`, label, null, null, issue);
    } else {
      const label = issue.assigneeName ?? "Unassigned";
      push(`assignee:${issue.assigneeId ?? "none"}`, label, null, null, issue);
    }
  }
  return [...groups.values()];
}

export function LinearPanel() {
  const syncStatus = useAtomValue(linearSyncStatusAtom);
  const issues = useAtomValue(linearIssuesAtom);
  const teams = useAtomValue(linearTeamsAtom);
  const [teamFilter, setTeamFilter] = useAtom(linearTeamFilterAtom);
  const [groupBy, setGroupBy] = useAtom(linearGroupByAtom);
  const [assignedToMe, setAssignedToMe] = useAtom(linearAssignedToMeAtom);
  const [showCompleted, setShowCompleted] = useAtom(linearShowCompletedAtom);
  const setDetailIssueId = useSetAtom(linearDetailIssueIdAtom);
  const zenOpen = useAtomValue(zenModeAtom).open;
  const rootRef = useRef<HTMLDivElement>(null);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());

  // Derived active view chip
  const activeView: LinearView = assignedToMe && groupBy === "state" ? "mine" : groupBy;

  function selectView(view: LinearView) {
    if (view === "mine") {
      setAssignedToMe(true);
      setGroupBy("state");
    } else {
      setAssignedToMe(false);
      setGroupBy(view);
    }
  }

  // Shift+←/→ cycles view chips (patterns.md §2)
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        !!target?.closest(".cm-editor") ||
        target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (!e.shiftKey || e.ctrlKey || e.altKey || e.metaKey) return;
      if (e.code !== "ArrowLeft" && e.code !== "ArrowRight") return;
      e.preventDefault();
      const idx = VIEW_ORDER.indexOf(activeView);
      const next =
        e.code === "ArrowRight"
          ? VIEW_ORDER[(idx + 1) % VIEW_ORDER.length]
          : VIEW_ORDER[(idx - 1 + VIEW_ORDER.length) % VIEW_ORDER.length];
      selectView(next);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeView]);

  // Mount-time deferred focus + initial cursor (mirrors ClickUpPanel)
  useEffect(() => {
    const timer = setTimeout(() => {
      const root = rootRef.current;
      if (!root) return;
      focusIfPanelZone(root);
      if (root.querySelector("[data-nav-selected='true']")) return;
      root.querySelector<HTMLElement>("[data-nav-item]")?.setAttribute("data-nav-selected", "true");
    }, 50);
    return () => clearTimeout(timer);
  }, []);

  const listenerActive = !zenOpen;

  // Alt+↑/↓ list navigation (patterns.md §1.4: within-list nav)
  useEffect(() => {
    if (!listenerActive) return;
    function onKey(e: KeyboardEvent) {
      if (!e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
      if (e.code !== "ArrowUp" && e.code !== "ArrowDown") return;
      const target = e.target as HTMLElement | null;
      const inField =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        !!target?.closest(".cm-editor") ||
        target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      const root = rootRef.current;
      if (!root) return;
      const items = Array.from(root.querySelectorAll<HTMLElement>("[data-nav-item]"));
      if (items.length === 0) return;
      const selected = root.querySelector<HTMLElement>("[data-nav-selected='true']");
      const idx = selected ? items.indexOf(selected) : -1;
      e.preventDefault();
      const next =
        e.code === "ArrowDown"
          ? idx === -1 ? 0 : (idx + 1) % items.length
          : idx === -1 ? items.length - 1 : (idx - 1 + items.length) % items.length;
      for (const item of items) item.removeAttribute("data-nav-selected");
      items[next].setAttribute("data-nav-selected", "true");
      items[next].scrollIntoView({ block: "nearest" });
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [listenerActive]);

  // Enter opens selected issue
  useEffect(() => {
    if (!listenerActive) return;
    function onKey(e: KeyboardEvent) {
      if (e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
      if (e.code !== "Enter") return;
      const target = e.target as HTMLElement | null;
      const inField =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        !!target?.closest(".cm-editor") ||
        target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      const root = rootRef.current;
      if (!root) return;
      const selected = root.querySelector<HTMLElement>(
        "[data-nav-selected='true'][data-issue-id]",
      );
      if (selected?.dataset.issueId) {
        e.preventDefault();
        setDetailIssueId(selected.dataset.issueId);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [listenerActive, setDetailIssueId]);

  const { groups, visibleCount } = useMemo(() => {
    const teamScoped = teamFilter
      ? issues.filter((i) => i.teamId === teamFilter)
      : issues;

    const viewerFiltered =
      assignedToMe && syncStatus?.viewerId
        ? teamScoped.filter((i) => i.assigneeId === syncStatus.viewerId)
        : teamScoped;

    const withCompleted = showCompleted
      ? viewerFiltered
      : viewerFiltered.filter((i) => !isTerminal(i.stateType));

    // Sub-issues (parentId set) show under parent rows — only top-level here
    const topLevel = withCompleted.filter((i) => !i.parentId);

    return {
      groups: groupIssues(topLevel, groupBy),
      visibleCount: topLevel.length,
    };
  }, [issues, teamFilter, assignedToMe, showCompleted, groupBy, syncStatus?.viewerId]);

  function toggleGroup(key: string) {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  const isLoading =
    syncStatus?.state === "syncing" && !syncStatus.baselineDone;

  if (syncStatus?.state === "needs_team") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6" data-focus-zone="linear">
        <p className="text-xs text-muted-foreground text-center">
          Select which Linear teams to sync in Settings → Linear.
        </p>
      </div>
    );
  }

  return (
    <TooltipProvider delay={0}>
      <div ref={rootRef} tabIndex={-1} className="flex h-full flex-col outline-none" data-focus-zone="linear">

        {/* Header: team selector + filter toggles */}
        <div className="flex shrink-0 items-center gap-1.5 border-b border-border/50 px-2 py-1.5">
          <select
            value={teamFilter ?? ""}
            onChange={(e) => setTeamFilter(e.target.value === "" ? null : e.target.value)}
            className="h-6 flex-1 min-w-0 rounded border border-input bg-secondary/40 px-1.5 text-[11px] text-foreground/80 outline-none focus:ring-1 focus:ring-ring/50"
          >
            <option value="">Todos</option>
            {teams.map((t) => (
              <option key={t.id} value={t.id}>{t.name}</option>
            ))}
          </select>

          <Tooltip>
            <TooltipTrigger
              render={
                <button
                  type="button"
                  data-header-action
                  onClick={() => {
                    if (activeView === "mine") return;
                    setAssignedToMe((prev) => !prev);
                  }}
                  disabled={activeView === "mine"}
                  aria-label="Assigned to me"
                  aria-pressed={assignedToMe}
                  className={`flex size-6 shrink-0 items-center justify-center rounded text-[10px] font-medium transition-colors disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent ${
                    assignedToMe
                      ? "bg-primary/10 text-primary"
                      : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
                  }`}
                />
              }
            >
              Me
            </TooltipTrigger>
            <TooltipContent side="bottom">
              {activeView === "mine" ? "Assigned to me — locked in My issues" : "Assigned to me"}
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger
              render={
                <button
                  type="button"
                  data-header-action
                  onClick={() => setShowCompleted((prev) => !prev)}
                  aria-label="Show completed"
                  aria-pressed={showCompleted}
                  className={`flex size-6 shrink-0 items-center justify-center rounded text-[10px] font-medium transition-colors ${
                    showCompleted
                      ? "bg-primary/10 text-primary"
                      : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
                  }`}
                />
              }
            >
              ✓
            </TooltipTrigger>
            <TooltipContent side="bottom">Show completed / canceled</TooltipContent>
          </Tooltip>
        </div>

        {/* View chip strip */}
        <div className="flex shrink-0 items-center gap-1 border-b border-border/50 px-2 py-1.5">
          {VIEW_ORDER.map((view) => (
            <Tooltip key={view}>
              <TooltipTrigger
                render={
                  <button
                    type="button"
                    onClick={() => selectView(view)}
                    aria-pressed={activeView === view}
                    className={`flex shrink-0 items-center rounded-full px-2.5 py-0.5 text-[11px] whitespace-nowrap transition-colors ${
                      activeView === view
                        ? "bg-secondary text-foreground"
                        : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground/80"
                    }`}
                  />
                }
              >
                {VIEW_LABEL[view]}
              </TooltipTrigger>
              <TooltipContent side="bottom" className="text-[10px]">{`${VIEW_LABEL[view]} (Shift+←/→)`}</TooltipContent>
            </Tooltip>
          ))}
        </div>

        {syncStatus?.state === "error" && syncStatus.error && (
          <div className="shrink-0 truncate bg-destructive/10 px-3 py-1 text-[10px] text-red-400" title={syncStatus.error}>
            Sync error: {syncStatus.error}
          </div>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto" data-scrollable>
          {isLoading ? (
            <div className="flex h-full flex-col items-center justify-center gap-2 px-6">
              <ProgressBar className="max-w-32" />
              <span className="text-[11px] text-muted-foreground">Syncing…</span>
            </div>
          ) : visibleCount === 0 ? (
            <div className="flex h-full items-center justify-center">
              <span className="text-xs text-muted-foreground">
                {syncStatus?.lastSync == null ? "Waiting for the first sync…" : "No issues"}
              </span>
            </div>
          ) : (
            groups.map((group) => (
              <LinearGroupSection
                key={group.key}
                group={group}
                expanded={!collapsed.has(group.key)}
                onToggle={() => toggleGroup(group.key)}
                issues={issues}
                onOpen={(id) => setDetailIssueId(id)}
              />
            ))
          )}
        </div>
      </div>
    </TooltipProvider>
  );
}

function LinearGroupSection({
  group,
  expanded,
  onToggle,
  issues,
  onOpen,
}: {
  group: IssueGroup;
  expanded: boolean;
  onToggle: () => void;
  issues: IssueView[];
  onOpen: (issueId: string) => void;
}) {
  const iconType = group.stateType ? linearStateToIconType(group.stateType) : null;

  return (
    <div>
      <button
        type="button"
        data-nav-item
        data-nav-expanded={expanded}
        data-group-key={group.key}
        onClick={onToggle}
        className="flex w-full items-center gap-1.5 bg-secondary/40 px-3 py-1.5 text-left text-[11px] font-medium text-foreground/70 transition-colors hover:bg-secondary/60"
      >
        {expanded
          ? <ChevronDown size={11} className="shrink-0 text-muted-foreground" />
          : <ChevronRight size={11} className="shrink-0 text-muted-foreground" />
        }
        {iconType ? (
          <StatusIcon type={iconType} color={group.color} size={13} className="shrink-0" title={group.label} />
        ) : group.color ? (
          <span className="size-1.5 shrink-0 rounded-full" style={{ background: group.color }} />
        ) : null}
        <span className="truncate">{group.label}</span>
        <span className="tabular-nums text-muted-foreground/50">{group.issues.length}</span>
      </button>
      {expanded && group.issues.map((issue) => (
        <LinearIssueRow
          key={issue.id}
          issue={issue}
          subIssues={issues.filter((i) => i.parentId === issue.id)}
          onOpen={onOpen}
        />
      ))}
    </div>
  );
}

function LinearIssueRow({
  issue,
  subIssues,
  onOpen,
  depth = 0,
}: {
  issue: IssueView;
  subIssues: IssueView[];
  onOpen: (issueId: string) => void;
  depth?: number;
}) {
  const [subExpanded, setSubExpanded] = useState(false);
  const hasChildren = subIssues.length > 0;
  const iconType = linearStateToIconType(issue.stateType);
  const priorityStr = linearPriorityStr(issue.priority);

  return (
    <div>
      <button
        type="button"
        data-nav-item
        data-issue-id={issue.id}
        onClick={() => onOpen(issue.id)}
        style={{ paddingLeft: 12 + depth * 16 }}
        className="group flex w-full items-center gap-1.5 py-1 pr-3 text-left transition-colors hover:bg-secondary/40"
      >
        {hasChildren ? (
          <span
            role="button"
            tabIndex={-1}
            aria-label={subExpanded ? "Collapse sub-issues" : "Expand sub-issues"}
            onClick={(e) => {
              e.stopPropagation();
              setSubExpanded((p) => !p);
            }}
            className="flex size-3.5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
          >
            {subExpanded ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          </span>
        ) : (
          <span className="w-3.5 shrink-0" aria-hidden />
        )}

        <PriorityIcon priority={priorityStr} size={12} className="shrink-0" />
        <StatusIcon
          type={iconType}
          color={issue.stateColor ?? null}
          size={13}
          className="shrink-0"
          title={issue.stateName}
        />

        {issue.identifier && (
          <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground/60">
            {issue.identifier}
          </span>
        )}

        <span className="min-w-0 flex-1 truncate text-[11px] text-foreground/80">
          {issue.title}
        </span>

        {/* Label chips */}
        {issue.labels.slice(0, 2).map((label) => (
          <span
            key={label.id}
            className="max-w-16 shrink-0 truncate rounded-full px-1.5 text-[9px] leading-4"
            style={{
              background: label.color ? `${label.color}33` : "var(--color-secondary)",
              color: label.color ?? "var(--color-secondary-foreground)",
            }}
          >
            {label.name}
          </span>
        ))}
      </button>
      {hasChildren && subExpanded && subIssues.map((sub) => (
        <LinearIssueRow
          key={sub.id}
          issue={sub}
          subIssues={[]}
          onOpen={onOpen}
          depth={depth + 1}
        />
      ))}
    </div>
  );
}
