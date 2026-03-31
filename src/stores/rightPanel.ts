import { atom } from "jotai";
import { activeSessionIdAtom } from "./workspace";
import { closedTabsStackAtom } from "./shortcuts";

export type TabType = "plan" | "diff" | "spec" | "tasks" | "git" | "transcript" | "file";

export type PanelCategory = "document" | "tool";

export const PANEL_CATEGORY_MAP: Record<TabType, PanelCategory> = {
  plan: "document",
  spec: "document",
  transcript: "document",
  file: "document",
  git: "tool",
  diff: "tool",
  tasks: "document",
};

export interface Tab {
  id: string;
  type: TabType;
  label: string;
  pinned: boolean;
  dirty: boolean;
  category: PanelCategory;
  data?: Record<string, unknown>;
}

export interface TabState {
  tabs: Tab[];
  activeTabId: string | null;
  previewTabId: string | null;
}

const SINGLETON_TYPES: TabType[] = ["tasks", "git"];
const defaultTabState: TabState = { tabs: [], activeTabId: null, previewTabId: null };

export const expandRightPanelAtom = atom(0);
export const tabOpenedSignalAtom = atom(0);

export const activePanelViewAtom = atom<TabType | null>(null);

export const tabStateMapAtom = atom<Record<string, TabState>>({});

export const activeTabStateAtom = atom<TabState>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return defaultTabState;
  return get(tabStateMapAtom)[id] ?? defaultTabState;
});

export const activeTabsAtom = atom<Tab[]>((get) => {
  return get(activeTabStateAtom).tabs;
});

export const activeTabAtom = atom<Tab | null>((get) => {
  const state = get(activeTabStateAtom);
  if (!state.activeTabId) return null;
  const tab = state.tabs.find((t) => t.id === state.activeTabId) ?? null;
  if (tab && !tab.category) {
    return { ...tab, category: PANEL_CATEGORY_MAP[tab.type] };
  }
  return tab;
});

export const activeTabIdAtom = atom(
  (get) => get(activeTabStateAtom).activeTabId,
  (get, set, tabId: string | null) => {
    const sessionId = get(activeSessionIdAtom);
    if (!sessionId) return;
    set(tabStateMapAtom, (prev) => {
      const state = prev[sessionId] ?? defaultTabState;
      return { ...prev, [sessionId]: { ...state, activeTabId: tabId } };
    });
  },
);

export const openTabAction = atom(
  null,
  (get, set, params: { tab: Omit<Tab, "pinned" | "dirty" | "category"> & { dirty?: boolean }; isPinned?: boolean }) => {
    const sessionId = get(activeSessionIdAtom);
    if (!sessionId) return;

    const { tab: partial } = params;

    set(tabOpenedSignalAtom, (n) => n + 1);
    set(tabStateMapAtom, (prev) => {
      const state = prev[sessionId] ?? defaultTabState;
      const isSingleton = SINGLETON_TYPES.includes(partial.type);

      if (isSingleton) {
        const existing = state.tabs.find((t) => t.type === partial.type);
        if (existing) {
          return { ...prev, [sessionId]: { ...state, activeTabId: existing.id } };
        }
      }

      const existingById = state.tabs.find((t) => t.id === partial.id);
      if (existingById) {
        return { ...prev, [sessionId]: { ...state, activeTabId: existingById.id } };
      }

      const newTab: Tab = {
        id: partial.id,
        type: partial.type,
        label: partial.label,
        pinned: true,
        dirty: partial.dirty ?? false,
        category: PANEL_CATEGORY_MAP[partial.type],
        data: partial.data,
      };

      const tabs = [...state.tabs, newTab];
      return { ...prev, [sessionId]: { ...state, tabs, activeTabId: newTab.id } };
    });
  },
);

export const closeTabAction = atom(null, (get, set, tabId: string) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) return;

  set(tabStateMapAtom, (prev) => {
    const state = prev[sessionId] ?? defaultTabState;
    const tab = state.tabs.find((t) => t.id === tabId);
    if (!tab) return prev;

    if (tab.dirty) return prev;

    const index = state.tabs.findIndex((t) => t.id === tabId);
    const next = state.tabs.filter((t) => t.id !== tabId);

    set(closedTabsStackAtom, (stack) => [...stack, tab]);

    let activeTabId = state.activeTabId;
    if (activeTabId === tabId) {
      const newActive = next[Math.min(index, next.length - 1)] ?? null;
      activeTabId = newActive?.id ?? null;
    }

    let previewTabId = state.previewTabId;
    if (previewTabId === tabId) {
      previewTabId = null;
    }

    return { ...prev, [sessionId]: { tabs: next, activeTabId, previewTabId } };
  });
});

export const pinTabAction = atom(null, (get, set, tabId: string) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) return;

  set(tabStateMapAtom, (prev) => {
    const state = prev[sessionId] ?? defaultTabState;
    const tabs = state.tabs.map((t) => (t.id === tabId ? { ...t, pinned: true } : t));
    const previewTabId = state.previewTabId === tabId ? null : state.previewTabId;
    return { ...prev, [sessionId]: { ...state, tabs, previewTabId } };
  });
});

export const reorderTabsAction = atom(null, (get, set, params: { sourceId: string; targetId: string; side: "left" | "right" }) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) return;

  set(tabStateMapAtom, (prev) => {
    const state = prev[sessionId] ?? defaultTabState;
    const source = state.tabs.find((t) => t.id === params.sourceId);
    if (!source) return prev;
    const filtered = state.tabs.filter((t) => t.id !== params.sourceId);
    const targetIdx = filtered.findIndex((t) => t.id === params.targetId);
    if (targetIdx === -1) return prev;
    const insertIdx = params.side === "right" ? targetIdx + 1 : targetIdx;
    filtered.splice(insertIdx, 0, source);
    return { ...prev, [sessionId]: { ...state, tabs: filtered } };
  });
});

export const setDirtyAction = atom(null, (get, set, params: { tabId: string; dirty: boolean }) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) return;

  set(tabStateMapAtom, (prev) => {
    const state = prev[sessionId] ?? defaultTabState;
    const tabs = state.tabs.map((t) => (t.id === params.tabId ? { ...t, dirty: params.dirty } : t));
    return { ...prev, [sessionId]: { ...state, tabs } };
  });
});

export const reopenTabAction = atom(null, (get, set) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) return;

  const stack = get(closedTabsStackAtom);
  if (stack.length === 0) return;

  const tab = stack[stack.length - 1];
  set(closedTabsStackAtom, stack.slice(0, -1));

  set(tabStateMapAtom, (prev) => {
    const state = prev[sessionId] ?? defaultTabState;
    const tabs = [...state.tabs, { ...tab, dirty: false }];
    return { ...prev, [sessionId]: { tabs, activeTabId: tab.id, previewTabId: state.previewTabId } };
  });
});

/// Tracks the current spec artifact being viewed (changeName + artifactPath).
/// Updated by SpecPanel, read by TopBar for "Open in IDE".
export const currentSpecArtifactAtom = atom<{ changeName: string; artifactPath: string } | null>(null);

/// Returns the category of the currently active panel/tab, or null if no panel is open.
/// Used by the layout preset engine to determine target proportions.
export const activePanelCategoryAtom = atom<PanelCategory | null>((get) => {
  const activeTab = get(activeTabAtom);
  if (activeTab) return activeTab.category;
  const panelView = get(activePanelViewAtom);
  if (panelView) return PANEL_CATEGORY_MAP[panelView];
  return null;
});
