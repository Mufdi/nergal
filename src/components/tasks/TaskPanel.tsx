import { useAtomValue } from "jotai";
import { activeSessionTasksAtom } from "@/stores/tasks";
import { TaskItem } from "./TaskItem";

export function TaskPanel() {
  const tasks = useAtomValue(activeSessionTasksAtom);

  if (tasks.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-xs text-muted-foreground">No tasks</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col py-0.5">
      {tasks.map((task) => (
        <TaskItem key={task.id} task={task} />
      ))}
    </div>
  );
}
