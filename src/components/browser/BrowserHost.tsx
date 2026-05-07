import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useAtomValue, useSetAtom } from "jotai";
import { invoke, listen } from "@/lib/tauri";
import { activePanelViewAtom, activeTabAtom } from "@/stores/rightPanel";
import { activeSessionIdAtom } from "@/stores/workspace";
import {
  browserCloseTabAction,
  browserCycleTabAction,
  browserHardReloadAction,
  browserModeForSessionAtom,
  browserNewTabAction,
  browserReloadAction,
  browserSessionForActiveAtom,
  browserToggleModeAction,
  type BrowserSlotBbox,
} from "@/stores/browser";
import { BrowserPanel } from "./BrowserPanel";

export const DOCK_SLOT_ATTR = "data-browser-dock-slot";
export const FLOATING_SLOT_ATTR = "data-browser-floating-slot";

/// Singleton mount point for the BrowserPanel. Lives in `Workspace`, mounts
/// once, and positions itself as `position:fixed` over whichever slot is
/// active for the current mode. The iframe inside is NEVER DOM-moved
/// (which would force a reload per HTML spec) — it just becomes invisible
/// when no slot is active. SPA state survives mode switches and tab
/// switches.
///
/// Polling is host-driven: we resolve the active slot via DOM attribute
/// each frame, query its bbox, and update style. This is race-free across
/// dock↔floating transitions — even when both slots briefly coexist (e.g.
/// during FloatingPanel's 100ms close animation), the host always picks
/// the slot whose marker matches the current mode.
///
/// Portaled to `document.body` so the fixed positioning is guaranteed to
/// be viewport-relative regardless of any `transform`/`will-change`
/// ancestor in the workspace tree.
export function BrowserHost() {
  const mode = useAtomValue(browserModeForSessionAtom);
  const activeTab = useAtomValue(activeTabAtom);
  const activePanelView = useAtomValue(activePanelViewAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const newTab = useSetAtom(browserNewTabAction);
  const closeTab = useSetAtom(browserCloseTabAction);
  const toggleMode = useSetAtom(browserToggleModeAction);
  const cycleTab = useSetAtom(browserCycleTabAction);
  const reload = useSetAtom(browserReloadAction);
  const hardReload = useSetAtom(browserHardReloadAction);
  const sessionState = useAtomValue(browserSessionForActiveAtom);
  const browserOpenInDock =
    activeTab?.type === "browser" || activePanelView === "browser";
  const shouldShow = mode === "floating" || browserOpenInDock;

  const [bbox, setBbox] = useState<BrowserSlotBbox | null>(null);
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  const wasShownRef = useRef(false);

  useEffect(() => {
    if (!shouldShow) {
      setBbox(null);
      return;
    }
    const selector =
      mode === "floating" ? `[${FLOATING_SLOT_ATTR}]` : `[${DOCK_SLOT_ATTR}]`;
    let raf = 0;
    let last: BrowserSlotBbox | null = null;

    function tick() {
      const el = document.querySelector(selector) as HTMLElement | null;
      if (el) {
        const r = el.getBoundingClientRect();
        if (r.width > 0 && r.height > 0) {
          const next: BrowserSlotBbox = {
            x: Math.round(r.left),
            y: Math.round(r.top),
            w: Math.round(r.width),
            h: Math.round(r.height),
          };
          if (
            !last ||
            last.x !== next.x ||
            last.y !== next.y ||
            last.w !== next.w ||
            last.h !== next.h
          ) {
            last = next;
            setBbox(next);
          }
        }
      }
      raf = requestAnimationFrame(tick);
    }
    tick();
    return () => {
      if (raf) cancelAnimationFrame(raf);
    };
  }, [shouldShow, mode]);

  const visible = shouldShow && bbox != null;

  // Auto-focus the panel content when the browser becomes VISIBLE (not
  // just shouldShow — shouldShow flips before bbox is set, while the
  // wrapper is still display:none, and `focus()` is a no-op on
  // display:none elements). Listened for in BlankHomepage.
  useEffect(() => {
    if (visible && !wasShownRef.current) {
      const id = requestAnimationFrame(() => {
        document.dispatchEvent(new CustomEvent("cluihud:browser-became-visible"));
      });
      wasShownRef.current = true;
      return () => cancelAnimationFrame(id);
    }
    if (!visible) wasShownRef.current = false;
  }, [visible]);

  // Reserved shortcuts (Ctrl+T, Ctrl+W, Ctrl+Shift+0) live as Tauri OS-
  // level globals while the browser panel is visible. The runtime
  // intercepts BEFORE the cross-origin iframe sees the keystroke — so
  // typing inside iframe inputs keeps working (non-reserved keys pass
  // through normally) while these specific chords always reach our
  // panel. See src-tauri/src/browser.rs for the registration handlers
  // and the corresponding event payloads.
  useEffect(() => {
    if (!visible) return;
    void invoke("browser_register_shortcuts").catch(() => {});
    return () => {
      void invoke("browser_unregister_shortcuts").catch(() => {});
    };
  }, [visible]);

  // Dispatch reserved-shortcut events to the matching atom action. Stays
  // mounted across visibility flips so we don't miss events emitted while
  // a register/unregister roundtrip is in flight.
  useEffect(() => {
    let unlistenFn: (() => void) | null = null;
    void listen<string>("browser:intercepted-shortcut", (payload) => {
      if (!sessionId) return;
      switch (payload) {
        case "browser:new-tab":
          void newTab({ sessionId });
          return;
        case "browser:close-tab": {
          const active = sessionState.activeTabId;
          if (active) closeTab({ sessionId, tabId: active });
          return;
        }
        case "browser:toggle-mode":
          toggleMode(sessionId);
          return;
        case "browser:next-tab":
          cycleTab({ sessionId, direction: 1 });
          return;
        case "browser:prev-tab":
          cycleTab({ sessionId, direction: -1 });
          return;
        case "browser:reload": {
          const active = sessionState.activeTabId;
          if (active) reload({ sessionId, tabId: active });
          return;
        }
        case "browser:hard-reload": {
          const active = sessionState.activeTabId;
          if (active) hardReload({ sessionId, tabId: active });
          return;
        }
      }
    }).then((fn) => {
      unlistenFn = fn;
    });
    return () => {
      if (unlistenFn) unlistenFn();
    };
  }, [
    sessionId,
    sessionState.activeTabId,
    newTab,
    closeTab,
    toggleMode,
    cycleTab,
    reload,
    hardReload,
  ]);

  if (typeof document === "undefined") return null;

  return createPortal(
    <div
      ref={wrapperRef}
      data-browser-host=""
      tabIndex={-1}
      style={{
        position: "fixed",
        top: bbox?.y ?? 0,
        left: bbox?.x ?? 0,
        width: bbox?.w ?? 0,
        height: bbox?.h ?? 0,
        display: visible ? undefined : "none",
        zIndex: 40,
        outline: "none",
      }}
    >
      <BrowserPanel />
    </div>,
    document.body,
  );
}
