import { atom, type getDefaultStore } from "jotai";
import { invoke, listen } from "@/lib/tauri";
import type { ObsidianConfig, ResolvedObsidianConfig } from "@/lib/types";
import { activeWorkspaceAtom } from "./workspace";
import {
  loadObsidianTemplates,
  obsidianTemplatesAtom,
  type ObsidianTemplate,
} from "./obsidianTemplates";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { toastsAtom } from "./toast";

type Store = ReturnType<typeof getDefaultStore>;

export interface ObsidianConfigChangedPayload {
  workspace_id: string;
  config: ResolvedObsidianConfig;
}

/// Startup outcome of the post-session bg runner (see post_session.rs).
export interface PostSessionStartupPayload {
  recovered: number;
  last_run_failed: boolean;
  bg_unavailable: boolean;
  log_path: string;
}

export const obsidianDefaultConfig: ResolvedObsidianConfig = {
  vault_root: null,
  vault_name: null,
  session_log_path: null,
  quick_capture_path: null,
  moc_path: null,
  templates_path: null,
  backlinks_enabled: false,
  render_wikilinks: true,
  search_subdir: null,
};

export const obsidianConfigAtom = atom<ResolvedObsidianConfig | null>(null);

// "subdir" is a no-op unless the active config has a non-empty search_subdir.
export const vaultSearchScopeAtom = atom<"whole" | "subdir">("whole");

export const obsidianEnabledAtom = atom((get) => {
  const cfg = get(obsidianConfigAtom);
  return cfg?.vault_root != null && cfg.vault_root !== "";
});

export const obsidianSelectedWorkspaceIdAtom = atom<string | null>(null);
export const obsidianSettingsResolvedAtom = atom<ResolvedObsidianConfig | null>(null);

export const obsidianDraftAtom = atom<ObsidianConfig>(obsidianDefaultConfig);
export const obsidianApplyBusyAtom = atom<boolean>(false);

export const obsidianDraftDirtyAtom = atom((get) => {
  const draft = get(obsidianDraftAtom);
  const resolved = get(obsidianSettingsResolvedAtom) ?? obsidianDefaultConfig;
  return (
    draft.vault_root !== resolved.vault_root ||
    draft.vault_name !== resolved.vault_name ||
    draft.session_log_path !== resolved.session_log_path ||
    draft.quick_capture_path !== resolved.quick_capture_path ||
    draft.moc_path !== resolved.moc_path ||
    draft.templates_path !== resolved.templates_path ||
    draft.backlinks_enabled !== resolved.backlinks_enabled ||
    draft.render_wikilinks !== resolved.render_wikilinks ||
    draft.search_subdir !== resolved.search_subdir
  );
});

export const resetObsidianDraftAtom = atom(null, (get, set) => {
  set(obsidianDraftAtom, get(obsidianSettingsResolvedAtom) ?? obsidianDefaultConfig);
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
  async (
    get,
    set,
    args: { workspaceId: string; cfg: ObsidianConfig },
  ): Promise<ResolvedObsidianConfig | null> => {
    try {
      const resolved = await invoke<ResolvedObsidianConfig>("save_obsidian_config", {
        workspaceId: args.workspaceId,
        cfg: args.cfg,
      });
      set(obsidianSettingsResolvedAtom, resolved);
      const active = get(activeWorkspaceAtom);
      if (active && active.id === args.workspaceId) {
        set(obsidianConfigAtom, resolved);
      }
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
    await listen<ObsidianConfigChangedPayload>("obsidian:config-changed", (payload) => {
      const active = store.get(activeWorkspaceAtom);
      if (active && active.id === payload.workspace_id) {
        store.set(obsidianConfigAtom, payload.config);
        void loadObsidianTemplates(active.id);
      }
      const selected = store.get(obsidianSelectedWorkspaceIdAtom);
      const effective = active?.id ?? selected;
      if (effective === payload.workspace_id) {
        store.set(obsidianSettingsResolvedAtom, payload.config);
      }
    }),
  );

  unlisteners.push(
    await listen<ObsidianTemplate[]>("obsidian:templates-updated", (payload) => {
      store.set(obsidianTemplatesAtom, payload);
    }),
  );

  unlisteners.push(
    await listen<PostSessionStartupPayload>("post-session:startup", (payload) => {
      // Recovery of orphaned markers runs silently — the count is internal
      // plumbing (often stale sessions) with nothing actionable for the user.
      // Only surface the two states that warrant action.
      if (payload.last_run_failed) {
        store.set(toastsAtom, {
          message: "Last session snapshot failed",
          description: `See ${payload.log_path}`,
          type: "error",
        });
      }
      if (payload.bg_unavailable) {
        store.set(toastsAtom, {
          message: "Background snapshots disabled",
          description: "This host blocks detached processing; snapshots run inline on close.",
          type: "error",
        });
      }
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
