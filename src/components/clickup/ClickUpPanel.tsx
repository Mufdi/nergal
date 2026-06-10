import { useEffect, useMemo, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import {
  ChevronDown,
  ChevronRight,
  CircleCheck,
  Flag,
  Loader2,
  UserCheck,
} from "lucide-react";
import { invoke } from "@/lib/tauri";
import { Select } from "@/components/ui/select";
import {
  GROUP_BY_ORDER,
  clickupAssignedToMeAtom,
  clickupClosedTasksAtom,
  clickupDetailTaskIdAtom,
  clickupGroupByAtom,
  clickupShowClosedAtom,
  clickupSpaceFilterAtom,
  clickupSpacesAtom,
  clickupSyncStatusAtom,
  clickupTasksAtom,
  type ClickUpGroupBy,
  type ClickUpTask,
} from "@/stores/clickup";
import { ClickUpTaskDetail } from "./ClickUpTaskDetail";

const GROUP_BY_LABEL: Record<ClickUpGroupBy, string> = {
  status: "Status",
  list: "List",
  assignee: "Assignee",
};

const PRIORITY_COLOR: Record<string, string> = {
  urgent: "#f50000",
  high: "#ffcc00",
  normal: "#6fddff",
  low: "#d8d8d8",
};

interface TaskGroup {
  key: string;
  label: string;
  color: string | null;
  tasks: ClickUpTask[];
}

export function formatDueDate(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function groupTasks(tasks: ClickUpTask[], groupBy: ClickUpGroupBy): TaskGroup[] {
  const groups = new Map<string, TaskGroup>();
  const push = (key: string, label: string, color: string | null, task: ClickUpTask) => {
    let group = groups.get(key);
    if (!group) {
      group = { key, label, color, tasks: [] };
      groups.set(key, group);
    }
    group.tasks.push(task);
  };
  for (const task of tasks) {
    if (groupBy === "status") {
      const label = task.status_name ?? "No status";
      push(`status:${label}`, label, task.status_color, task);
    } else if (groupBy === "list") {
      push(`list:${task.list_id}`, task.list_name, null, task);
    } else if (task.assignees.length === 0) {
      push("assignee:none", "Unassigned", null, task);
    } else {
      // A multi-assignee task appears under each of its assignees.
      for (const a of task.assignees) {
        push(`assignee:${a.id ?? a.username ?? "?"}`, a.username ?? "Unknown", a.color, task);
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
  const setDetailTaskId = useSetAtom(clickupDetailTaskIdAtom);
  const [closedLoading, setClosedLoading] = useState(false);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());

  // Shift+←/→ cycles group-by — component-local handler per the chip-strip
  // contract (docs/patterns.md §2), with the same editable-field guard.
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
      const idx = GROUP_BY_ORDER.indexOf(groupBy);
      const next = e.code === "ArrowRight"
        ? GROUP_BY_ORDER[(idx + 1) % GROUP_BY_ORDER.length]
        : GROUP_BY_ORDER[(idx - 1 + GROUP_BY_ORDER.length) % GROUP_BY_ORDER.length];
      setGroupBy(next);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [groupBy, setGroupBy]);

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
  }, [showClosed, spaceFilter, setClosedTasks]);

  const userId = syncStatus?.user_id ?? null;

  const { groups, closedIds, visibleCount } = useMemo(() => {
    const open = spaceFilter ? tasks.filter((t) => t.space_id === spaceFilter) : tasks;
    const openIds = new Set(open.map((t) => t.id));
    // Fetched-while-offline fallback rows are tombstoned mirror rows scoped
    // by the backend; live rows are space-scoped by the fetch query. Either
    // way only ids not already shown as open join the list.
    const closedExtra = showClosed
      ? closedTasks.filter((t) => !openIds.has(t.id))
      : [];
    const closedIds = new Set(closedExtra.map((t) => t.id));
    let all = [...open, ...closedExtra];
    if (assignedToMe && userId !== null) {
      all = all.filter((t) => t.assignees.some((a) => a.id === userId));
    }
    return { groups: groupTasks(all, groupBy), closedIds, visibleCount: all.length };
  }, [tasks, closedTasks, spaceFilter, showClosed, assignedToMe, userId, groupBy]);

  function toggleGroup(key: string) {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  // Arrow-left/right on a focused group header collapses/expands it
  // (data-nav-expanded contract, docs/patterns.md §5.2).
  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    if (e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
    const header = (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-nav-expanded]");
    const key = header?.dataset.groupKey;
    if (!header || !key) return;
    e.preventDefault();
    const expanded = header.dataset.navExpanded === "true";
    if (e.key === "ArrowLeft" && expanded) toggleGroup(key);
    if (e.key === "ArrowRight" && !expanded) toggleGroup(key);
  }

  if (syncStatus?.state === "needs_team") {
    return (
      <div className="flex h-full flex-col" data-focus-zone="clickup">
        <TeamPicker teams={syncStatus.teams} />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col" data-focus-zone="clickup" onKeyDown={handleKeyDown}>
      {/* Header: persistent Space selector + local filters */}
      <div className="flex shrink-0 items-center gap-1.5 border-b border-border/50 px-2 py-1.5">
        <Select
          value={spaceFilter ?? ""}
          onValueChange={(v) => setSpaceFilter(v === "" ? null : v)}
          options={[{ value: "", label: "Todos" }, ...spaces.map((s) => ({ value: s.id, label: s.name }))]}
          className="h-6 w-auto min-w-28 flex-1 px-2 py-0 text-[11px]"
        />
        <button
          type="button"
          onClick={() => setAssignedToMe(!assignedToMe)}
          aria-label="Assigned to me"
          aria-pressed={assignedToMe}
          title="Assigned to me"
          className={`flex size-6 shrink-0 items-center justify-center rounded transition-colors ${
            assignedToMe
              ? "bg-primary/10 text-primary"
              : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
          }`}
        >
          <UserCheck size={13} />
        </button>
        <button
          type="button"
          onClick={() => setShowClosed(!showClosed)}
          aria-label="Show closed tasks"
          aria-pressed={showClosed}
          title="Show closed tasks"
          className={`flex size-6 shrink-0 items-center justify-center rounded transition-colors ${
            showClosed
              ? "bg-primary/10 text-primary"
              : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
          }`}
        >
          {closedLoading ? <Loader2 size={13} className="animate-spin" /> : <CircleCheck size={13} />}
        </button>
      </div>

      {/* Group-by chip strip */}
      <div className="flex shrink-0 items-center gap-1 border-b border-border/50 px-2 py-1.5">
        <span className="text-[10px] uppercase tracking-wider text-muted-foreground">Group</span>
        {GROUP_BY_ORDER.map((mode) => (
          <button
            key={mode}
            type="button"
            onClick={() => setGroupBy(mode)}
            aria-pressed={groupBy === mode}
            title={`${GROUP_BY_LABEL[mode]} (Shift+←/→)`}
            className={`flex shrink-0 items-center rounded-full px-2.5 py-0.5 text-[11px] whitespace-nowrap transition-colors ${
              groupBy === mode
                ? "bg-secondary text-foreground"
                : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground/80"
            }`}
          >
            {GROUP_BY_LABEL[mode]}
          </button>
        ))}
      </div>

      {syncStatus?.state === "error" && syncStatus.error && (
        <div className="shrink-0 truncate bg-destructive/10 px-3 py-1 text-[10px] text-red-400" title={syncStatus.error}>
          Sync error: {syncStatus.error}
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto" data-scrollable>
        {visibleCount === 0 ? (
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
              closedIds={closedIds}
              showListName={groupBy !== "list"}
              onOpen={(id) => setDetailTaskId(id)}
            />
          ))
        )}
      </div>

      <ClickUpTaskDetail />
    </div>
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
  closedIds,
  showListName,
  onOpen,
}: {
  group: TaskGroup;
  expanded: boolean;
  onToggle: () => void;
  closedIds: ReadonlySet<string>;
  showListName: boolean;
  onOpen: (taskId: string) => void;
}) {
  // Subtasks nest under their parent only when the parent landed in the same
  // group; otherwise they render flat in their own group.
  const inGroup = new Set(group.tasks.map((t) => t.id));
  const roots = group.tasks.filter((t) => !t.parent_id || !inGroup.has(t.parent_id));
  const childrenOf = (id: string) => group.tasks.filter((t) => t.parent_id === id);

  return (
    <div>
      <button
        type="button"
        data-nav-item
        data-nav-expanded={expanded}
        data-group-key={group.key}
        onClick={onToggle}
        className="flex w-full items-center gap-1.5 px-3 py-1 text-left text-[10px] font-medium uppercase tracking-wider text-muted-foreground transition-colors hover:bg-secondary/30"
      >
        {expanded ? <ChevronDown size={10} className="shrink-0" /> : <ChevronRight size={10} className="shrink-0" />}
        {group.color && (
          <span className="size-1.5 shrink-0 rounded-full" style={{ background: group.color }} />
        )}
        <span className="truncate">{group.label}</span>
        <span className="tabular-nums text-muted-foreground/60">{group.tasks.length}</span>
      </button>
      {expanded &&
        roots.map((task) => (
          <div key={task.id}>
            <TaskRow task={task} closed={closedIds.has(task.id)} showListName={showListName} onOpen={onOpen} />
            {childrenOf(task.id).map((sub) => (
              <TaskRow
                key={sub.id}
                task={sub}
                closed={closedIds.has(sub.id)}
                showListName={showListName}
                onOpen={onOpen}
                indented
              />
            ))}
          </div>
        ))}
    </div>
  );
}

function TaskRow({
  task,
  closed,
  showListName,
  onOpen,
  indented = false,
}: {
  task: ClickUpTask;
  closed: boolean;
  showListName: boolean;
  onOpen: (taskId: string) => void;
  indented?: boolean;
}) {
  const overdue = task.due_date !== null && task.due_date < Date.now() && !closed;
  const priorityColor = task.priority ? PRIORITY_COLOR[task.priority] ?? "#d8d8d8" : null;

  return (
    <button
      type="button"
      data-nav-item
      onClick={() => onOpen(task.id)}
      className={`group flex w-full items-center gap-1.5 py-1 pr-3 text-left transition-colors hover:bg-secondary/40 ${
        indented ? "pl-7" : "pl-3"
      } ${closed ? "opacity-60" : ""}`}
    >
      <span
        className="size-1.5 shrink-0 rounded-full"
        style={{ background: task.status_color ?? "var(--color-muted-foreground)" }}
        title={task.status_name ?? undefined}
      />
      <span
        className={`min-w-0 flex-1 truncate text-[11px] ${
          closed ? "text-muted-foreground line-through" : "text-foreground/80"
        }`}
      >
        {task.name}
      </span>
      {priorityColor && (
        <Flag size={10} className="shrink-0" style={{ color: priorityColor }} aria-label={`Priority: ${task.priority}`} />
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
      {showListName && (
        <span className="max-w-20 shrink-0 truncate text-[10px] text-muted-foreground/70">
          {task.list_name}
        </span>
      )}
    </button>
  );
}
