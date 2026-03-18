import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionTasksAtom, clearCompletedTasksAtom } from "@/stores/tasks";
import { TaskItem } from "./TaskItem";
import { Trash2 } from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

export function TaskPanel() {
  const tasks = useAtomValue(activeSessionTasksAtom);
  const clearCompleted = useSetAtom(clearCompletedTasksAtom);

  const hasCompleted = tasks.some((t) => t.status === "completed");

  if (tasks.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-xs text-muted-foreground">No tasks</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col">
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
