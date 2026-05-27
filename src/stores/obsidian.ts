import { atom, type getDefaultStore } from "jotai";
import { invoke, listen } from "@/lib/tauri";
import type { ObsidianConfig, ResolvedObsidianConfig } from "@/lib/types";
import { activeWorkspaceAtom } from "./workspace";
import { loadObsidianTemplates } from "./obsidianTemplates";
import type { UnlistenFn } from "@tauri-apps/api/event";

type Store = ReturnType<typeof getDefaultStore>;

export const obsidianDefaultConfig: ResolvedObsidianConfig = {
  vault_root: null,
  vault_name: null,
  session_log_path: null,
  quick_capture_path: null,
  moc_path: null,
  templates_path: null,
  backlinks_enabled: false,
  render_wikilinks: true,
};

export const obsidianConfigAtom = atom<ResolvedObsidianConfig | null>(null);

export const obsidianEnabledAtom = atom((get) => {
  const cfg = get(obsidianConfigAtom);
  return cfg?.vault_root != null && cfg.vault_root !== "";
});

// Out of component so the dialog footer can render Apply contextually.
export const obsidianDraftAtom = atom<ObsidianConfig>(obsidianDefaultConfig);
export const obsidianApplyBusyAtom = atom<boolean>(false);

export const obsidianDraftDirtyAtom = atom((get) => {
  const draft = get(obsidianDraftAtom);
  const resolved = get(obsidianConfigAtom) ?? obsidianDefaultConfig;
  return (
    draft.vault_root !== resolved.vault_root ||
    draft.vault_name !== resolved.vault_name ||
    draft.session_log_path !== resolved.session_log_path ||
    draft.quick_capture_path !== resolved.quick_capture_path ||
    draft.moc_path !== resolved.moc_path ||
    draft.templates_path !== resolved.templates_path ||
    draft.backlinks_enabled !== resolved.backlinks_enabled ||
    draft.render_wikilinks !== resolved.render_wikilinks
  );
});

export const resetObsidianDraftAtom = atom(null, (get, set) => {
  set(obsidianDraftAtom, get(obsidianConfigAtom) ?? obsidianDefaultConfig);
});

export interface BootstrapPromptState {
  workspaceId: string;
  workspaceName: string;
  expectedPath: string;
  inheritedVault?: boolean;
}

export const bootstrapPromptAtom = atom<BootstrapPromptState | null>(null);

export const loadObsidianConfigAtom = atom(null, async (get, set) => {
  const ws = get(activeWorkspaceAtom);
  if (!ws) {
    set(obsidianConfigAtom, obsidianDefaultConfig);
    return;
  }
  try {
    const cfg = await invoke<ResolvedObsidianConfig>("get_obsidian_config", {
      workspaceId: ws.id,
    });
    set(obsidianConfigAtom, cfg);
  } catch (err) {
    console.warn("[obsidian] get_obsidian_config failed:", err);
    set(obsidianConfigAtom, obsidianDefaultConfig);
  }
});

export const saveObsidianConfigAtom = atom(
  null,
  async (get, set, cfg: ObsidianConfig): Promise<ResolvedObsidianConfig | null> => {
    const ws = get(activeWorkspaceAtom);
    if (!ws) return null;
    try {
      const resolved = await invoke<ResolvedObsidianConfig>("save_obsidian_config", {
        workspaceId: ws.id,
        cfg,
      });
      set(obsidianConfigAtom, resolved);
      return resolved;
    } catch (err) {
      console.warn("[obsidian] save_obsidian_config failed:", err);
      throw err;
    }
  },
);

export async function setupObsidianListeners(store: Store): Promise<UnlistenFn[]> {
  const unlisteners: UnlistenFn[] = [];

  unlisteners.push(
    await listen<ResolvedObsidianConfig>("obsidian:config-changed", (payload) => {
      store.set(obsidianConfigAtom, payload);
      const ws = store.get(activeWorkspaceAtom);
      void loadObsidianTemplates(ws?.id ?? null);
    }),
  );

  const unsubWorkspace = store.sub(activeWorkspaceAtom, () => {
    const ws = store.get(activeWorkspaceAtom);
    store.set(loadObsidianConfigAtom);
    void loadObsidianTemplates(ws?.id ?? null);
  });
  unlisteners.push(unsubWorkspace as UnlistenFn);

  await store.set(loadObsidianConfigAtom);
  const ws = store.get(activeWorkspaceAtom);
  await loadObsidianTemplates(ws?.id ?? null);

  return unlisteners;
}
