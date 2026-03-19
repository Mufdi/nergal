import { atom } from "jotai";
import { activeSessionIdAtom } from "./workspace";
import { invoke } from "@/lib/tauri";

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

interface ChangedFile {
  path: string;
  status: string;
}

/// Load changed files from git status for a session (used on session activation / app restart).
export const loadSessionFilesAtom = atom(null, async (_get, set, sessionId: string) => {
  try {
    const files = await invoke<ChangedFile[]>("get_session_changed_files", { sessionId });
    if (files.length === 0) return;

    set(fileMapAtom, (prev) => {
      const existing = prev[sessionId] ?? [];
      const existingPaths = new Set(existing.map((f) => f.path));

      const newEntries: ModifiedFile[] = [];
      for (const f of files) {
        if (!existingPaths.has(f.path)) {
          newEntries.push({ path: f.path, tool: f.status, timestamp: Date.now() });
        }
      }

      if (newEntries.length === 0) return prev;
      return { ...prev, [sessionId]: [...existing, ...newEntries] };
    });
  } catch {
    // silently ignore
  }
});
