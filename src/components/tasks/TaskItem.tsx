import { useState } from "react";
import type { Task } from "@/lib/types";

interface TaskItemProps {
  task: Task;
}

const STATUS_INDICATORS: Record<Task["status"], { icon: string; color: string }> = {
  pending: { icon: "\u25CB", color: "text-text-muted" },
  in_progress: { icon: "\u25CF", color: "text-accent" },
  completed: { icon: "\u2713", color: "text-success" },
};

export function TaskItem({ task }: TaskItemProps) {
  const [expanded, setExpanded] = useState(false);
  const { icon, color } = STATUS_INDICATORS[task.status];

  return (
    <div className="border-b border-border">
      <button
        onClick={() => setExpanded((prev) => !prev)}
        className="flex w-full items-start gap-2 px-3 py-2 text-left text-sm hover:bg-surface-raised"
        aria-expanded={expanded}
      >
        <span className={`mt-0.5 flex-shrink-0 font-mono ${color}`} aria-hidden="true">
          {icon}
        </span>
        <span className="flex-1 text-text">{task.subject}</span>
        {task.blocked_by && task.blocked_by.length > 0 && (
          <span className="text-xs text-warning" title="Blocked">
            blocked
          </span>
        )}
      </button>

      {expanded && (
        <div className="border-t border-border bg-surface-raised px-3 py-2">
          {task.description ? (
            <p className="text-xs text-text-muted">{task.description}</p>
          ) : (
            <p className="text-xs text-text-muted italic">No description</p>
          )}
          {task.active_form && (
            <p className="mt-1 text-xs text-text-muted">
              Form: <span className="text-text">{task.active_form}</span>
            </p>
          )}
          {task.blocked_by && task.blocked_by.length > 0 && (
            <p className="mt-1 text-xs text-text-muted">
              Blocked by: <span className="text-warning">{task.blocked_by.join(", ")}</span>
            </p>
          )}
        </div>
      )}
    </div>
  );
}
