import { useEffect, useRef } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { atom } from "jotai";
import { invoke } from "@/lib/tauri";
import { FloatingPanel, type FloatingGeometry } from "@/components/floating/FloatingPanel";
import { activeSessionIdAtom } from "@/stores/workspace";
import {
  browserModeForSessionAtom,
  browserSetModeAction,
} from "@/stores/browser";
import { FLOATING_SLOT_ATTR } from "./BrowserHost";

/// `browser-v4` invalidates older saved geometries (default sizes from
/// v1/v2/v3 may have been small enough to feel cramped). v4 ships with
/// 92% × 90% viewport without caps.
const BROWSER_PANEL_ID = "browser-v4";

/// Compute the default geometry on demand (not at module load) so
/// `window.innerWidth/Height` reflect the actual cluihud window size at
/// the moment the floating browser is first opened.
function computeDefaultGeometry(): FloatingGeometry {
  const vw = typeof window !== "undefined" ? window.innerWidth : 1920;
  const vh = typeof window !== "undefined" ? window.innerHeight : 1080;
  const width = Math.round(vw * 0.92);
  const height = Math.round(vh * 0.9);
  return {
    x: Math.max(20, Math.round((vw - width) / 2)),
    y: Math.max(20, Math.round((vh - height) / 2)),
    width,
    height,
  };
}

/// Held as nullable so `computeDefaultGeometry()` only runs once we have
/// a real viewport (post-mount, not at module init when window dimensions
/// can be 0 in some Tauri startup orderings).
const browserGeometryAtom = atom<FloatingGeometry | null>(null);

let geometryLoaded = false;
async function loadGeometryOnce(set: (g: FloatingGeometry) => void) {
  if (geometryLoaded) return;
  geometryLoaded = true;
  try {
    const row = await invoke<{ geometry_json: string; opacity: number } | null>(
      "scratchpad_get_geometry",
      { panelId: BROWSER_PANEL_ID },
    );
    if (row) {
      const parsed = JSON.parse(row.geometry_json) as FloatingGeometry;
      set(clampToViewport(parsed));
    }
  } catch {
    /* persist failure → keep default geometry, not user-facing */
  }
}

function clampToViewport(g: FloatingGeometry): FloatingGeometry {
  const vw = typeof window !== "undefined" ? window.innerWidth : 1920;
  const vh = typeof window !== "undefined" ? window.innerHeight : 1080;
  return {
    x: Math.max(0, Math.min(g.x, vw - 200)),
    y: Math.max(0, Math.min(g.y, vh - 100)),
    width: Math.max(360, Math.min(g.width, vw)),
    height: Math.max(240, Math.min(g.height, vh)),
  };
}

async function persistGeometry(g: FloatingGeometry) {
  try {
    await invoke("scratchpad_set_geometry", {
      panelId: BROWSER_PANEL_ID,
      geometryJson: JSON.stringify(g),
      opacity: 1.0,
    });
  } catch {
    /* persist failure → silent, user state lives in atom for the session */
  }
}

/// Floating-mode chrome for the browser. Renders only the FloatingPanel
/// chrome plus an empty slot div — the BrowserPanel itself is mounted once
/// in Workspace via BrowserHost and overlaid on this slot's bbox.
export function BrowserFloating() {
  const sessionId = useAtomValue(activeSessionIdAtom);
  const mode = useAtomValue(browserModeForSessionAtom);
  const setMode = useSetAtom(browserSetModeAction);
  const [geometry, setGeometry] = useAtom(browserGeometryAtom);
  const initRef = useRef(false);

  useEffect(() => {
    if (initRef.current) return;
    initRef.current = true;
    // Set default first so the panel always has geometry at first paint,
    // then async-load the saved row (which overrides if present).
    setGeometry(computeDefaultGeometry());
    void loadGeometryOnce(setGeometry);
  }, [setGeometry]);

  useEffect(() => {
    function reclamp() {
      setGeometry((prev) => (prev ? clampToViewport(prev) : prev));
    }
    window.addEventListener("resize", reclamp);
    return () => window.removeEventListener("resize", reclamp);
  }, [setGeometry]);

  const open = mode === "floating" && sessionId != null && geometry != null;

  if (!geometry) return null;

  return (
    <FloatingPanel
      panelId={BROWSER_PANEL_ID}
      open={open}
      onClose={() => sessionId && setMode({ sessionId, mode: "dock" })}
      geometry={geometry}
      onGeometryChange={(next) => {
        setGeometry(next);
        void persistGeometry(next);
      }}
      opacity={1.0}
      title={<span className="font-medium">Browser</span>}
    >
      {/* Inset 12px so FloatingPanel's corner resize handles stay
          uncovered by BrowserHost's iframe overlay. The data attribute
          lives on the INNER div (whose bbox is the inset area) — putting
          it on the outer wrapper would publish the full children bbox
          and the iframe would still cover the resize hit-zones. */}
      <div className="relative h-full w-full" style={{ padding: 12 }}>
        <div
          className="h-full w-full"
          {...{ [FLOATING_SLOT_ATTR]: "" }}
        />
      </div>
    </FloatingPanel>
  );
}
