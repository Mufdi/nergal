import { atom } from "jotai";
import { activeSessionAtom } from "./session";

export interface ModifiedFile {
  path: string;
  tool: string;
  timestamp: number;
}

// Map of session_id → modified files (deduped by path, keeps latest)
export const fileMapAtom = atom<Record<string, ModifiedFile[]>>({});

export const activeSessionFilesAtom = atom<ModifiedFile[]>((get) => {
  const session = get(activeSessionAtom);
  if (!session) return [];
  const map = get(fileMapAtom);
  return map[session.id] ?? [];
});
