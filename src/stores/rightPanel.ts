import { atom } from "jotai";

export interface RightPanelTab {
  id: string;
  type: "plan" | "diff" | "spec" | "git" | "transcript";
  label: string;
  filePath?: string;
  specPath?: string;
  sessionId?: string;
}

export const expandRightPanelAtom = atom(0);
export const openTabsAtom = atom<RightPanelTab[]>([]);
export const activeTabIdAtom = atom<string | null>(null);

export const activeTabAtom = atom<RightPanelTab | null>((get) => {
  const tabs = get(openTabsAtom);
  const activeId = get(activeTabIdAtom);
  if (!activeId) return null;
  return tabs.find((t) => t.id === activeId) ?? null;
});

export const openTabAtom = atom(null, (_get, set, tab: RightPanelTab) => {
  set(openTabsAtom, (prev) => {
    const exists = prev.find((t) => t.id === tab.id);
    if (exists) return prev;
    return [...prev, tab];
  });
  set(activeTabIdAtom, tab.id);
});

export const closeTabAtom = atom(null, (get, set, tabId: string) => {
  const prev = get(openTabsAtom);
  const index = prev.findIndex((t) => t.id === tabId);
  if (index === -1) return;

  const next = prev.filter((t) => t.id !== tabId);
  set(openTabsAtom, next);

  if (get(activeTabIdAtom) === tabId) {
    const newActive = next[Math.min(index, next.length - 1)] ?? null;
    set(activeTabIdAtom, newActive?.id ?? null);
  }
});
