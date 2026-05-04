import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionTasksAtom, clearCompletedTasksAtom } from "@/stores/tasks";
import { hasCapabilityAtom } from "@/stores/agent";
import { TaskItem } from "./TaskItem";
import { Trash2 } from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

export function TaskPanel() {
  const supportsTaskList = useAtomValue(hasCapabilityAtom("TASK_LIST"));
  const tasks = useAtomValue(activeSessionTasksAtom);
  const clearCompleted = useSetAtom(clearCompletedTasksAtom);

  if (!supportsTaskList) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-xs text-muted-foreground">
          The active agent does not expose task lists.
        </p>
      </div>
    );
  }

  const hasCompleted = tasks.some((t) => t.status === "completed");

  if (tasks.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-xs text-muted-foreground">No tasks</p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      <div className="flex items-center justify-between px-3 py-1.5">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Tasks ({tasks.length})
        </span>
        {hasCompleted && (
          <Tooltip>
            <TooltipTrigger
              render={
                <div
                  role="button"
                  onClick={clearCompleted}
                  className="flex size-5 items-center justify-center rounded text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
                  aria-label="Clear completed tasks"
                />
              }
            >
              <Trash2 size={12} />
            </TooltipTrigger>
            <TooltipContent side="top">Clear completed</TooltipContent>
          </Tooltip>
        )}
      </div>
      {tasks.map((task) => (
        <TaskItem key={task.id} task={task} />
      ))}
    </div>
  );
}
