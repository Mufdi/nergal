import { atom } from "jotai";
import type { Task } from "@/lib/types";
import { invoke } from "@/lib/tauri";
import { activeSessionIdAtom } from "./workspace";

export const taskMapAtom = atom<Record<string, Task[]>>({});

export const activeSessionTasksAtom = atom<Task[]>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return [];
  const map = get(taskMapAtom);
  return map[id] ?? [];
});

export const removeTaskAtom = atom(null, (get, set, taskId: string) => {
  const id = get(activeSessionIdAtom);
  if (!id) return;
  set(taskMapAtom, (prev) => {
    const tasks = prev[id] ?? [];
    return { ...prev, [id]: tasks.filter((t) => t.id !== taskId) };
  });
  // Persist the tombstone — a frontend-only filter is undone by the next
  // get_tasks hydration / TodoWrite emit (BUG-12).
  void invoke("delete_task", { sessionId: id, taskId }).catch(() => {});
});

export const clearCompletedTasksAtom = atom(null, (get, set) => {
  const id = get(activeSessionIdAtom);
  if (!id) return;
  set(taskMapAtom, (prev) => {
    const tasks = prev[id] ?? [];
    return { ...prev, [id]: tasks.filter((t) => t.status !== "completed") };
  });
  void invoke("clear_completed_tasks", { sessionId: id }).catch(() => {});
});

export const clearAllTasksAtom = atom(null, (get, set) => {
  const id = get(activeSessionIdAtom);
  if (!id) return;
  set(taskMapAtom, (prev) => ({ ...prev, [id]: [] }));
  void invoke("delete_all_tasks", { sessionId: id }).catch(() => {});
});
