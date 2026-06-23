import { useEffect, useMemo, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import {
  AlignLeft,
  ArrowDown,
  ArrowUp,
  CalendarClock,
  CalendarPlus,
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  CircleCheck,
  Clock,
  ExternalLink,
  Flag,
  GitBranchPlus,
  GitFork,
  Link2,
  ListChecks,
  PanelRight,
  Paperclip,
  Pin,
  PinOff,
  RefreshCw,
  RotateCcw,
  Send,
  Unlink,
  UserCheck,
} from "lucide-react";
import { open as openShell } from "@tauri-apps/plugin-shell";
import { invoke } from "@/lib/tauri";
import { focusIfPanelZone } from "@/lib/panelFocus";
import { configAtom } from "@/stores/config";
import { Select } from "@/components/ui/select";
import { StatusIcon } from "@/components/clickup/StatusIcon";
import { PriorityIcon } from "@/components/clickup/PriorityIcon";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { PulseDots } from "@/components/ui/PulseDots";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { zenModeAtom } from "@/stores/zenMode";
import {
  GROUP_BY_ORDER,
  activeSessionClickUpPinsAtom,
  activeSessionClickUpTaskAtom,
  clickupAssignedToMeAtom,
  clickupClosedOutAtom,
  clickupClosedTasksAtom,
  clickupClosureOfferAtom,
  clickupListStatusesAtom,
  reinjectTaskAction,
  clickupDetailTaskIdAtom,
  clickupGroupByAtom,
  clickupSendConfirmAtom,
  clickupShowClosedAtom,
  clickupSortAtom,
  clickupSpaceFilterAtom,
  clickupSpacesAtom,
  clickupSyncStatusAtom,
  clickupTasksAtom,
  copyTaskIdAction,
  openClickUpTaskTabAction,
  requestBindTaskAction,
  requestSendTaskAction,
  spawnWorktreeWithTaskAction,
  statusFraction,
  togglePinTaskAction,
  CLICKUP_ACTION_LABELS as ACTION_LABELS,
  type ClickUpGroupBy,
  type ClickUpListStatus,
  type ClickUpSortField,
  type ClickUpTask,
  type ClickUpTaskActions,
} from "@/stores/clickup";
import { activeSessionIdAtom } from "@/stores/workspace";

/// Chip-strip views: "mine" is a preset (assigned-to-me filter + grouped by
/// status); the rest are plain group-by modes over the current filter state.
type ClickUpView = "mine" | ClickUpGroupBy;
const VIEW_ORDER: ClickUpView[] = ["mine", ...GROUP_BY_ORDER];

/// The configured default view is applied once per app session (survives panel
/// remounts; a user view change afterwards is never overridden).
let clickupDefaultViewApplied = false;

const VIEW_LABEL: Record<ClickUpView, string> = {
  mine: "My tasks",
  status: "Status",
  list: "List",
  assignee: "Assignee",
};

const PRIORITY_RANK: Record<string, number> = { urgent: 0, high: 1, normal: 2, low: 3 };

/// Numeric sort key per field. Nulls sort last (oldest / no-due / no-priority).
function sortKey(t: ClickUpTask, field: ClickUpSortField): number {
  switch (field) {
    case "updated":
      return t.date_updated ?? 0;
    case "created":
      return t.date_created ?? 0;
    case "priority":
      return t.priority ? PRIORITY_RANK[t.priority] ?? 4 : 4;
    case "due":
      return t.due_date ?? Number.MAX_SAFE_INTEGER;
  }
}

/// Sort picker buttons. Priority sorts ascending by default (urgent first); the
/// date fields descending (newest/soonest-changed first).
const SORT_FIELDS: { field: ClickUpSortField; label: string; Icon: typeof Clock }[] = [
  { field: "updated", label: "Updated", Icon: Clock },
  { field: "created", label: "Created", Icon: CalendarPlus },
  { field: "priority", label: "Priority", Icon: Flag },
  { field: "due", label: "Due date", Icon: CalendarClock },
];

function defaultDirFor(field: ClickUpSortField): "asc" | "desc" {
  return field === "priority" ? "asc" : "desc";
}

interface TaskGroup {
  key: string;
  label: string;
  color: string | null;
  statusType: string | null;
  tasks: ClickUpTask[];
}

export function formatDueDate(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function groupTasks(tasks: ClickUpTask[], groupBy: ClickUpGroupBy): TaskGroup[] {
  const groups = new Map<string, TaskGroup>();
  const push = (
    key: string,
    label: string,
    color: string | null,
    statusType: string | null,
    task: ClickUpTask,
  ) => {
    let group = groups.get(key);
    if (!group) {
      group = { key, label, color, statusType, tasks: [] };
      groups.set(key, group);
    }
    group.tasks.push(task);
  };
  for (const task of tasks) {
    if (groupBy === "status") {
      const label = task.status_name ?? "No status";
      push(`status:${label}`, label, task.status_color, task.status_type, task);
    } else if (groupBy === "list") {
      push(`list:${task.list_id}`, task.list_name, null, null, task);
    } else if (task.assignees.length === 0) {
      push("assignee:none", "Unassigned", null, null, task);
    } else {
      // A multi-assignee task appears under each of its assignees.
      for (const a of task.assignees) {
        push(`assignee:${a.id ?? a.username ?? "?"}`, a.username ?? "Unknown", a.color, null, task);
      }
    }
  }
  return [...groups.values()];
}

export function ClickUpPanel() {
  const syncStatus = useAtomValue(clickupSyncStatusAtom);
  const tasks = useAtomValue(clickupTasksAtom);
  const spaces = useAtomValue(clickupSpacesAtom);
  const [spaceFilter, setSpaceFilter] = useAtom(clickupSpaceFilterAtom);
  const [groupBy, setGroupBy] = useAtom(clickupGroupByAtom);
  const [assignedToMe, setAssignedToMe] = useAtom(clickupAssignedToMeAtom);
  const [showClosed, setShowClosed] = useAtom(clickupShowClosedAtom);
  const [closedTasks, setClosedTasks] = useAtom(clickupClosedTasksAtom);
  const userId = syncStatus?.user_id ?? null;
  const [detailTaskId, setDetailTaskId] = useAtom(clickupDetailTaskIdAtom);
  const sendConfirm = useAtomValue(clickupSendConfirmAtom);
  const zenOpen = useAtomValue(zenModeAtom).open;
  const rootRef = useRef<HTMLDivElement>(null);
  const [closedLoading, setClosedLoading] = useState(false);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  // Per-task subtask expansion. Default COLLAPSED in the panel (absent = closed);
  // Space toggles the keyboard-cursor task, the chevron toggles on click. (The
  // detail modal shows subtasks expanded — it doesn't use this set.)
  const [expandedTaskIds, setExpandedTaskIds] = useState<ReadonlySet<string>>(new Set());
  const [sort, setSort] = useAtom(clickupSortAtom);
  const setListStatuses = useSetAtom(clickupListStatusesAtom);
  // Lists whose workflow has been requested this session (loaded or in-flight)
  // — keeps the background resolve to one network call per list.
  const resolvedListsRef = useRef<Set<string>>(new Set());

  const boundTaskId = useAtomValue(activeSessionClickUpTaskAtom);
  const pinnedTaskIds = useAtomValue(activeSessionClickUpPinsAtom);
  const requestSend = useSetAtom(requestSendTaskAction);
  const spawnWorktree = useSetAtom(spawnWorktreeWithTaskAction);
  const togglePin = useSetAtom(togglePinTaskAction);
  const requestBind = useSetAtom(requestBindTaskAction);
  const reinject = useSetAtom(reinjectTaskAction);
  const setClosureOffer = useSetAtom(clickupClosureOfferAtom);
  const copyTaskId = useSetAtom(copyTaskIdAction);
  const openTaskTab = useSetAtom(openClickUpTaskTabAction);
  const activeSessionId = useAtomValue(activeSessionIdAtom);

  const actions: ClickUpTaskActions = useMemo(
    () => ({
      send: (id) => requestSend(id),
      spawn: (id) => void spawnWorktree(id),
      togglePin: (id) => void togglePin(id),
      toggleBind: (id) => void requestBind(id),
      reinject: (id) => void reinject(id),
      closeOut: (id) => {
        if (activeSessionId) setClosureOffer({ taskId: id, sessionId: activeSessionId });
      },
      openInClickup: (id) => {
        const url = [...tasks, ...closedTasks].find((t) => t.id === id)?.url;
        if (url && /^https?:\/\//i.test(url)) void openShell(url);
      },
      openTab: (id) => openTaskTab(id),
      copyId: (displayId) => copyTaskId(displayId),
    }),
    [requestSend, spawnWorktree, togglePin, requestBind, reinject, setClosureOffer, copyTaskId, openTaskTab, activeSessionId, tasks, closedTasks],
  );

  // Derived chip state: the "mine" preset IS assigned-to-me + status, so a
  // manually-toggled UserCheck filter over status grouping lights it too.
  const activeView: ClickUpView = assignedToMe && groupBy === "status" ? "mine" : groupBy;

  function selectView(view: ClickUpView) {
    if (view === "mine") {
      setAssignedToMe(true);
      setGroupBy("status");
    } else {
      setAssignedToMe(false);
      setGroupBy(view);
    }
  }

  // Apply the configured default view once per session (before any user change).
  const defaultView = useAtomValue(configAtom).clickup_default_view;
  useEffect(() => {
    if (clickupDefaultViewApplied) return;
    clickupDefaultViewApplied = true;
    if (defaultView && defaultView !== "mine" && VIEW_ORDER.includes(defaultView as ClickUpView)) {
      selectView(defaultView as ClickUpView);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Shift+←/→ cycles the view chips — component-local handler per the
  // chip-strip contract (docs/patterns.md §2), with the editable-field guard.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (!e.shiftKey || e.ctrlKey || e.altKey || e.metaKey) return;
      if (e.code !== "ArrowLeft" && e.code !== "ArrowRight") return;
      e.preventDefault();
      const idx = VIEW_ORDER.indexOf(activeView);
      const next = e.code === "ArrowRight"
        ? VIEW_ORDER[(idx + 1) % VIEW_ORDER.length]
        : VIEW_ORDER[(idx - 1 + VIEW_ORDER.length) % VIEW_ORDER.length];
      if (next === "mine") {
        setAssignedToMe(true);
        setGroupBy("status");
      } else {
        setAssignedToMe(false);
        setGroupBy(next);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [activeView, setAssignedToMe, setGroupBy]);

  // Show-closed is the one on-demand fetch the panel performs (closed tasks
  // aren't in the default poll). Result is ephemeral — merged for display.
  useEffect(() => {
    if (!showClosed) {
      setClosedTasks([]);
      return;
    }
    let cancelled = false;
    setClosedLoading(true);
    invoke<ClickUpTask[]>("clickup_fetch_closed_tasks", {
      spaceId: spaceFilter ?? undefined,
      // Scope to the current user when filtering to mine — the unscoped fetch
      // pages the whole workspace's closed history and is very slow.
      assigneeId: assignedToMe ? userId ?? undefined : undefined,
    })
      .then((rows) => {
        if (!cancelled) setClosedTasks(rows);
      })
      .catch((err) => {
        console.warn("[clickup] closed fetch failed:", err);
        if (!cancelled) setClosedTasks([]);
      })
      .finally(() => {
        if (!cancelled) setClosedLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [showClosed, spaceFilter, assignedToMe, userId, setClosedTasks]);

  // Status glyphs need each list's ordered workflow for the proportional pie
  // fill. Bulk mirror read seeds whatever is already cached (instant, no
  // network); then any visible list still missing is resolved live once
  // (GET /list/{id}, which also caches into the mirror), so the proportions
  // snap in progressively. Tracked per session via resolvedListsRef.
  useEffect(() => {
    let cancelled = false;
    invoke<Record<string, ClickUpListStatus[]>>("clickup_read_all_list_statuses")
      .then((cached) => {
        if (cancelled) return;
        setListStatuses((prev) => ({ ...cached, ...prev }));
        for (const id of Object.keys(cached)) resolvedListsRef.current.add(id);
        const missing = [...new Set([...tasks, ...closedTasks].map((t) => t.list_id))].filter(
          (id) => !resolvedListsRef.current.has(id),
        );
        for (const id of missing) resolvedListsRef.current.add(id);
        void (async () => {
          for (const listId of missing) {
            try {
              const s = await invoke<ClickUpListStatus[]>("clickup_read_list_statuses", { listId });
              if (cancelled) return;
              setListStatuses((prev) => ({ ...prev, [listId]: s }));
            } catch {
              resolvedListsRef.current.delete(listId);
            }
          }
        })();
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [tasks, closedTasks, setListStatuses]);

  const { groups, closedIds, visibleCount, childMap } = useMemo(() => {
    const open = spaceFilter ? tasks.filter((t) => t.space_id === spaceFilter) : tasks;
    const openIds = new Set(open.map((t) => t.id));
    // Fetched-while-offline fallback rows are tombstoned mirror rows scoped
    // by the backend; live rows are space-scoped by the fetch query. Either
    // way only ids not already shown as open join the list.
    const closedExtra = showClosed
      ? closedTasks.filter((t) => !openIds.has(t.id))
      : [];
    const closedIds = new Set(closedExtra.map((t) => t.id));
    // The full pool (space + show-closed scoped, but NOT assignee-filtered) is
    // the nesting source: a visible parent shows ALL its subtasks, even ones
    // assigned to someone else, mirroring ClickUp/Linear's expand behavior.
    const pool = [...open, ...closedExtra];
    const compare = (a: ClickUpTask, b: ClickUpTask) => {
      const ka = sortKey(a, sort.field);
      const kb = sortKey(b, sort.field);
      const base = ka < kb ? -1 : ka > kb ? 1 : 0;
      return sort.dir === "asc" ? base : -base;
    };
    const childMap = new Map<string, ClickUpTask[]>();
    for (const t of pool) {
      if (t.parent_id) {
        const arr = childMap.get(t.parent_id);
        if (arr) arr.push(t);
        else childMap.set(t.parent_id, [t]);
      }
    }
    for (const arr of childMap.values()) arr.sort(compare);
    // The assignee filter only scopes which tasks become TOP-LEVEL rows; their
    // subtrees come from the pool via childMap.
    const visible =
      assignedToMe && userId !== null
        ? pool.filter((t) => t.assignees.some((a) => a.id === userId))
        : pool;
    const visibleIds = new Set(visible.map((t) => t.id));
    // A visible task is a top-level row unless its parent is also visible — in
    // which case it nests under that parent instead of grouping separately (so
    // it never renders twice).
    const topLevel = visible
      .filter((t) => !t.parent_id || !visibleIds.has(t.parent_id))
      .sort(compare);
    return {
      groups: groupTasks(topLevel, groupBy),
      closedIds,
      visibleCount: visible.length,
      childMap,
    };
  }, [tasks, closedTasks, spaceFilter, showClosed, assignedToMe, userId, groupBy, sort]);

  // Latest parent-id set for the expand/collapse-all shortcut (the window
  // keydown closure can't see the freshest childMap without re-subscribing).
  const allParentIdsRef = useRef<ReadonlyMap<string, ClickUpTask[]>>(new Map());
  allParentIdsRef.current = childMap;

  const allExpanded = expandedTaskIds.size > 0;
  function toggleExpandAll() {
    setExpandedTaskIds(allExpanded ? new Set() : new Set(childMap.keys()));
  }
  const isDefaultSort = sort.field === "updated" && sort.dir === "desc";

  function toggleGroup(key: string) {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  function toggleTaskExpand(id: string) {
    setExpandedTaskIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  // Mount-time focus + initial cursor. Intentional opens set the zone to
  // "panel" BEFORE this mounts, but togglePanel's rAF-deferred focusZone()
  // runs while the collapsed RightPanel still has no zone container, so DOM
  // focus stays in the terminal (which swallows arrows) — the "looks focused
  // but arrows are dead" state. Same race NavigablePickerContainer solves
  // with a deferred self-focus; gating on the zone keeps session-switch
  // restores from stealing the prompt (BUG-09).
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

  // Plain ↑/↓ moves a data-nav-selected cursor over the rows — window-level
  // like the git PRs picker (PrsChip): a handler on the panel div never fires
  // because focus normally sits on RightPanel's outer zone container, not
  // inside it. Cursor = selection attribute (patterns.md §5.2, styled by the
  // globals.css rule), never DOM focus — no focus ring on rows. Enter opens
  // the selected item; ←/→ collapse/expand a selected group header; ↑ from
  // the first item steps up into the Space select (focus-based, the Settings
  // convention for form controls). A/C toggle the header filters. The
  // terminal swallows its own keys at the canvas layer, so this only sees
  // strays.
  const listenerActive = !zenOpen && detailTaskId === null && sendConfirm === null;
  useEffect(() => {
    if (!listenerActive) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      // Ctrl+C over the keyboard-cursor task copies its id (custom id or
      // internal). Editable fields keep native copy; runs before the
      // modifier-bail below.
      if (e.code === "KeyC" && e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) {
        if (inField) return;
        if (!target?.closest("[data-focus-zone='panel']") && !target?.closest("[data-focus-zone='clickup']")) return;
        const sel = rootRef.current?.querySelector<HTMLElement>(
          "[data-nav-selected='true'][data-task-copy-id]",
        );
        if (sel?.dataset.taskCopyId) {
          e.preventDefault();
          copyTaskId(sel.dataset.taskCopyId);
        }
        return;
      }
      if (e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
      if (inField) return;
      // The sidebar, open dialogs (incl. swal confirms) and the select's
      // own popup own their keys.
      if (
        target?.closest("[data-focus-zone='sidebar']")
        || target?.closest("[role='dialog']")
        || target?.closest("[role='listbox']")
      ) return;
      const root = rootRef.current;
      if (!root) return;
      // Focus on a header-action button: its own arrow-nav (the capture handler
      // below) + native Enter/Space own the keys — don't run list nav here.
      if (target?.closest("[data-header-action]")) return;
      if (e.code === "KeyA" || e.code === "KeyH" || e.code === "KeyE") {
        // Bare-letter toggles: only while the user is interacting with the
        // panel (same zone scoping as the S/W/P/B task verbs). "C" is reserved
        // for close-out (the cursor row's verb); show-closed moved to "H";
        // "E" expands/collapses all subtask trees.
        if (!target?.closest("[data-focus-zone='panel']") && !target?.closest("[data-focus-zone='clickup']")) return;
        if (e.code === "KeyA") {
          // Disabled while the "My tasks" preset is active (it already implies
          // assigned-to-me) — leave it via the chips instead.
          if (assignedToMe && groupBy === "status") return;
          e.preventDefault();
          setAssignedToMe((prev) => !prev);
        } else if (e.code === "KeyH") {
          e.preventDefault();
          setShowClosed((prev) => !prev);
        } else {
          e.preventDefault();
          setExpandedTaskIds((prev) =>
            prev.size > 0 ? new Set() : new Set(allParentIdsRef.current.keys()),
          );
        }
        return;
      }
      const items = Array.from(root.querySelectorAll<HTMLElement>("[data-nav-item]"));
      if (items.length === 0) return;
      const selected = root.querySelector<HTMLElement>("[data-nav-selected='true']");
      const idx = selected ? items.indexOf(selected) : -1;
      if (e.code === "Space") {
        // Toggle the cursor task's subtasks (only when it has a subtree).
        const id = selected?.dataset.taskId;
        if (id && selected?.dataset.hasSubtree !== undefined) {
          e.preventDefault();
          setExpandedTaskIds((prev) => {
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
          // Top of the list → step up into the Space select.
          selected?.removeAttribute("data-nav-selected");
          root.querySelector<HTMLElement>("[role='combobox']")?.focus();
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
  }, [listenerActive, setAssignedToMe, setShowClosed, setExpandedTaskIds, assignedToMe, groupBy, copyTaskId]);

  // ↓ from the focused Space select hands control back to the list cursor.
  // Capture-phase on window so it runs BEFORE the select trigger's own
  // capture handler (which would otherwise do a body-wide focus walk —
  // its scope selector has no anchor inside this panel).
  useEffect(() => {
    if (!listenerActive) return;
    function onSelectKey(e: KeyboardEvent) {
      if (e.code !== "ArrowDown" && e.code !== "ArrowUp") return;
      if (e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
      const root = rootRef.current;
      const target = e.target as HTMLElement | null;
      if (!root || !target) return;
      if (target !== root.querySelector("[role='combobox']")) return;
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
  }, [listenerActive]);

  // Header-action row keyboard nav: → from the Space select enters the header
  // buttons (sort / reset / expand-all / filters); ←/→ move along them; ← from
  // the first returns to the select. Capture phase so it beats the Select's own
  // handler and the list-cursor nav. Disabled buttons (e.g. "assigned to me"
  // while locked in My tasks) are skipped.
  useEffect(() => {
    if (!listenerActive) return;
    function onHeaderNav(e: KeyboardEvent) {
      if (e.code !== "ArrowLeft" && e.code !== "ArrowRight") return;
      if (e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
      const root = rootRef.current;
      const target = e.target as HTMLElement | null;
      if (!root || !target) return;
      const combobox = root.querySelector<HTMLElement>("[role='combobox']");
      const onCombobox = target === combobox;
      const headerBtn = target.closest<HTMLElement>("[data-header-action]");
      if (!onCombobox && !headerBtn) return;
      const btns = Array.from(
        root.querySelectorAll<HTMLElement>("[data-header-action]:not([disabled])"),
      );
      if (btns.length === 0) return;
      if (onCombobox) {
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
      else if (i <= 0) combobox?.focus();
      else btns[i - 1]?.focus();
    }
    window.addEventListener("keydown", onHeaderNav, true);
    return () => window.removeEventListener("keydown", onHeaderNav, true);
  }, [listenerActive]);

  if (syncStatus?.state === "needs_team") {
    return (
      <div className="flex h-full flex-col" data-focus-zone="clickup">
        <TeamPicker teams={syncStatus.teams} />
      </div>
    );
  }

  return (
    <TooltipProvider delay={0}>
    <div ref={rootRef} tabIndex={-1} className="flex h-full flex-col outline-none" data-focus-zone="clickup">
      {/* Header: persistent Space selector + local filters */}
      <div className="flex shrink-0 items-center gap-1.5 border-b border-border/50 px-2 py-1.5">
        <Select
          value={spaceFilter ?? ""}
          onValueChange={(v) => setSpaceFilter(v === "" ? null : v)}
          options={[{ value: "", label: "Todos" }, ...spaces.map((s) => ({ value: s.id, label: s.name }))]}
          className="h-6 w-auto min-w-28 flex-1 px-2 py-0 text-[11px]"
        />
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                data-header-action
                onClick={() => void invoke("clickup_sync_now").catch(() => {})}
                disabled={syncStatus?.state === "syncing" || syncStatus?.state === "no_token"}
                aria-label="Sync from ClickUp"
                className="flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
              />
            }
          >
            <RefreshCw size={12} className={syncStatus?.state === "syncing" ? "animate-spin" : undefined} />
          </TooltipTrigger>
          <TooltipContent side="bottom">
            {syncStatus?.state === "syncing" ? "Syncing…" : "Sync from ClickUp"}
          </TooltipContent>
        </Tooltip>

        <span className="mx-0.5 h-4 w-px shrink-0 bg-border" aria-hidden />

        {/* Sort: one icon per field (click the active field to flip direction)
            + reset-to-default. Keyboard-reachable via the header-action row. */}
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
                  // The button disables itself once default — keep focus in the
                  // panel so the bare-letter shortcuts (E/A/H) keep firing.
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
                aria-label={allExpanded ? "Collapse all subtasks" : "Expand all subtasks"}
                className="flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
              />
            }
          >
            {allExpanded ? <ChevronsDownUp size={13} /> : <ChevronsUpDown size={13} />}
          </TooltipTrigger>
          <TooltipContent side="bottom">{allExpanded ? "Collapse all subtasks (E)" : "Expand all subtasks (E)"}</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                data-header-action
                onClick={() => setAssignedToMe(!assignedToMe)}
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
            {activeView === "mine" ? "Assigned to me — locked in My tasks" : "Assigned to me (A)"}
          </TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                data-header-action
                onClick={() => setShowClosed(!showClosed)}
                aria-label="Show closed tasks"
                aria-pressed={showClosed}
                className={`flex size-6 shrink-0 items-center justify-center rounded transition-colors ${
                  showClosed
                    ? "bg-primary/10 text-primary"
                    : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
                }`}
              />
            }
          >
            {closedLoading ? <PulseDots count={1} dotClassName="size-1.5" /> : <CircleCheck size={13} />}
          </TooltipTrigger>
          <TooltipContent side="bottom">Show closed tasks (H)</TooltipContent>
        </Tooltip>
      </div>

      {/* View chip strip: "My tasks" preset first, then plain group-by modes */}
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
        {/* "My tasks" can't be filtered until we know who "me" is — the mirror
            read returns before the user_id resolves, so without this gate the
            panel briefly flashes every task. Hold a progress bar instead. */}
        {assignedToMe && userId === null
          && syncStatus?.state !== "error" && syncStatus?.state !== "no_token" ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 px-6">
            <ProgressBar className="max-w-32" />
            <span className="text-[11px] text-muted-foreground">Loading your tasks…</span>
          </div>
        ) : visibleCount === 0 ? (
          <div className="flex h-full items-center justify-center">
            <span className="text-xs text-muted-foreground">
              {syncStatus?.last_sync == null ? "Waiting for the first sync…" : "No tasks"}
            </span>
          </div>
        ) : (
          groups.map((group) => (
            <GroupSection
              key={group.key}
              group={group}
              expanded={!collapsed.has(group.key)}
              onToggle={() => toggleGroup(group.key)}
              expandedTaskIds={expandedTaskIds}
              onToggleTask={toggleTaskExpand}
              childMap={childMap}
              closedIds={closedIds}
              showListName={groupBy !== "list"}
              onOpen={(id) => setDetailTaskId(id)}
              boundTaskId={boundTaskId}
              pinnedTaskIds={pinnedTaskIds}
              actions={actions}
            />
          ))
        )}
      </div>
    </div>
    </TooltipProvider>
  );
}

function TeamPicker({ teams }: { teams: { id: string; name: string }[] }) {
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function pick(teamId: string) {
    setBusyId(teamId);
    setError(null);
    try {
      await invoke("clickup_select_team", { teamId });
    } catch (err) {
      setError(String(err));
    } finally {
      setBusyId(null);
    }
  }

  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-6">
      <p className="text-xs text-muted-foreground">
        Multiple ClickUp workspaces found — pick which one to sync.
      </p>
      <div className="flex w-full max-w-60 flex-col gap-1.5">
        {teams.map((team) => (
          <button
            key={team.id}
            type="button"
            onClick={() => void pick(team.id)}
            disabled={busyId !== null}
            className="rounded-md border border-border bg-card px-3 py-1.5 text-left text-[11px] text-muted-foreground transition-colors hover:border-orange-500 hover:bg-orange-500/10 hover:text-foreground disabled:opacity-50"
          >
            {busyId === team.id ? "Selecting…" : team.name}
          </button>
        ))}
      </div>
      {error && <p className="text-[10px] text-red-400">{error}</p>}
    </div>
  );
}

function GroupSection({
  group,
  expanded,
  onToggle,
  expandedTaskIds,
  onToggleTask,
  childMap,
  closedIds,
  showListName,
  onOpen,
  boundTaskId,
  pinnedTaskIds,
  actions,
}: {
  group: TaskGroup;
  expanded: boolean;
  onToggle: () => void;
  expandedTaskIds: ReadonlySet<string>;
  onToggleTask: (taskId: string) => void;
  childMap: ReadonlyMap<string, ClickUpTask[]>;
  closedIds: ReadonlySet<string>;
  showListName: boolean;
  onOpen: (taskId: string) => void;
  boundTaskId: string | null;
  pinnedTaskIds: string[];
  actions: ClickUpTaskActions;
}) {
  const listStatuses = useAtomValue(clickupListStatusesAtom);
  const headerFraction = statusFraction(listStatuses[group.tasks[0]?.list_id], group.label);

  // Render a task and (when expanded) its full subtree, recursively. Children
  // come from the global childMap — a visible parent shows ALL its subtasks
  // regardless of the assignee/status filter. `seen` guards against a malformed
  // parent cycle.
  function renderNode(task: ClickUpTask, depth: number, seen: ReadonlySet<string>): React.ReactNode {
    const children = seen.has(task.id) ? [] : childMap.get(task.id) ?? [];
    const hasChildren = children.length > 0;
    const taskExpanded = expandedTaskIds.has(task.id);
    const nextSeen = hasChildren ? new Set([...seen, task.id]) : seen;
    return (
      <div key={task.id}>
        <TaskRow
          task={task}
          depth={depth}
          hasChildren={hasChildren}
          expanded={taskExpanded}
          onToggleExpand={() => onToggleTask(task.id)}
          closed={closedIds.has(task.id)}
          showListName={showListName}
          onOpen={onOpen}
          bound={task.id === boundTaskId}
          pinned={pinnedTaskIds.includes(task.id)}
          actions={actions}
        />
        {hasChildren && taskExpanded && children.map((c) => renderNode(c, depth + 1, nextSeen))}
      </div>
    );
  }

  // Linear-style group header: colored workflow glyph + normal-case label +
  // count (not the uppercase section-caps used elsewhere — the panel mirrors
  // Linear's grouped issue list here).
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
        {expanded ? <ChevronDown size={11} className="shrink-0 text-muted-foreground" /> : <ChevronRight size={11} className="shrink-0 text-muted-foreground" />}
        {group.statusType !== null ? (
          <StatusIcon type={group.statusType} color={group.color} fraction={headerFraction} size={13} className="shrink-0" title={group.label} />
        ) : group.color ? (
          <span className="size-1.5 shrink-0 rounded-full" style={{ background: group.color }} />
        ) : null}
        <span className="truncate">{group.label}</span>
        <span className="tabular-nums text-muted-foreground/50">{group.tasks.length}</span>
      </button>
      {expanded && group.tasks.map((task) => renderNode(task, 0, new Set<string>()))}
    </div>
  );
}

function RowAction({
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
  // span[role=button]: the row itself is a <button>, nesting real buttons is
  // invalid HTML (same pattern as the TopBar tab close affordance). Tooltip
  // component (delay-0 provider) instead of native title — instant hover,
  // same convention as the TopBar actions.
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <span
            role="button"
            tabIndex={-1}
            aria-label={label}
            onClick={(e) => {
              e.stopPropagation();
              onClick();
            }}
            className={`flex size-4 shrink-0 items-center justify-center rounded transition-colors ${
              active
                ? "text-primary hover:bg-secondary/60"
                : "text-muted-foreground/70 hover:bg-secondary/60 hover:text-foreground"
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

function TaskRow({
  task,
  closed,
  showListName,
  onOpen,
  bound,
  pinned,
  actions,
  depth = 0,
  hasChildren = false,
  expanded = true,
  onToggleExpand,
}: {
  task: ClickUpTask;
  closed: boolean;
  showListName: boolean;
  onOpen: (taskId: string) => void;
  bound: boolean;
  pinned: boolean;
  actions: ClickUpTaskActions;
  depth?: number;
  hasChildren?: boolean;
  expanded?: boolean;
  onToggleExpand?: () => void;
}) {
  const overdue = task.due_date !== null && task.due_date < Date.now() && !closed;
  const workedClosed = useAtomValue(clickupClosedOutAtom).includes(task.id);
  const listStatuses = useAtomValue(clickupListStatusesAtom);

  return (
    <button
      type="button"
      data-nav-item
      data-task-id={task.id}
      data-task-copy-id={task.custom_id ?? task.id}
      data-has-subtree={hasChildren ? "true" : undefined}
      onClick={() => onOpen(task.id)}
      style={{ paddingLeft: 12 + depth * 16 }}
      className={`group flex w-full items-center gap-1.5 py-1 pr-3 text-left transition-colors hover:bg-secondary/40 ${closed ? "opacity-60" : ""}`}
    >
      {/* Chevron slot: toggles subtasks when present (Space when keyboard-
          hovered), else a spacer so every row's priority/status stay aligned. */}
      {hasChildren ? (
        <span
          role="button"
          tabIndex={-1}
          aria-label={expanded ? "Collapse subtasks" : "Expand subtasks"}
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
      <PriorityIcon priority={task.priority} size={12} className="shrink-0" />
      <StatusIcon
        type={task.status_type}
        color={task.status_color}
        fraction={statusFraction(listStatuses[task.list_id], task.status_name)}
        size={13}
        className="shrink-0"
        title={task.status_name ?? undefined}
      />
      {(task.custom_id ?? task.id) && (
        <Tooltip>
          <TooltipTrigger
            render={
              <span
                role="button"
                tabIndex={-1}
                aria-label="Copy task ID"
                onClick={(e) => {
                  e.stopPropagation();
                  actions.copyId(task.custom_id ?? task.id);
                }}
                className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground/60 transition-colors hover:text-foreground"
              />
            }
          >
            {task.custom_id ?? task.id}
          </TooltipTrigger>
          <TooltipContent side="bottom" className="text-[10px]">Copy task ID</TooltipContent>
        </Tooltip>
      )}
      <span
        className={`min-w-0 flex-1 truncate text-[11px] ${
          closed ? "text-muted-foreground line-through" : "text-foreground/80"
        }`}
      >
        {task.name}
      </span>

      {/* Only the list name yields to the hover actions — priority, tags and
          the rest of the meta stay visible and shift next to the buttons. */}
      <span className="flex shrink-0 items-center gap-1.5">
        {workedClosed && (
          <CircleCheck
            size={10}
            className="shrink-0 text-emerald-500"
            aria-label="Worked and closed out from a session"
          />
        )}
        {bound && (
          <Link2 size={10} className="shrink-0 text-primary" aria-label="Active task of the current session" />
        )}
        {pinned && !bound && (
          <Pin size={10} className="shrink-0 text-primary/70" aria-label="Pinned as context for the current session" />
        )}
        {(task.has_description || task.subtask_count > 0 || task.checklist_count > 0 || task.attachment_count > 0) && (
          <span className="flex shrink-0 items-center gap-1 text-muted-foreground/50">
            {task.has_description && <AlignLeft size={10} aria-label="Has description" />}
            {task.subtask_count > 0 && (
              <span className="flex items-center gap-0.5">
                <GitFork size={10} />
                <span className="text-[9px] tabular-nums">{task.subtask_count}</span>
              </span>
            )}
            {task.checklist_count > 0 && <ListChecks size={10} aria-label="Has checklist" />}
            {task.attachment_count > 0 && <Paperclip size={10} aria-label="Has attachment" />}
          </span>
        )}
        {task.tags.slice(0, 2).map((tag) => (
          <span
            key={tag.name}
            className="max-w-16 shrink-0 truncate rounded-full px-1.5 text-[9px] leading-4"
            style={{
              background: tag.tag_bg ? `${tag.tag_bg}33` : "var(--color-secondary)",
              color: tag.tag_fg ?? tag.tag_bg ?? "var(--color-secondary-foreground)",
            }}
          >
            {tag.name}
          </span>
        ))}
        {task.due_date !== null && (
          <span className={`shrink-0 text-[10px] tabular-nums ${overdue ? "text-red-400" : "text-muted-foreground"}`}>
            {formatDueDate(task.due_date)}
          </span>
        )}
        {task.assignees.slice(0, 2).map((a) => (
          <span
            key={a.id ?? a.username ?? "?"}
            title={a.username ?? undefined}
            className="flex size-4 shrink-0 items-center justify-center rounded-full text-[8px] font-medium text-white"
            style={{ background: a.color ?? "var(--color-secondary)" }}
          >
            {a.initials ?? (a.username?.slice(0, 2).toUpperCase() ?? "?")}
          </span>
        ))}
      </span>

      {/* The keyboard cursor (data-nav-selected) mirrors the mouse hover:
          list name yields, action buttons appear — same affordance, and it
          doubles as the "which row am I on" signal. */}
      {showListName && (
        <span className="max-w-20 shrink-0 truncate text-[10px] text-muted-foreground/70 group-hover:hidden group-data-[nav-selected=true]:hidden">
          {task.list_name}
        </span>
      )}

      <span className="hidden shrink-0 items-center gap-0.5 group-hover:flex group-data-[nav-selected=true]:flex">
        <RowAction label={ACTION_LABELS.openAsTab} onClick={() => actions.openTab(task.id)}>
          <PanelRight size={10} />
        </RowAction>
        <RowAction label={ACTION_LABELS.send} onClick={() => actions.send(task.id)}>
          <Send size={10} />
        </RowAction>
        <RowAction label={ACTION_LABELS.spawn} onClick={() => actions.spawn(task.id)}>
          <GitBranchPlus size={10} />
        </RowAction>
        <RowAction
          label={pinned ? ACTION_LABELS.unpin : ACTION_LABELS.pin}
          onClick={() => actions.togglePin(task.id)}
          active={pinned}
        >
          {pinned ? <PinOff size={10} /> : <Pin size={10} />}
        </RowAction>
        <RowAction
          label={bound ? ACTION_LABELS.unbind : ACTION_LABELS.bind}
          onClick={() => actions.toggleBind(task.id)}
          active={bound}
        >
          {bound ? <Unlink size={10} /> : <Link2 size={10} />}
        </RowAction>
        {(bound || pinned) && (
          <RowAction label={ACTION_LABELS.reinject} onClick={() => actions.reinject(task.id)}>
            <RefreshCw size={10} />
          </RowAction>
        )}
        {bound && (
          <RowAction label={ACTION_LABELS.closeOut} onClick={() => actions.closeOut(task.id)}>
            <CircleCheck size={10} />
          </RowAction>
        )}
        {task.url && (
          <RowAction label={ACTION_LABELS.openInClickup} onClick={() => actions.openInClickup(task.id)}>
            <ExternalLink size={10} />
          </RowAction>
        )}
      </span>
    </button>
  );
}
