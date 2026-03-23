import { atom } from "jotai";
import { activePanelCategoryAtom, type PanelCategory } from "./rightPanel";
import { activeSessionIdAtom } from "./workspace";

export type LayoutPreset = "terminal-focus" | "doc-review" | "tool-workspace";

export interface PresetSizes {
  sidebar: number;
  center: number;
  right: number;
  sidebarAutoCollapse: boolean;
}

export const PRESET_SIZES: Record<LayoutPreset, PresetSizes> = {
  "terminal-focus": {
    sidebar: 15,
    center: 85,
    right: 0,
    sidebarAutoCollapse: false,
  },
  "doc-review": {
    sidebar: 15,
    center: 50,
    right: 35,
    sidebarAutoCollapse: false,
  },
  "tool-workspace": {
    sidebar: 0,
    center: 30,
    right: 55,
    sidebarAutoCollapse: true,
  },
};

const CATEGORY_TO_PRESET: Record<PanelCategory, LayoutPreset> = {
  document: "doc-review",
  tool: "tool-workspace",
};

/// Derives the target layout preset from the active panel category.
export const layoutPresetAtom = atom<LayoutPreset>((get) => {
  const category = get(activePanelCategoryAtom);
  if (!category) return "terminal-focus";
  return CATEGORY_TO_PRESET[category];
});

/// Per-session layout preset persistence.
/// Stores the last active preset for each session so switching sessions restores layout.
export const sessionLayoutMapAtom = atom<Record<string, LayoutPreset>>({});

export const sessionLayoutPresetAtom = atom(
  (get): LayoutPreset => {
    const sessionId = get(activeSessionIdAtom);
    if (!sessionId) return "terminal-focus";
    return get(sessionLayoutMapAtom)[sessionId] ?? "terminal-focus";
  },
  (get, set, preset: LayoutPreset) => {
    const sessionId = get(activeSessionIdAtom);
    if (!sessionId) return;
    set(sessionLayoutMapAtom, (prev) => ({ ...prev, [sessionId]: preset }));
  },
);

/// Signals Workspace to transition. Incremented when preset changes should be applied.
export const applyPresetSignalAtom = atom(0);

/// Tracks whether a resize drag is in progress (disables CSS transitions).
export const isDraggingAtom = atom(false);
