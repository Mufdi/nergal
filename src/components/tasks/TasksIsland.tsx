import { useEffect, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionTasksAtom, clearCompletedTasksAtom, removeTaskAtom, taskMapAtom } from "@/stores/tasks";
import { activeSessionIdAtom } from "@/stores/workspace";
import { invoke } from "@/lib/tauri";
import type { Task } from "@/lib/types";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { ChevronDown, ChevronRight, Trash2, CheckCircle2, Circle } from "lucide-react";
import { PulseDots } from "@/components/ui/PulseDots";

// in_progress renders as a pulsing dot (see below), not a static glyph.
const STATUS_ICON: Record<string, typeof Circle> = {
  pending: Circle,
  completed: CheckCircle2,
};

const STATUS_COLOR: Record<string, string> = {
  pending: "text-muted-foreground",
  in_progress: "text-primary",
  completed: "text-green-500",
};

export function TasksIsland() {
  const tasks = useAtomValue(activeSessionTasksAtom);
  const clearCompleted = useSetAtom(clearCompletedTasksAtom);
  const removeTask = useSetAtom(removeTaskAtom);
  const setTaskMap = useSetAtom(taskMapAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const taskMap = useAtomValue(taskMapAtom);
  const [collapsed, setCollapsed] = useState(false);

  // Hydrate from DB the first time a session becomes active — the live
  // `tasks:update` stream only carries deltas, so a session that already
  // had tasks before the app started (or before the user switched to it)
  // would otherwise show an empty panel until the next TaskUpdate fires.
  useEffect(() => {
    if (!sessionId) return;
    if (sessionId in taskMap) return;
    invoke<Task[]>("get_tasks", { sessionId })
      .then((loaded) => setTaskMap((prev) => ({ ...prev, [sessionId]: loaded })))
      .catch(() => setTaskMap((prev) => ({ ...prev, [sessionId]: [] })));
  }, [sessionId, taskMap, setTaskMap]);

  if (tasks.length === 0) return null;

  const hasCompleted = tasks.some((t) => t.status === "completed");
  const activeCount = tasks.filter((t) => t.status !== "completed").length;

  return (
    <div className="shrink-0 rounded-lg bg-card overflow-hidden">
      {/* Header */}
      <button
        type="button"
        onClick={() => setCollapsed(!collapsed)}
        className="flex w-full items-center gap-1.5 px-3 py-2 text-left hover:bg-secondary/30"
      >
        {collapsed ? (
          <ChevronRight className="size-3 text-muted-foreground" />
        ) : (
          <ChevronDown className="size-3 text-muted-foreground" />
        )}
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Tasks
        </span>
        <span className="ml-auto text-[10px] text-muted-foreground">
          {activeCount > 0 ? activeCount : tasks.length}
        </span>
        {hasCompleted && (
          <div
            role="button"
            tabIndex={0}
            onClick={(e) => { e.stopPropagation(); clearCompleted(); }}
            onKeyDown={(e) => { if (e.key === "Enter") { e.stopPropagation(); clearCompleted(); } }}
            className="ml-1 rounded p-0.5 text-muted-foreground hover:text-foreground"
            aria-label="Clear completed"
          >
            <Trash2 className="size-3" />
          </div>
        )}
      </button>

      {/* Task list */}
      {!collapsed && (
        <div className="max-h-40 overflow-y-auto px-2 pb-2">
          {tasks.map((task) => {
            const Icon = STATUS_ICON[task.status] ?? Circle;
            const color = STATUS_COLOR[task.status] ?? "text-muted-foreground";
            return (
              // data-nav-item opts the row into the sidebar's hover/selected
              // "d" shortcut, which clicks the aria-label="Delete" action.
              <div
                key={task.id}
                data-nav-item
                className="group flex items-center gap-2 rounded px-2 py-1 hover:bg-secondary/30"
              >
                {task.status === "in_progress" ? (
                  <PulseDots count={1} className={`flex-shrink-0 ${color}`} dotClassName="size-2" />
                ) : (
                  <Icon className={`size-3 flex-shrink-0 ${color}`} />
                )}
                <TooltipProvider delay={0}>
                  <Tooltip>
                    <TooltipTrigger
                      render={
                        <span
                          className={`min-w-0 flex-1 truncate text-xs ${task.status === "completed" ? "text-muted-foreground line-through" : "text-foreground"}`}
                        />
                      }
                    >
                      {task.subject}
                    </TooltipTrigger>
                    <TooltipContent side="right" className="max-w-md text-[10px]">
                      {task.subject}
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>
                <span
                  role="button"
                  tabIndex={0}
                  aria-label="Delete"
                  onClick={(e) => { e.stopPropagation(); removeTask(task.id); }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      e.stopPropagation();
                      removeTask(task.id);
                    }
                  }}
                  className="hidden size-4 shrink-0 items-center justify-center rounded text-muted-foreground/70 hover:bg-secondary hover:text-foreground transition-colors group-hover:flex"
                >
                  <Trash2 className="size-2.5" />
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
