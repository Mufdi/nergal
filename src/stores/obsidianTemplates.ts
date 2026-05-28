import { atom } from "jotai";
import { invoke } from "@/lib/tauri";
import { appStore } from "./jotaiStore";

export interface ObsidianTemplate {
  filename: string;
  name: string;
  description: string | null;
  body: string;
}

export const obsidianTemplatesAtom = atom<ObsidianTemplate[]>([]);

export async function loadObsidianTemplates(workspaceId: string | null): Promise<void> {
  if (!workspaceId) {
    appStore.set(obsidianTemplatesAtom, []);
    return;
  }
  try {
    const list = await invoke<ObsidianTemplate[]>("obsidian_watch_templates", {
      workspaceId,
    });
    appStore.set(obsidianTemplatesAtom, list);
  } catch (err) {
    console.warn("[obsidianTemplates] load failed:", err);
    appStore.set(obsidianTemplatesAtom, []);
  }
}
