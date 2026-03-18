import { atom } from "jotai";
import type { Task } from "@/lib/types";
import { activeSessionIdAtom } from "./workspace";

export const taskMapAtom = atom<Record<string, Task[]>>({});

export const activeSessionTasksAtom = atom<Task[]>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return [];
  const map = get(taskMapAtom);
  return map[id] ?? [];
});

export const clearCompletedTasksAtom = atom(null, (get, set) => {
  const id = get(activeSessionIdAtom);
  if (!id) return;
  set(taskMapAtom, (prev) => {
    const tasks = prev[id] ?? [];
    return { ...prev, [id]: tasks.filter((t) => t.status !== "completed") };
  });
});
