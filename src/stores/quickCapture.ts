import { atom } from "jotai";
import { invoke } from "@/lib/tauri";
import { appStore } from "./jotaiStore";
import type { FloatingGeometry } from "@/components/floating/FloatingPanel";

export const QUICK_CAPTURE_PANEL_ID = "quick-capture";

const DEFAULT_GEOMETRY: FloatingGeometry = {
  x: typeof window !== "undefined" ? Math.max(20, window.innerWidth / 2 - 240) : 480,
  y: typeof window !== "undefined" ? Math.max(20, window.innerHeight / 4) : 120,
  width: 480,
  height: 220,
};

const DEFAULT_OPACITY = 0.96;

export const quickCaptureOpenAtom = atom(false);
export const quickCaptureGeometryAtom = atom<FloatingGeometry>(DEFAULT_GEOMETRY);
export const quickCaptureOpacityAtom = atom<number>(DEFAULT_OPACITY);

// Re-uses the generic floating_panel_geometry SQLite row via the scratchpad-
// prefixed Tauri commands (they're keyed by panel_id and the prefix is a
// historical naming artifact). Rename surgery is deferred — for now we just
// pass our own panel id.
export async function loadQuickCaptureGeometry(): Promise<void> {
  try {
    const row = await invoke<{ geometry_json: string; opacity: number } | null>(
      "scratchpad_get_geometry",
      { panelId: QUICK_CAPTURE_PANEL_ID },
    );
    if (!row) return;
    try {
      const parsed = JSON.parse(row.geometry_json) as FloatingGeometry;
      appStore.set(quickCaptureGeometryAtom, clampToViewport(parsed));
    } catch {
      appStore.set(quickCaptureGeometryAtom, DEFAULT_GEOMETRY);
    }
    appStore.set(quickCaptureOpacityAtom, row.opacity);
  } catch (err) {
    console.warn("[quickCapture] geometry load failed:", err);
  }
}

export async function saveQuickCaptureGeometry(
  geometry: FloatingGeometry,
  opacity: number,
): Promise<void> {
  try {
    await invoke("scratchpad_set_geometry", {
      panelId: QUICK_CAPTURE_PANEL_ID,
      geometryJson: JSON.stringify(geometry),
      opacity,
    });
  } catch (err) {
    console.warn("[quickCapture] geometry save failed:", err);
  }
}

export function clampToViewport(g: FloatingGeometry): FloatingGeometry {
  if (typeof window === "undefined") return g;
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const width = Math.min(g.width, Math.max(320, vw - 40));
  const height = Math.min(g.height, Math.max(160, vh - 40));
  const x = Math.min(Math.max(0, g.x), Math.max(0, vw - width));
  const y = Math.min(Math.max(0, g.y), Math.max(0, vh - height));
  return { x, y, width, height };
}
