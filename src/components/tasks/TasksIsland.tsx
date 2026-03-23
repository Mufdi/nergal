import { useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionTasksAtom, clearCompletedTasksAtom } from "@/stores/tasks";
import { ChevronDown, ChevronRight, Trash2, CheckCircle2, Circle, Loader2 } from "lucide-react";

const STATUS_ICON: Record<string, typeof Circle> = {
  pending: Circle,
  in_progress: Loader2,
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
  const [collapsed, setCollapsed] = useState(false);

  if (tasks.length === 0) return null;

  const hasCompleted = tasks.some((t) => t.status === "completed");
  const activeCount = tasks.filter((t) => t.status !== "completed").length;

  return (
    <div className="mt-auto border-t border-border/50 bg-card/50">
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
              <div
                key={task.id}
                className="flex items-center gap-2 rounded px-2 py-1 hover:bg-secondary/30"
              >
                <Icon className={`size-3 flex-shrink-0 ${color} ${task.status === "in_progress" ? "animate-spin" : ""}`} />
                <span className={`truncate text-xs ${task.status === "completed" ? "text-muted-foreground line-through" : "text-foreground"}`}>
                  {task.subject}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
