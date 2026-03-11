import { useState } from "react";
import type { Task } from "@/lib/types";
import { Badge } from "@/components/ui/badge";

interface TaskItemProps {
  task: Task;
}

const STATUS_DOT_COLORS: Record<Task["status"], string> = {
  pending: "bg-muted-foreground",
  in_progress: "bg-orange-500",
  completed: "bg-green-500",
};

const STATUS_BADGE_VARIANT: Record<Task["status"], "outline" | "default" | "secondary"> = {
  pending: "outline",
  in_progress: "default",
  completed: "secondary",
};

const STATUS_LABELS: Record<Task["status"], string> = {
  pending: "Pending",
  in_progress: "In Progress",
  completed: "Done",
};

export function TaskItem({ task }: TaskItemProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="group">
      <button
        onClick={() => setExpanded((prev) => !prev)}
        className="flex w-full items-start gap-2.5 rounded-md px-2.5 py-2 text-left transition-colors hover:bg-muted/50"
        aria-expanded={expanded}
      >
        <span
          className={`mt-1.5 size-2 flex-shrink-0 rounded-full ${STATUS_DOT_COLORS[task.status]}`}
          aria-hidden="true"
        />

        <div className="min-w-0 flex-1">
          <span className="text-sm text-foreground">{task.subject}</span>
          {task.description && (
            <p className="mt-0.5 line-clamp-1 text-xs text-muted-foreground">
              {task.description}
            </p>
          )}
        </div>

        <Badge
          variant={STATUS_BADGE_VARIANT[task.status]}
          className="mt-0.5 h-4 flex-shrink-0 px-1.5 text-[10px]"
        >
          {STATUS_LABELS[task.status]}
        </Badge>
      </button>

      {expanded && (
        <div className="ml-5 mr-2.5 mb-2 space-y-1.5 rounded-md bg-muted/30 px-3 py-2">
          {task.description ? (
            <p className="text-xs text-muted-foreground">{task.description}</p>
          ) : (
            <p className="text-xs italic text-muted-foreground">No description</p>
          )}
          {task.active_form && (
            <p className="text-xs text-muted-foreground">
              Form: <span className="text-foreground">{task.active_form}</span>
            </p>
          )}
          {task.blocked_by && task.blocked_by.length > 0 && (
            <div className="flex flex-wrap items-center gap-1">
              <span className="text-xs text-muted-foreground">Blocked by:</span>
              {task.blocked_by.map((blocker) => (
                <Badge
                  key={blocker}
                  variant="destructive"
                  className="h-4 px-1.5 text-[10px]"
                >
                  {blocker}
                </Badge>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
