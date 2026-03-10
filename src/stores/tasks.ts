import { atom } from "jotai";
import type { Task } from "@/lib/types";
import { activeSessionAtom } from "./session";

export const taskMapAtom = atom<Record<string, Task[]>>({});

export const activeSessionTasksAtom = atom<Task[]>((get) => {
  const session = get(activeSessionAtom);
  if (!session) return [];
  const map = get(taskMapAtom);
  return map[session.id] ?? [];
});
