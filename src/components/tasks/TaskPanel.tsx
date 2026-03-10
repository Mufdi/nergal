import { useAtomValue } from "jotai";
import { activeSessionTasksAtom } from "@/stores/tasks";
import { TaskItem } from "./TaskItem";

export function TaskPanel() {
  const tasks = useAtomValue(activeSessionTasksAtom);

  return (
    <section className="flex h-full flex-col" aria-label="Tasks">
      <header className="flex h-8 items-center border-b border-border px-3">
        <h2 className="text-xs font-medium text-text-muted">
          Tasks {tasks.length > 0 && <span className="text-text">({tasks.length})</span>}
        </h2>
      </header>

      <div className="flex-1 overflow-y-auto">
        {tasks.length === 0 ? (
          <p className="px-3 py-4 text-center text-xs text-text-muted">No tasks</p>
        ) : (
          tasks.map((task) => <TaskItem key={task.id} task={task} />)
        )}
      </div>
    </section>
  );
}
