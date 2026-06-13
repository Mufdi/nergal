import { atom } from "jotai";
import { activeSessionIdAtom } from "./workspace";
import { closedTabsStackAtom } from "./shortcuts";
// Safe cycle with pinnedNotes.ts: both sides only touch the imported atoms inside callbacks.
import { pinnedNotesMapAtom } from "./pinnedNotes";
import { toastsAtom } from "./toast";

export type TabType = "plan" | "diff" | "spec" | "tasks" | "git" | "transcript" | "file" | "browser" | "obsidiannote" | "clickup" | "clickup-task";

export type PanelCategory = "document" | "tool";

/// Display label for a standalone view panel (and its virtual tab in the
/// TabBar). Single source for RightPanel's header + the TabBar virtual tab.
export function viewPanelLabel(view: TabType): string {
  const labels: Record<TabType, string> = {
    plan: "Plans",
    file: "Files",
    diff: "Diff",
    spec: "Spec",
    tasks: "Tasks",
    git: "Git",
    transcript: "Transcript",
    browser: "Browser",
    obsidiannote: "Obsidian",
    clickup: "ClickUp",
    "clickup-task": "Task",
  };
  return labels[view];
}

export const PANEL_CATEGORY_MAP: Record<TabType, PanelCategory> = {
  plan: "document",
  spec: "document",
  transcript: "document",
  file: "document",
  git: "tool",
  diff: "tool",
  tasks: "document",
  browser: "tool",
  obsidiannote: "document",
  clickup: "tool",
  // A single task opened as a tab — content document, not a launcher/tool view.
  "clickup-task": "document",
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

const SINGLETON_TYPES: TabType[] = ["tasks", "git", "browser", "clickup"];
const defaultTabState: TabState = { tabs: [], activeTabId: null, previewTabId: null };

export const expandRightPanelAtom = atom(0);
export const tabOpenedSignalAtom = atom(0);

/// Whether the right panel's file-picker overlay (Ctrl+Shift+K) is open. Lifted
/// out of RightPanel local state so DiffView can bail its capture-phase chunk
/// listener while the picker owns arrow keys — without this, DiffView's
/// `j`/`k`/arrow handler stops the picker from receiving keys (capture wins
/// over React's onKeyDown bubble).
export const filePickerOpenAtom = atom(false);

/// Per-session standalone panel view (e.g. Git panel opened via Ctrl+Shift+G
/// without a tab). Keyed by sessionId so each session remembers what it was
/// showing — switching sessions and back restores the panel without needing
/// a tab to anchor it. Tabs already persist via `tabStateMapAtom`; this is
/// the equivalent for tab-less standalone panels.
export const activePanelViewMapAtom = atom<Record<string, TabType | null>>({});

/// Missing entry means "no user gesture yet, defer to layout preset" —
/// the layout effect needs this third state to avoid clobbering the
/// preset on first encounter while still respecting a saved gesture.
export const rightPanelCollapsedMapAtom = atom<Record<string, boolean>>({});

/// Sentinel map key for panel state when no session is active. Session-less
/// views (ClickUp reads a global mirror) must still open — without this the
/// facade write no-ops and the user gets an expanded-but-empty right panel.
export const NO_SESSION_PANEL_KEY = "__no-session__";

/// Reader/writer facade keyed by the active session. All call sites use
/// `useAtomValue` / `useSetAtom`, so flipping this from primitive to derived
/// is transparent — they keep working without touching their code.
export const activePanelViewAtom = atom<TabType | null, [TabType | null], void>(
  (get) => {
    const key = get(activeSessionIdAtom) ?? NO_SESSION_PANEL_KEY;
    return get(activePanelViewMapAtom)[key] ?? null;
  },
  (get, set, view: TabType | null) => {
    const key = get(activeSessionIdAtom) ?? NO_SESSION_PANEL_KEY;
    set(activePanelViewMapAtom, (prev) => ({ ...prev, [key]: view }));
  },
);

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
  (get, set, params: { tab: Omit<Tab, "pinned" | "dirty" | "category"> & { dirty?: boolean }; isPinned?: boolean; activate?: boolean }) => {
    const sessionId = get(activeSessionIdAtom);
    if (!sessionId) return;

    const { tab: partial } = params;
    const activate = params.activate ?? true;

    // activate:false restores are background ops (pinned-note tabs on a
    // session switch) — the signal would focus the panel.
    if (activate) set(tabOpenedSignalAtom, (n) => n + 1);
    set(tabStateMapAtom, (prev) => {
      const state = prev[sessionId] ?? defaultTabState;
      const isSingleton = SINGLETON_TYPES.includes(partial.type);
      const focused = (id: string) => (activate ? id : state.activeTabId);

      if (isSingleton) {
        const existing = state.tabs.find((t) => t.type === partial.type);
        if (existing) {
          return { ...prev, [sessionId]: { ...state, activeTabId: focused(existing.id) } };
        }
      }

      const existingById = state.tabs.find((t) => t.id === partial.id);
      if (existingById) {
        return { ...prev, [sessionId]: { ...state, activeTabId: focused(existingById.id) } };
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
      return { ...prev, [sessionId]: { ...state, tabs, activeTabId: focused(newTab.id) } };
    });
  },
);

export const closeTabAction = atom(null, (get, set, tabId: string) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) return;

  // Closing a context-pinned note would silently desync the pin from the
  // agent's context — require an explicit unpin first.
  const target = (get(tabStateMapAtom)[sessionId] ?? defaultTabState).tabs.find((t) => t.id === tabId);
  if (target?.type === "obsidiannote") {
    const pinned = get(pinnedNotesMapAtom)[sessionId] ?? [];
    if (pinned.includes(target.data?.path as string)) {
      set(toastsAtom, {
        message: "Note is pinned to the session",
        description: "Unpin it first to close the tab.",
        type: "info",
      });
      return;
    }
  }

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

/// Replace the `path` field on a tab's `data` blob so a viewer can swap which
/// file it shows without remounting via close+open. Used by DiffView's file
/// prev/next chevrons (and Ctrl+Left/Right) to navigate within
/// `activeSessionFilesAtom` while keeping the tab — and its diff state —
/// in place.
export const setTabPathAction = atom(null, (get, set, params: { tabId: string; path: string }) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) return;
  set(tabStateMapAtom, (prev) => {
    const state = prev[sessionId] ?? defaultTabState;
    const tabs = state.tabs.map((t) =>
      t.id === params.tabId
        ? { ...t, data: { ...(t.data ?? {}), path: params.path } }
        : t,
    );
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

interface FileBrowserState {
  rootEntries: { name: string; is_dir: boolean; path: string }[];
  expanded: string[];
  children: Record<string, { name: string; is_dir: boolean; path: string }[]>;
  lastOpened: string | null;
}

/// Module-level so reopening the file picker (or its mount-unmount cycle as
/// the overlay opens/closes) doesn't drop expanded dirs, cached listings, or
/// the last-opened file. Keyed per session.
export const fileBrowserStateMapAtom = atom<Record<string, FileBrowserState>>({});

/// Persists the active sub-tab (pill) per spec change across tab switches.
export const specSubTabMapAtom = atom<Record<string, string>>({});


/// Returns the category of the currently active panel/tab, or null if no panel is open.
/// Used by the layout preset engine to determine target proportions.
export const activePanelCategoryAtom = atom<PanelCategory | null>((get) => {
  const activeTab = get(activeTabAtom);
  if (activeTab) return activeTab.category;
  const panelView = get(activePanelViewAtom);
  if (panelView) return PANEL_CATEGORY_MAP[panelView];
  return null;
});
