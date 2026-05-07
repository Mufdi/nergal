import { useEffect, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { FloatingPanel } from "@/components/floating/FloatingPanel";
import { ScratchpadTabBar } from "./ScratchpadTabBar";
import { ScratchpadEditor } from "./ScratchpadEditor";
import {
  bootstrapScratchpad,
  clampGeometryToViewport,
  createNewScratchTab,
  saveScratchpadGeometry,
  SCRATCHPAD_PANEL_ID,
  scratchpadActiveTabIdAtom,
  scratchpadFocusSignalAtom,
  scratchpadGeometryAtom,
  scratchpadOpacityAtom,
  scratchpadOpenAtom,
  scratchpadTabsAtom,
} from "@/stores/scratchpad";

let bootstrapped = false;

export function ScratchpadPanel() {
  const open = useAtomValue(scratchpadOpenAtom);
  const setOpen = useSetAtom(scratchpadOpenAtom);
  const tabs = useAtomValue(scratchpadTabsAtom);
  const activeTabId = useAtomValue(scratchpadActiveTabIdAtom);
  const setActiveTabId = useSetAtom(scratchpadActiveTabIdAtom);
  const geometry = useAtomValue(scratchpadGeometryAtom);
  const setGeometry = useSetAtom(scratchpadGeometryAtom);
  const opacity = useAtomValue(scratchpadOpacityAtom);
  const setFocusSignal = useSetAtom(scratchpadFocusSignalAtom);
  const lastOpenRef = useRef(false);

  // One-shot bootstrap: load tabs + geometry + subscribe watcher events.
  // Idempotent so HMR doesn't double-register.
  useEffect(() => {
    if (bootstrapped) return;
    bootstrapped = true;
    void bootstrapScratchpad();
  }, []);

  // On open: ensure there's at least one tab (auto-create) and signal the
  // editor to grab focus. Avoids the "user opens panel, has to click + or
  // a tab before typing" friction.
  useEffect(() => {
    if (!open) {
      lastOpenRef.current = false;
      return;
    }
    const justOpened = !lastOpenRef.current;
    lastOpenRef.current = true;
    if (tabs.length === 0) {
      void createNewScratchTab();
    } else if (!activeTabId) {
      setActiveTabId(tabs[0].tab_id);
    }
    if (justOpened) {
      // Bump the focus signal on the next tick so the editor's effect
      // (subscribed to active tab + signal) lands after layout settles.
      requestAnimationFrame(() => setFocusSignal((p) => p + 1));
    }
  }, [open, tabs, activeTabId, setActiveTabId, setFocusSignal]);

  // Re-clamp geometry on viewport changes (display reconnect, window resize).
  useEffect(() => {
    function reclamp() {
      setGeometry((prev) => clampGeometryToViewport(prev));
    }
    window.addEventListener("resize", reclamp);
    return () => window.removeEventListener("resize", reclamp);
  }, [setGeometry]);

  return (
    <FloatingPanel
      panelId={SCRATCHPAD_PANEL_ID}
      open={open}
      onClose={() => setOpen(false)}
      geometry={geometry}
      onGeometryChange={(next) => {
        setGeometry(next);
        void saveScratchpadGeometry(next, opacity);
      }}
      opacity={opacity}
      zIndex={55}
      title={
        <>
          <span className="font-medium">Scratchpad</span>
          <span className="text-muted-foreground tabular-nums">{tabs.length}</span>
        </>
      }
    >
      <div className="flex h-full flex-col">
        <ScratchpadTabBar />
        <div className="flex-1 min-h-0">
          {activeTabId ? (
            <ScratchpadEditor tabId={activeTabId} />
          ) : (
            <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
              <button
                type="button"
                onClick={() => void createNewScratchTab()}
                className="rounded-md border border-border bg-card px-3 py-1.5 hover:text-foreground"
              >
                No notes yet — press +
              </button>
            </div>
          )}
        </div>
      </div>
    </FloatingPanel>
  );
}
