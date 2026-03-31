import { useState, useEffect, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { TopBar } from "./TopBar";
import { Sidebar } from "./Sidebar";
import { RightPanel } from "./RightPanel";
import { StatusBar } from "./StatusBar";
import { Toaster } from "sileo";
import { TerminalManager } from "@/components/terminal/TerminalManager";
import { SettingsPanel } from "@/components/settings/SettingsPanel";
import { ActivityDrawer } from "@/components/activity/ActivityDrawer";
import { ZenMode } from "@/components/zen/ZenMode";
import { expandRightPanelAtom, activePanelViewAtom, activeTabAtom } from "@/stores/rightPanel";
import { activeSessionIdAtom } from "@/stores/workspace";
import { toggleSidebarAtom, toggleRightPanelAtom } from "@/stores/shortcuts";
import { layoutPresetAtom, PRESET_SIZES, sessionLayoutPresetAtom, type LayoutPreset } from "@/stores/layout";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { CommandPalette } from "@/components/command/CommandPalette";
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/resizable";
import { usePanelRef } from "react-resizable-panels";

const COLLAPSED_SIZE_PX = 28;

export function Workspace() {
  useKeyboardShortcuts();

  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [rightCollapsed, setRightCollapsed] = useState(true);
  const expandSignal = useAtomValue(expandRightPanelAtom);
  const sidebarToggle = useAtomValue(toggleSidebarAtom);
  const rightToggle = useAtomValue(toggleRightPanelAtom);
  const layoutPreset = useAtomValue(layoutPresetAtom);
  const setSessionPreset = useSetAtom(sessionLayoutPresetAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);

  const sidebarPanelRef = usePanelRef();
  const centerPanelRef = usePanelRef();
  const rightPanelRef = usePanelRef();

  const prevPresetRef = useRef<LayoutPreset | null>(null);
  const sidebarAutoCollapsedRef = useRef(false);

  // Force re-apply when session changes (the new session may have the same preset name but different panel state)
  const prevSessionRef = useRef<string | null>(null);

  const setActivePanelView = useSetAtom(activePanelViewAtom);
  const activeTab = useAtomValue(activeTabAtom);

  // Redistribute space when sidebar collapses/expands manually
  const prevSidebarCollapsed = useRef(sidebarCollapsed);
  useEffect(() => {
    if (prevSidebarCollapsed.current === sidebarCollapsed) return;
    prevSidebarCollapsed.current = sidebarCollapsed;

    const center = centerPanelRef.current;
    const right = rightPanelRef.current;
    if (!center || !right || right.isCollapsed()) return;

    requestAnimationFrame(() => {
      if (sidebarCollapsed) {
        center.resize("50%");
        right.resize("50%");
      } else {
        const sizes = PRESET_SIZES[layoutPreset];
        center.resize(`${sizes.center}%`);
        right.resize(`${sizes.right}%`);
      }
    });
  }, [sidebarCollapsed, layoutPreset]);

  // Clear global panel view on session switch if new session has no tabs
  useEffect(() => {
    if (prevSessionRef.current !== null && prevSessionRef.current !== activeSessionId) {
      if (!activeTab) {
        setActivePanelView(null);
      }
    }
  }, [activeSessionId, activeTab, setActivePanelView]);

  // Apply layout preset when it changes or session switches
  useEffect(() => {
    const sessionChanged = prevSessionRef.current !== activeSessionId;
    prevSessionRef.current = activeSessionId ?? null;

    if (!sessionChanged && prevPresetRef.current === layoutPreset) return;
    prevPresetRef.current = layoutPreset;

    const sizes = PRESET_SIZES[layoutPreset];
    setSessionPreset(layoutPreset);

    requestAnimationFrame(() => {
      const sidebar = sidebarPanelRef.current;
      const center = centerPanelRef.current;
      const right = rightPanelRef.current;

      if (sizes.sidebarAutoCollapse) {
        if (sidebar && !sidebarCollapsed) {
          sidebar.collapse();
          sidebarAutoCollapsedRef.current = true;
        }
      } else if (sidebarAutoCollapsedRef.current) {
        if (sidebar && sidebarCollapsed) {
          sidebar.expand();
        }
        sidebarAutoCollapsedRef.current = false;
      }

      if (sizes.right === 0) {
        if (right) right.collapse();
      } else {
        if (right) {
          if (right.isCollapsed()) right.expand();
          right.resize(`${sizes.right}%`);
        }
        if (center) center.resize(`${sizes.center}%`);
      }
    });
  }, [layoutPreset, sidebarCollapsed, setSessionPreset]);

  // Collapse right panel on mount
  useEffect(() => {
    requestAnimationFrame(() => {
      const panel = rightPanelRef.current;
      if (panel) panel.collapse();
    });
  }, []);

  // Expand right panel when expandSignal changes
  useEffect(() => {
    if (expandSignal > 0) {
      const panel = rightPanelRef.current;
      if (panel) panel.expand();
    }
  }, [expandSignal]);

  useEffect(() => {
    if (sidebarToggle > 0) handleToggleSidebar();
  }, [sidebarToggle]);

  useEffect(() => {
    if (rightToggle > 0) handleToggleRight();
  }, [rightToggle]);

  function handleToggleSidebar() {
    const panel = sidebarPanelRef.current;
    if (!panel) return;
    if (sidebarCollapsed) {
      panel.expand();
      sidebarAutoCollapsedRef.current = false;
    } else {
      panel.collapse();
    }
  }

  function handleToggleRight() {
    const panel = rightPanelRef.current;
    if (!panel) return;
    if (rightCollapsed) {
      panel.expand();
    } else {
      panel.collapse();
    }
  }

  return (
    <div className="flex h-full flex-col bg-background">
      <TopBar onOpenSettings={() => setSettingsOpen(true)} rightPanelVisible={!rightCollapsed} />

      <div className="flex flex-1 overflow-hidden px-1 pb-1">
        <ResizablePanelGroup
          orientation="horizontal"
          className="flex-1"
        >
          {/* Sidebar */}
          <ResizablePanel
            id="sidebar"
            panelRef={sidebarPanelRef}
            defaultSize="12%"
            minSize="6%"
            maxSize="18%"
            collapsible
            collapsedSize={20}
            onResize={(size) => {
              setSidebarCollapsed(size.inPixels <= 20);
            }}
          >
            <Sidebar
              collapsed={sidebarCollapsed}
              onToggle={handleToggleSidebar}
            />
          </ResizablePanel>

          <ResizableHandle />

          {/* Center: Terminal */}
          <ResizablePanel
            id="center"
            panelRef={centerPanelRef}
            defaultSize="42%"
            minSize="25%"
          >
            <div className="flex h-full flex-col gap-1 overflow-hidden">
              <div className="flex-1 overflow-hidden rounded" style={{ background: "#0a0a0b" }} data-focus-zone="terminal">
                <TerminalManager />
              </div>
              <ActivityDrawer />
            </div>
          </ResizablePanel>

          <ResizableHandle />

          {/* Right panel */}
          <ResizablePanel
            id="right"
            panelRef={rightPanelRef}
            defaultSize="43%"
            minSize="15%"
            maxSize="65%"
            collapsible
            collapsedSize={COLLAPSED_SIZE_PX}
            onResize={(size) => {
              setRightCollapsed(size.inPixels <= COLLAPSED_SIZE_PX);
            }}
          >
            <RightPanel
              collapsed={rightCollapsed}
              onToggle={handleToggleRight}
            />
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>

      <StatusBar layoutPreset={layoutPreset} />

      <ZenMode />
      <Toaster position="bottom-right" />
      <SettingsPanel open={settingsOpen} onOpenChange={setSettingsOpen} />
      <CommandPalette />
    </div>
  );
}
