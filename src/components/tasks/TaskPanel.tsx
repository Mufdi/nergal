import { useAtomValue } from "jotai";
import { activeSessionTasksAtom } from "@/stores/tasks";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { TaskItem } from "./TaskItem";

export function TaskPanel() {
  const tasks = useAtomValue(activeSessionTasksAtom);

  return (
    <section className="flex h-full flex-col" aria-label="Tasks">
      <header className="flex h-9 items-center justify-between border-b border-border px-3">
        <h2 className="text-xs font-medium text-muted-foreground">Tasks</h2>
        {tasks.length > 0 && (
          <Badge variant="secondary" className="h-4 px-1.5 text-[10px]">
            {tasks.length}
          </Badge>
        )}
      </header>

      {tasks.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-sm text-muted-foreground">No tasks yet</p>
        </div>
      ) : (
        <ScrollArea className="flex-1">
          <div className="divide-y divide-border/40 px-1 py-1">
            {tasks.map((task) => (
              <TaskItem key={task.id} task={task} />
            ))}
          </div>
        </ScrollArea>
      )}
    </section>
  );
}
