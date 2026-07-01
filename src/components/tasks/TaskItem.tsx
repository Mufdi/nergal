import { useState } from "react";
import { useSetAtom } from "jotai";
import { Trash2 } from "lucide-react";
import type { Task } from "@/lib/types";
import { removeTaskAtom } from "@/stores/tasks";

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
  const removeTask = useSetAtom(removeTaskAtom);

  return (
    <div className="group">
      <div className="flex items-center hover:bg-secondary/40 transition-colors">
        <button
          onClick={() => setExpanded((v) => !v)}
          className="flex min-w-0 flex-1 items-center gap-1.5 px-2.5 py-1 text-left"
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
        <span
          role="button"
          tabIndex={0}
          aria-label="Delete task"
          onClick={(e) => { e.stopPropagation(); removeTask(task.id); }}
          onKeyDown={(e) => {
            if (e.code === "Enter" || e.code === "Space") {
              e.preventDefault();
              e.stopPropagation();
              removeTask(task.id);
            }
          }}
          className="mr-1.5 hidden size-4 shrink-0 items-center justify-center rounded text-muted-foreground/70 hover:bg-secondary hover:text-foreground transition-colors group-hover:flex"
        >
          <Trash2 className="size-2.5" />
        </span>
      </div>

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
