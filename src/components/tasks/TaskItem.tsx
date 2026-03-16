import { useState } from "react";
import type { Task } from "@/lib/types";

interface TaskItemProps {
  task: Task;
}

const STATUS_DOT_COLORS: Record<Task["status"], string> = {
  pending: "bg-muted-foreground/60",
  in_progress: "bg-orange-500",
  completed: "bg-green-500/70",
};

const STATUS_LABELS: Record<Task["status"], string> = {
  pending: "pending",
  in_progress: "active",
  completed: "done",
};

export function TaskItem({ task }: TaskItemProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div>
      <button
        onClick={() => setExpanded((v) => !v)}
        className="flex w-full items-center gap-1.5 px-2.5 py-1 text-left hover:bg-secondary/40 transition-colors"
      >
        <span
          className={`size-1.5 shrink-0 rounded-full ${STATUS_DOT_COLORS[task.status]}`}
          aria-hidden="true"
        />
        <span className={`min-w-0 flex-1 truncate text-[11px] ${
          task.status === "completed"
            ? "text-muted-foreground line-through"
            : "text-foreground/80"
        }`}>
          {task.subject}
        </span>
        <span className="shrink-0 text-[10px] text-muted-foreground/60">
          {STATUS_LABELS[task.status]}
        </span>
      </button>

      {expanded && task.description && (
        <div className="px-2.5 pb-1.5 pl-6">
          <p className="text-[10px] leading-relaxed text-muted-foreground">
            {task.description}
          </p>
        </div>
      )}
    </div>
  );
}
