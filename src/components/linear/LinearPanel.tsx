import { useEffect, useMemo, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import {
  AlignLeft,
  ArrowDown,
  ArrowUp,
  CalendarClock,
  CalendarPlus,
  ChevronsDownUp,
  ChevronsUpDown,
  ChevronDown,
  ChevronRight,
  Clock,
  Flag,
  RotateCcw,
  UserCheck,
} from "lucide-react";
import { focusIfPanelZone } from "@/lib/panelFocus";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { Select } from "@/components/ui/select";
import { StatusIcon } from "@/components/clickup/StatusIcon";
import { PriorityIcon } from "@/components/clickup/PriorityIcon";
import { zenModeAtom } from "@/stores/zenMode";
import {
  GROUP_BY_ORDER,
  copyLinearIssueAction,
  linearAssignedToMeAtom,
  linearDetailIssueIdAtom,
  linearGroupByAtom,
  linearIssuesAtom,
  linearShowCompletedAtom,
  linearSortAtom,
  linearSyncStatusAtom,
  linearTeamFilterAtom,
  linearTeamsAtom,
  type IssueView,
  type LinearGroupBy,
  type LinearSortField,
} from "@/stores/linear";

// ── Priority mapping: Linear int → PriorityIcon string ──
// 0=none, 1=urgent, 2=high, 3=medium, 4=low
export function linearPriorityStr(p: number): string | null {
  switch (p) {
    case 1: return "urgent";
    case 2: return "high";
    case 3: return "normal";
    case 4: return "low";
    default: return null;
  }
}

// ── State-type mapping: Linear stateType → StatusIcon type ──
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

// ── Sort ──

/// Numeric sort key per field. Nulls (0/undefined) sort last.
/// Linear priority: 1=urgent(highest)…4=low, 0=none → treat 0 as 5 (last).
function sortKey(i: IssueView, field: LinearSortField): number {
  switch (field) {
    case "updated":
      return i.updatedAt ?? 0;
    case "created":
      return i.createdAt ?? 0;
    case "priority":
      // 0=none must sort last when ascending (urgent first); map 0 → 5.
      return i.priority === 0 ? 5 : i.priority;
    case "due":
      // No due date sorts last when ascending (soonest first).
      return i.dueDate ?? Number.MAX_SAFE_INTEGER;
  }
}

/// Sort picker buttons. Priority ascending = urgent first (asc on 1…4, none
/// last); due ascending = soonest first (no-due last).
const SORT_FIELDS: { field: LinearSortField; label: string; Icon: typeof Clock }[] = [
  { field: "updated", label: "Updated", Icon: Clock },
  { field: "created", label: "Created", Icon: CalendarPlus },
  { field: "priority", label: "Priority", Icon: Flag },
  { field: "due", label: "Due date", Icon: CalendarClock },
];

function defaultDirFor(field: LinearSortField): "asc" | "desc" {
  // Priority ascending = urgent first; due ascending = soonest first; other
  // dates descending = newest first.
  return field === "priority" || field === "due" ? "asc" : "desc";
}

function compareIssues(a: IssueView, b: IssueView, field: LinearSortField, dir: "asc" | "desc"): number {
  const ka = sortKey(a, field);
  const kb = sortKey(b, field);
  const base = ka < kb ? -1 : ka > kb ? 1 : 0;
  return dir === "asc" ? base : -base;
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
  const [sort, setSort] = useAtom(linearSortAtom);
  const setDetailIssueId = useSetAtom(linearDetailIssueIdAtom);
  const copyIssue = useSetAtom(copyLinearIssueAction);
  const zenOpen = useAtomValue(zenModeAtom).open;
  const rootRef = useRef<HTMLDivElement>(null);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  // Per-issue sub-issue expansion. Default collapsed (absent = closed).
  const [expandedIssueIds, setExpandedIssueIds] = useState<ReadonlySet<string>>(new Set());

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
  // selectView is stable (captures atoms, not state), but activeView changes — dep on activeView only.
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

  // detailOpen gates the key listeners so the panel's bare letters don't fire
  // while the floating detail is handling its own navigation.
  const detailOpen = useAtomValue(linearDetailIssueIdAtom) !== null;
  const keyListenerActive = !zenOpen && !detailOpen;

  // Latest parent-id set for the expand/collapse-all shortcut.
  const allParentIdsRef = useRef<ReadonlySet<string>>(new Set());

  const { groups, visibleCount, childMap } = useMemo(() => {
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

    // Build global parent→children map over the full (unfiltered) pool so a
    // visible parent shows ALL its sub-issues regardless of filter (§9 canon).
    const fullPool = showCompleted ? teamScoped : teamScoped.filter((i) => !isTerminal(i.stateType));
    const childMap = new Map<string, IssueView[]>();
    for (const i of fullPool) {
      if (i.parentId) {
        const arr = childMap.get(i.parentId);
        if (arr) arr.push(i);
        else childMap.set(i.parentId, [i]);
      }
    }
    const compare = (a: IssueView, b: IssueView) => compareIssues(a, b, sort.field, sort.dir);
    for (const arr of childMap.values()) arr.sort(compare);

    // Top-level rows: visible issues whose parent is not also visible.
    const visibleIds = new Set(withCompleted.map((i) => i.id));
    const topLevel = withCompleted
      .filter((i) => !i.parentId || !visibleIds.has(i.parentId))
      .sort(compare);

    return {
      groups: groupIssues(topLevel, groupBy),
      visibleCount: topLevel.length,
      childMap,
    };
  }, [issues, teamFilter, assignedToMe, showCompleted, groupBy, syncStatus?.viewerId, sort]);

  allParentIdsRef.current = new Set(childMap.keys());

  const allExpanded = expandedIssueIds.size > 0;
  const isDefaultSort = sort.field === "updated" && sort.dir === "desc";

  function toggleExpandAll() {
    setExpandedIssueIds(allExpanded ? new Set() : new Set(childMap.keys()));
    rootRef.current?.focus({ preventScroll: true });
  }

  function toggleGroup(key: string) {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  function toggleIssueExpand(id: string) {
    setExpandedIssueIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  // Plain ↑/↓ moves the data-nav-selected cursor. Enter opens. Space toggles
  // sub-issues. E expands/collapses all. A toggles assigned-to-me. H toggles
  // show-completed. Ctrl+C copies the identifier of the cursor row.
  // header-action buttons own their own ←/→ (capture handler below).
  useEffect(() => {
    if (!keyListenerActive) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        !!target?.closest(".cm-editor") ||
        target?.getAttribute("contenteditable") === "true";

      // Ctrl+C over the keyboard-cursor issue copies its identifier.
      if (e.code === "KeyC" && e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) {
        if (inField) return;
        if (!target?.closest("[data-focus-zone='linear']")) return;
        const sel = rootRef.current?.querySelector<HTMLElement>(
          "[data-nav-selected='true'][data-issue-copy-id]",
        );
        if (sel?.dataset.issueCopyId) {
          e.preventDefault();
          copyIssue(sel.dataset.issueCopyId);
        }
        return;
      }

      if (e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
      if (inField) return;
      if (
        target?.closest("[data-focus-zone='sidebar']") ||
        target?.closest("[role='dialog']") ||
        target?.closest("[role='listbox']")
      ) return;
      const root = rootRef.current;
      if (!root) return;
      if (target?.closest("[data-header-action]")) return;

      if (e.code === "KeyA" || e.code === "KeyH" || e.code === "KeyE") {
        if (!target?.closest("[data-focus-zone='linear']")) return;
        if (e.code === "KeyA") {
          if (assignedToMe && groupBy === "state") return;
          e.preventDefault();
          setAssignedToMe((prev) => !prev);
        } else if (e.code === "KeyH") {
          e.preventDefault();
          setShowCompleted((prev) => !prev);
        } else {
          e.preventDefault();
          setExpandedIssueIds((prev) =>
            prev.size > 0 ? new Set() : new Set(allParentIdsRef.current),
          );
          root.focus({ preventScroll: true });
        }
        return;
      }

      const items = Array.from(root.querySelectorAll<HTMLElement>("[data-nav-item]"));
      if (items.length === 0) return;
      const selected = root.querySelector<HTMLElement>("[data-nav-selected='true']");
      const idx = selected ? items.indexOf(selected) : -1;

      if (e.code === "Space") {
        const id = selected?.dataset.issueId;
        if (id && selected?.dataset.hasSubtree !== undefined) {
          e.preventDefault();
          setExpandedIssueIds((prev) => {
            const nextSet = new Set(prev);
            if (nextSet.has(id)) nextSet.delete(id);
            else nextSet.add(id);
            return nextSet;
          });
        }
        return;
      }

      if (e.code === "ArrowUp" || e.code === "ArrowDown") {
        e.preventDefault();
        if (e.code === "ArrowUp" && idx === 0) {
          selected?.removeAttribute("data-nav-selected");
          root.querySelector<HTMLElement>("[role='combobox'], select")?.focus();
          return;
        }
        const next = e.code === "ArrowDown"
          ? (idx === -1 ? 0 : (idx + 1) % items.length)
          : (idx === -1 ? items.length - 1 : (idx - 1 + items.length) % items.length);
        for (const item of items) item.removeAttribute("data-nav-selected");
        items[next].setAttribute("data-nav-selected", "true");
        items[next].scrollIntoView({ block: "nearest" });
        return;
      }

      if (e.code === "Enter") {
        if (idx === -1) return;
        e.preventDefault();
        items[idx].click();
        return;
      }

      if (e.code !== "ArrowLeft" && e.code !== "ArrowRight") return;
      if (!selected || selected.dataset.navExpanded === undefined) return;
      const key = selected.dataset.groupKey;
      if (!key) return;
      e.preventDefault();
      const expanded = selected.dataset.navExpanded === "true";
      if (e.code === "ArrowLeft" && expanded) toggleGroup(key);
      if (e.code === "ArrowRight" && !expanded) toggleGroup(key);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [keyListenerActive, assignedToMe, groupBy, setAssignedToMe, setShowCompleted, copyIssue]);

  // ↓ from the focused team select hands control back to the list cursor
  // (mirrors ClickUpPanel's Select handler).
  useEffect(() => {
    if (!keyListenerActive) return;
    function onSelectKey(e: KeyboardEvent) {
      if (e.code !== "ArrowDown" && e.code !== "ArrowUp") return;
      if (e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
      const root = rootRef.current;
      const target = e.target as HTMLElement | null;
      if (!root || !target) return;
      const sel = root.querySelector<HTMLElement>("[role='combobox'], select");
      if (!sel || target !== sel) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.code !== "ArrowDown") return;
      target.blur();
      focusIfPanelZone(root);
      const first = root.querySelector<HTMLElement>("[data-nav-item]");
      if (!first) return;
      for (const item of root.querySelectorAll("[data-nav-selected]")) {
        item.removeAttribute("data-nav-selected");
      }
      first.setAttribute("data-nav-selected", "true");
      first.scrollIntoView({ block: "nearest" });
    }
    window.addEventListener("keydown", onSelectKey, true);
    return () => window.removeEventListener("keydown", onSelectKey, true);
  }, [keyListenerActive]);

  // Header-action row keyboard nav: → from the team select enters the header
  // buttons; ←/→ move along them; ← from the first returns to the select.
  // Mirrors ClickUpPanel's header-nav capture handler (§10).
  useEffect(() => {
    if (!keyListenerActive) return;
    function onHeaderNav(e: KeyboardEvent) {
      if (e.code !== "ArrowLeft" && e.code !== "ArrowRight") return;
      if (e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
      const root = rootRef.current;
      const target = e.target as HTMLElement | null;
      if (!root || !target) return;
      const sel = root.querySelector<HTMLElement>("[role='combobox'], select");
      const onSelect = target === sel;
      const headerBtn = target.closest<HTMLElement>("[data-header-action]");
      if (!onSelect && !headerBtn) return;
      const btns = Array.from(
        root.querySelectorAll<HTMLElement>("[data-header-action]:not([disabled])"),
      );
      if (btns.length === 0) return;
      if (onSelect) {
        if (e.code !== "ArrowRight") return;
        e.preventDefault();
        e.stopPropagation();
        btns[0].focus();
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      const i = btns.indexOf(headerBtn!);
      if (e.code === "ArrowRight") btns[Math.min(i + 1, btns.length - 1)]?.focus();
      else if (i <= 0) sel?.focus();
      else btns[i - 1]?.focus();
    }
    window.addEventListener("keydown", onHeaderNav, true);
    return () => window.removeEventListener("keydown", onHeaderNav, true);
  }, [keyListenerActive]);

  const isLoading = syncStatus?.state === "syncing" && !syncStatus.baselineDone;

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

        {/* Header: team selector + sort pickers + filter toggles */}
        <div className="flex shrink-0 items-center gap-1.5 border-b border-border/50 px-2 py-1.5">
          <Select
            value={teamFilter ?? ""}
            onValueChange={(v) => setTeamFilter(v === "" ? null : v)}
            options={[{ value: "", label: "Todos" }, ...teams.map((t) => ({ value: t.id, label: t.name }))]}
            className="h-6 w-auto min-w-28 flex-1 px-2 py-0 text-[11px]"
          />

          {/* Sort: one icon per field (click active field to flip dir) + reset-to-default */}
          {SORT_FIELDS.map(({ field, label, Icon }) => {
            const active = sort.field === field;
            return (
              <Tooltip key={field}>
                <TooltipTrigger
                  render={
                    <button
                      type="button"
                      data-header-action
                      aria-label={`Sort by ${label}`}
                      aria-pressed={active}
                      onClick={() =>
                        setSort(
                          active
                            ? { field, dir: sort.dir === "asc" ? "desc" : "asc" }
                            : { field, dir: defaultDirFor(field) },
                        )
                      }
                      className={`flex size-6 shrink-0 items-center justify-center rounded transition-colors ${
                        active
                          ? "bg-primary/10 text-primary"
                          : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
                      }`}
                    />
                  }
                >
                  {active ? (
                    <span className="flex items-center gap-px">
                      <Icon size={12} />
                      {sort.dir === "asc" ? <ArrowUp size={9} /> : <ArrowDown size={9} />}
                    </span>
                  ) : (
                    <Icon size={13} />
                  )}
                </TooltipTrigger>
                <TooltipContent side="bottom">
                  {`Sort by ${label.toLowerCase()}${active ? (sort.dir === "asc" ? " · asc" : " · desc") : ""}`}
                </TooltipContent>
              </Tooltip>
            );
          })}

          <Tooltip>
            <TooltipTrigger
              render={
                <button
                  type="button"
                  data-header-action
                  onClick={() => {
                    setSort({ field: "updated", dir: "desc" });
                    rootRef.current?.focus({ preventScroll: true });
                  }}
                  disabled={isDefaultSort}
                  aria-label="Reset sort to default"
                  className="flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground disabled:opacity-30 disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
                />
              }
            >
              <RotateCcw size={12} />
            </TooltipTrigger>
            <TooltipContent side="bottom">Reset sort (Updated · desc)</TooltipContent>
          </Tooltip>

          <span className="mx-0.5 h-4 w-px shrink-0 bg-border" aria-hidden />

          <Tooltip>
            <TooltipTrigger
              render={
                <button
                  type="button"
                  data-header-action
                  onClick={toggleExpandAll}
                  aria-label={allExpanded ? "Collapse all sub-issues" : "Expand all sub-issues"}
                  className="flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
                />
              }
            >
              {allExpanded ? <ChevronsDownUp size={13} /> : <ChevronsUpDown size={13} />}
            </TooltipTrigger>
            <TooltipContent side="bottom">
              {allExpanded ? "Collapse all sub-issues (E)" : "Expand all sub-issues (E)"}
            </TooltipContent>
          </Tooltip>

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
                  className={`flex size-6 shrink-0 items-center justify-center rounded transition-colors disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent ${
                    assignedToMe
                      ? "bg-primary/10 text-primary"
                      : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
                  }`}
                />
              }
            >
              <UserCheck size={13} />
            </TooltipTrigger>
            <TooltipContent side="bottom">
              {activeView === "mine" ? "Assigned to me — locked in My issues" : "Assigned to me (A)"}
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger
              render={
                <button
                  type="button"
                  data-header-action
                  onClick={() => setShowCompleted((prev) => !prev)}
                  aria-label="Show completed / canceled"
                  aria-pressed={showCompleted}
                  className={`flex size-6 shrink-0 items-center justify-center rounded text-[10px] font-medium transition-colors ${
                    showCompleted
                      ? "bg-primary/10 text-primary"
                      : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
                  }`}
                />
              }
            >
              <AlignLeft size={13} />
            </TooltipTrigger>
            <TooltipContent side="bottom">Show completed / canceled (H)</TooltipContent>
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
                expandedIssueIds={expandedIssueIds}
                onToggleIssue={toggleIssueExpand}
                childMap={childMap}
                onOpen={(id) => setDetailIssueId(id)}
                copyIssue={copyIssue}
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
  expandedIssueIds,
  onToggleIssue,
  childMap,
  onOpen,
  copyIssue,
}: {
  group: IssueGroup;
  expanded: boolean;
  onToggle: () => void;
  expandedIssueIds: ReadonlySet<string>;
  onToggleIssue: (issueId: string) => void;
  childMap: ReadonlyMap<string, IssueView[]>;
  onOpen: (issueId: string) => void;
  copyIssue: (identifier: string) => void;
}) {
  const iconType = group.stateType ? linearStateToIconType(group.stateType) : null;

  // Render an issue and (when expanded) its full sub-issue tree, recursively.
  // `seen` guards malformed parent cycles (§9 canon).
  function renderNode(issue: IssueView, depth: number, seen: ReadonlySet<string>): React.ReactNode {
    const children = seen.has(issue.id) ? [] : (childMap.get(issue.id) ?? []);
    const hasChildren = children.length > 0;
    const issueExpanded = expandedIssueIds.has(issue.id);
    const nextSeen = hasChildren ? new Set([...seen, issue.id]) : seen;
    return (
      <div key={issue.id}>
        <LinearIssueRow
          issue={issue}
          depth={depth}
          hasChildren={hasChildren}
          expanded={issueExpanded}
          onToggleExpand={() => onToggleIssue(issue.id)}
          onOpen={onOpen}
          copyIssue={copyIssue}
        />
        {hasChildren && issueExpanded && children.map((c) => renderNode(c, depth + 1, nextSeen))}
      </div>
    );
  }

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
      {expanded && group.issues.map((issue) => renderNode(issue, 0, new Set<string>()))}
    </div>
  );
}

function LinearIssueRow({
  issue,
  depth = 0,
  hasChildren = false,
  expanded = false,
  onToggleExpand,
  onOpen,
  copyIssue,
}: {
  issue: IssueView;
  depth?: number;
  hasChildren?: boolean;
  expanded?: boolean;
  onToggleExpand?: () => void;
  onOpen: (issueId: string) => void;
  copyIssue: (identifier: string) => void;
}) {
  const iconType = linearStateToIconType(issue.stateType);
  const priorityStr = linearPriorityStr(issue.priority);

  return (
    <button
      type="button"
      data-nav-item
      data-issue-id={issue.id}
      data-issue-copy-id={issue.identifier ?? issue.id}
      data-has-subtree={hasChildren ? "true" : undefined}
      onClick={() => onOpen(issue.id)}
      style={{ paddingLeft: 12 + depth * 16 }}
      className="group flex w-full items-center gap-1.5 py-1 pr-3 text-left transition-colors hover:bg-secondary/40"
    >
      {/* Chevron slot: same-width spacer keeps columns aligned (§9) */}
      {hasChildren ? (
        <span
          role="button"
          tabIndex={-1}
          aria-label={expanded ? "Collapse sub-issues" : "Expand sub-issues"}
          onClick={(e) => {
            e.stopPropagation();
            onToggleExpand?.();
          }}
          className="flex size-3.5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
        >
          {expanded ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
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

      {/* Copyable identifier — click copies (§12); Ctrl+C on the cursor row also copies */}
      {(issue.identifier ?? issue.id) && (
        <Tooltip>
          <TooltipTrigger
            render={
              <span
                role="button"
                tabIndex={-1}
                aria-label="Copy issue ID"
                onClick={(e) => {
                  e.stopPropagation();
                  copyIssue(issue.identifier ?? issue.id);
                }}
                className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground/60 transition-colors hover:text-foreground"
              />
            }
          >
            {issue.identifier ?? issue.id}
          </TooltipTrigger>
          <TooltipContent side="bottom" className="text-[10px]">Copy issue ID</TooltipContent>
        </Tooltip>
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
  );
}
