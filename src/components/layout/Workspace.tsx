import { useState, useEffect, useCallback, useRef } from "react";
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
import { expandRightPanelAtom } from "@/stores/rightPanel";
import { toggleSidebarAtom, toggleRightPanelAtom } from "@/stores/shortcuts";
import { layoutPresetAtom, PRESET_SIZES, isDraggingAtom, sessionLayoutPresetAtom, type LayoutPreset } from "@/stores/layout";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { CommandPalette } from "@/components/command/CommandPalette";
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/resizable";
import { usePanelRef } from "react-resizable-panels";

const COLLAPSED_SIZE_PX = 40;

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
  const setIsDragging = useSetAtom(isDraggingAtom);
  const isDragging = useAtomValue(isDraggingAtom);

  const sidebarPanelRef = usePanelRef();
  const centerPanelRef = usePanelRef();
  const rightPanelRef = usePanelRef();

  const prevPresetRef = useRef<LayoutPreset | null>(null);
  const sidebarAutoCollapsedRef = useRef(false);
  const transitionEnabledRef = useRef(true);
  const groupRef = useRef<HTMLDivElement>(null);

  // Manage CSS transition class on the panel group
  const setTransition = useCallback((enabled: boolean) => {
    transitionEnabledRef.current = enabled;
    const el = groupRef.current;
    if (!el) return;
    if (enabled) {
      el.classList.add("panel-transition");
    } else {
      el.classList.remove("panel-transition");
    }
  }, []);

  // Disable transition while dragging
  useEffect(() => {
    setTransition(!isDragging);
  }, [isDragging, setTransition]);

  const handleDragStart = useCallback(() => setIsDragging(true), [setIsDragging]);
  const handleDragEnd = useCallback(() => setIsDragging(false), [setIsDragging]);

  // Apply layout preset when it changes
  useEffect(() => {
    if (prevPresetRef.current === layoutPreset) return;
    prevPresetRef.current = layoutPreset;

    const sizes = PRESET_SIZES[layoutPreset];
    setSessionPreset(layoutPreset);
    setTransition(true);

    requestAnimationFrame(() => {
      const sidebar = sidebarPanelRef.current;
      const center = centerPanelRef.current;
      const right = rightPanelRef.current;

      // Sidebar auto-collapse for tool-workspace
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

      // Right panel: collapse or expand
      if (sizes.right === 0) {
        if (right) right.collapse();
      } else {
        if (right) {
          if (right.isCollapsed()) right.expand();
          right.resize(sizes.right);
        }
        if (center) center.resize(sizes.center);
      }
    });
  }, [layoutPreset, sidebarCollapsed, setSessionPreset, setTransition]);

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

      <div ref={groupRef} className="flex flex-1 overflow-hidden p-1.5">
        <ResizablePanelGroup
          orientation="horizontal"
          className="flex-1"
        >
          {/* Sidebar */}
          <ResizablePanel
            id="sidebar"
            panelRef={sidebarPanelRef}
            defaultSize="15%"
            minSize="8%"
            maxSize="20%"
            collapsible
            collapsedSize={COLLAPSED_SIZE_PX}
            onResize={(size) => {
              setSidebarCollapsed(size.inPixels <= COLLAPSED_SIZE_PX);
            }}
          >
            <Sidebar
              collapsed={sidebarCollapsed}
              onToggle={handleToggleSidebar}
            />
          </ResizablePanel>

          <ResizableHandle onPointerDown={handleDragStart} onPointerUp={handleDragEnd} />

          {/* Center: Terminal (full height — activity log moved to status bar drawer) */}
          <ResizablePanel
            id="center"
            panelRef={centerPanelRef}
            defaultSize="42%"
            minSize="25%"
          >
            <div className="flex h-full overflow-hidden rounded-lg" style={{ background: "#141415" }}>
              <TerminalManager />
            </div>
          </ResizablePanel>

          <ResizableHandle onPointerDown={handleDragStart} onPointerUp={handleDragEnd} />

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

      <ActivityDrawer />
      <StatusBar layoutPreset={layoutPreset} />

      <ZenMode />
      <Toaster position="bottom-right" />
      <SettingsPanel open={settingsOpen} onOpenChange={setSettingsOpen} />
      <CommandPalette />
    </div>
  );
}
