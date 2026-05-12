import { atom } from "jotai";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@/lib/tauri";
import { appStore } from "./jotaiStore";
import { toastsAtom } from "./toast";

/// Backend payload returned by `scratchpad_list_tabs`.
export interface ScratchTab {
  tab_id: string;
  position: number;
  last_modified_ms: number;
}

/// Geometry persisted per floating panel. Coordinates in CSS pixels relative
/// to the viewport at the time of save.
export interface FloatingGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
}

export const SCRATCHPAD_PANEL_ID = "scratchpad";
export const DEFAULT_GEOMETRY: FloatingGeometry = {
  x: 200,
  y: 150,
  width: 520,
  height: 420,
};
export const DEFAULT_OPACITY = 0.9;
/// Soft cap; UI shows a warning badge when reached. The backend imposes no
/// limit — this is purely for tab-bar legibility.
export const MAX_TABS_SOFT = 50;

export const scratchpadOpenAtom = atom(false);
export const scratchpadTabsAtom = atom<ScratchTab[]>([]);
export const scratchpadActiveTabIdAtom = atom<string | null>(null);

/// Per-tab content buffer. Source of truth while editing; flushed to disk by
/// the autosave hook. Backend is the source of truth at rest.
export const scratchpadContentAtom = atom<Record<string, string>>({});

/// Per-tab dirty flag — true when the in-memory buffer differs from what was
/// last written. Used by the watcher reconciliation to suppress external
/// reloads on dirty tabs (soft conflict marker instead).
export const scratchpadDirtyAtom = atom<Record<string, boolean>>({});

/// Per-tab "external change pending" badge. Set when a watcher event with a
/// hash NOT in the own-write ring buffer arrives and the buffer is dirty.
/// User must resolve manually (reload from disk loses unsaved edits).
export const scratchpadConflictAtom = atom<Record<string, boolean>>({});

export const scratchpadGeometryAtom = atom<FloatingGeometry>(DEFAULT_GEOMETRY);
export const scratchpadOpacityAtom = atom<number>(DEFAULT_OPACITY);

/// Currently configured scratchpad directory (canonicalized by the backend).
export const scratchpadPathAtom = atom<string>("");

/// Selection text inside the active scratchpad editor. Updated on every
/// CodeMirror selection change. Exposed for future send-to-prompt adapter
/// (composes with `activeSessionIdAtom` from outside this store).
export const currentScratchpadSelectionAtom = atom<string>("");

/// In-memory stack of recently soft-deleted tab ids in this session.
/// Drives Ctrl+Shift+T restore. Files still live in `.trash/` until purge.
export const closedScratchTabsAtom = atom<string[]>([]);

/// Bumped each time the scratchpad panel becomes visible, so the editor
/// can re-focus reliably even when the active tab id hasn't changed.
export const scratchpadFocusSignalAtom = atom(0);

/// Per-tab caret offset (in document chars) captured at editor unmount.
/// In-memory only — lost on app restart. Falls back to doc end when missing
/// so a fresh open lands the cursor at the bottom of the note instead of
/// jumping to row 0.
export const scratchpadCursorAtom = atom<Record<string, number>>({});

function pushToast(kind: "info" | "error" | "success", message: string) {
  appStore.set(toastsAtom, { type: kind, message });
}

/// Compute display name "Scratch N" from position. Pure function of the
/// in-memory tab list at render time; no persistence.
export function displayNameFor(tab: ScratchTab): string {
  return `Scratch ${tab.position + 1}`;
}

/// Refresh the tab list from disk via Tauri. Resolves the active tab id if
/// the previous one was deleted externally.
export async function reloadTabsFromBackend(): Promise<void> {
  try {
    const tabs = await invoke<ScratchTab[]>("scratchpad_list_tabs");
    appStore.set(scratchpadTabsAtom, tabs);
    const currentActive = appStore.get(scratchpadActiveTabIdAtom);
    const stillExists = currentActive && tabs.some((t) => t.tab_id === currentActive);
    if (!stillExists) {
      appStore.set(scratchpadActiveTabIdAtom, tabs[0]?.tab_id ?? null);
    }
  } catch (err) {
    console.error("[scratchpad] list failed:", err);
  }
}

/// Create a new tab on disk and make it active. Returns the new tab_id.
export async function createNewScratchTab(): Promise<string | null> {
  try {
    const tabId = await invoke<string>("scratchpad_create_tab");
    await reloadTabsFromBackend();
    appStore.set(scratchpadActiveTabIdAtom, tabId);
    appStore.set(scratchpadContentAtom, (prev) => ({ ...prev, [tabId]: "" }));
    return tabId;
  } catch (err) {
    pushToast("error", `Failed to create scratchpad tab: ${err}`);
    return null;
  }
}

/// Soft-delete a tab (moves to .trash/). Adjusts active tab if needed.
/// Pushes the tab_id onto the closed-stack so Ctrl+Shift+T can restore it.
export async function closeScratchTab(tabId: string): Promise<void> {
  try {
    await invoke("scratchpad_close_tab", { tabId });
    appStore.set(scratchpadContentAtom, (prev) => {
      const next = { ...prev };
      delete next[tabId];
      return next;
    });
    appStore.set(scratchpadDirtyAtom, (prev) => {
      const next = { ...prev };
      delete next[tabId];
      return next;
    });
    appStore.set(scratchpadConflictAtom, (prev) => {
      const next = { ...prev };
      delete next[tabId];
      return next;
    });
    appStore.set(closedScratchTabsAtom, (prev) => [...prev, tabId]);
    await reloadTabsFromBackend();
  } catch (err) {
    pushToast("error", `Failed to close scratchpad tab: ${err}`);
  }
}

/// Restore the most recently soft-deleted tab. No-op if the stack is empty
/// or if the trash file is gone. Returns the restored tab_id.
export async function restoreLastClosedScratchTab(): Promise<string | null> {
  const stack = appStore.get(closedScratchTabsAtom);
  if (stack.length === 0) return null;
  const tabId = stack[stack.length - 1];
  appStore.set(closedScratchTabsAtom, (prev) => prev.slice(0, -1));
  try {
    const ok = await invoke<boolean>("scratchpad_restore_tab", { tabId });
    if (!ok) return null;
    await reloadTabsFromBackend();
    appStore.set(scratchpadActiveTabIdAtom, tabId);
    return tabId;
  } catch (err) {
    pushToast("error", `Failed to restore scratchpad tab: ${err}`);
    return null;
  }
}

/// Cycle to the next/previous tab. Wraps around. No-op with <2 tabs.
export function cycleScratchTab(direction: 1 | -1): void {
  const tabs = appStore.get(scratchpadTabsAtom);
  if (tabs.length < 2) return;
  const activeId = appStore.get(scratchpadActiveTabIdAtom);
  const idx = Math.max(0, tabs.findIndex((t) => t.tab_id === activeId));
  const nextIdx = (idx + direction + tabs.length) % tabs.length;
  appStore.set(scratchpadActiveTabIdAtom, tabs[nextIdx].tab_id);
}

/// Read a tab's content from disk into the in-memory buffer if not already
/// loaded.
export async function loadTabContentIfNeeded(tabId: string): Promise<void> {
  const buffers = appStore.get(scratchpadContentAtom);
  if (Object.prototype.hasOwnProperty.call(buffers, tabId)) return;
  try {
    const content = await invoke<string>("scratchpad_read_tab", { tabId });
    appStore.set(scratchpadContentAtom, (prev) => ({ ...prev, [tabId]: content }));
  } catch (err) {
    console.error("[scratchpad] read failed:", err);
  }
}

/// Write a tab's content back to disk.
export async function persistTabContent(tabId: string, content: string): Promise<void> {
  try {
    await invoke("scratchpad_write_tab", { tabId, content });
    appStore.set(scratchpadContentAtom, (prev) => ({ ...prev, [tabId]: content }));
    appStore.set(scratchpadDirtyAtom, (prev) => ({ ...prev, [tabId]: false }));
  } catch (err) {
    pushToast("error", `Autosave failed: ${err}`);
  }
}

/// Load persisted geometry + opacity from SQLite (via Tauri). Falls back to
/// defaults if no row exists. Clamps geometry to the current viewport so a
/// disconnected monitor cannot leave the panel unreachable.
export async function loadScratchpadGeometry(): Promise<void> {
  try {
    const row = await invoke<{ geometry_json: string; opacity: number } | null>(
      "scratchpad_get_geometry",
      { panelId: SCRATCHPAD_PANEL_ID },
    );
    if (row) {
      try {
        const parsed = JSON.parse(row.geometry_json) as FloatingGeometry;
        appStore.set(scratchpadGeometryAtom, clampGeometryToViewport(parsed));
      } catch {
        appStore.set(scratchpadGeometryAtom, DEFAULT_GEOMETRY);
      }
      appStore.set(scratchpadOpacityAtom, row.opacity);
    }
  } catch (err) {
    console.error("[scratchpad] geometry load failed:", err);
  }
}

/// Persist current geometry + opacity to SQLite.
export async function saveScratchpadGeometry(geometry: FloatingGeometry, opacity: number): Promise<void> {
  try {
    await invoke("scratchpad_set_geometry", {
      panelId: SCRATCHPAD_PANEL_ID,
      geometryJson: JSON.stringify(geometry),
      opacity,
    });
  } catch (err) {
    console.error("[scratchpad] geometry save failed:", err);
  }
}

/// Clamp a geometry to the current viewport. Used on load and on
/// path-restore. If the persisted coords would push the panel partially or
/// fully off-screen (multi-monitor disconnect, scaling change), reset to
/// centered defaults.
export function clampGeometryToViewport(g: FloatingGeometry): FloatingGeometry {
  const vw = typeof window === "undefined" ? 1280 : window.innerWidth;
  const vh = typeof window === "undefined" ? 800 : window.innerHeight;
  const minMargin = 24;
  const tooWide = g.width > vw - minMargin * 2;
  const tooTall = g.height > vh - minMargin * 2;
  if (tooWide || tooTall || g.x < 0 || g.y < 0 || g.x + 64 > vw || g.y + 32 > vh) {
    const w = Math.min(DEFAULT_GEOMETRY.width, vw - minMargin * 2);
    const h = Math.min(DEFAULT_GEOMETRY.height, vh - minMargin * 2);
    return {
      x: Math.max(minMargin, Math.floor((vw - w) / 2)),
      y: Math.max(minMargin, Math.floor((vh - h) / 3)),
      width: w,
      height: h,
    };
  }
  return g;
}

/// Bootstrap: load tabs + geometry + path; subscribe to backend events.
/// Returns an unsubscribe function for HMR cleanup. Idempotent — calling it
/// again replaces the previous listeners.
let unsubscribers: Array<() => void> = [];

export async function bootstrapScratchpad(): Promise<void> {
  for (const u of unsubscribers) {
    try { u(); } catch {}
  }
  unsubscribers = [];

  try {
    const path = await invoke<string>("scratchpad_get_path");
    appStore.set(scratchpadPathAtom, path);
  } catch (err) {
    console.error("[scratchpad] get_path failed:", err);
  }

  await loadScratchpadGeometry();
  await reloadTabsFromBackend();

  const offChanged = await listen<{ tab_id: string; hash_hex: string }>(
    "scratchpad:tab-changed",
    async (event) => {
      const { tab_id } = event.payload;
      const dirty = appStore.get(scratchpadDirtyAtom)[tab_id] ?? false;
      if (dirty) {
        appStore.set(scratchpadConflictAtom, (prev) => ({ ...prev, [tab_id]: true }));
        return;
      }
      try {
        const content = await invoke<string>("scratchpad_read_tab", { tabId: tab_id });
        appStore.set(scratchpadContentAtom, (prev) => ({ ...prev, [tab_id]: content }));
      } catch (err) {
        console.error("[scratchpad] reload after external change failed:", err);
      }
    },
  );
  unsubscribers.push(offChanged);

  const offDeleted = await listen<{ tab_id: string }>(
    "scratchpad:tab-deleted",
    async (event) => {
      const { tab_id } = event.payload;
      appStore.set(scratchpadContentAtom, (prev) => {
        const next = { ...prev };
        delete next[tab_id];
        return next;
      });
      appStore.set(scratchpadDirtyAtom, (prev) => {
        const next = { ...prev };
        delete next[tab_id];
        return next;
      });
      await reloadTabsFromBackend();
    },
  );
  unsubscribers.push(offDeleted);

  const offDirMissing = await listen("scratchpad:dir-missing", async () => {
    pushToast("info", "Scratchpad directory was removed externally — recreating.");
    try {
      const currentPath = appStore.get(scratchpadPathAtom);
      if (currentPath) {
        await invoke("scratchpad_set_path", { newPath: currentPath });
        await reloadTabsFromBackend();
      }
    } catch (err) {
      pushToast("error", `Failed to recreate scratchpad dir: ${err}`);
    }
  });
  unsubscribers.push(offDirMissing);
}
