import { atom } from "jotai";
import { activeSessionIdAtom } from "./workspace";

export interface ModifiedFile {
  path: string;
  tool: string;
  timestamp: number;
}

// Map of session_id -> modified files (deduped by path, keeps latest)
export const fileMapAtom = atom<Record<string, ModifiedFile[]>>({});

export const activeSessionFilesAtom = atom<ModifiedFile[]>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return [];
  const map = get(fileMapAtom);
  return map[id] ?? [];
});
