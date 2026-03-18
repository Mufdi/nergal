import { useState, useEffect } from "react";
import { useAtomValue } from "jotai";
import { TopBar } from "./TopBar";
import { Sidebar } from "./Sidebar";
import { RightPanel } from "./RightPanel";
import { StatusBar } from "./StatusBar";
import { Toasts } from "./Toasts";
import { TerminalManager } from "@/components/terminal/TerminalManager";
import { ActivityLog } from "@/components/activity/ActivityLog";
import { SettingsPanel } from "@/components/settings/SettingsPanel";
import { expandRightPanelAtom } from "@/stores/rightPanel";
import { toggleSidebarAtom, toggleRightPanelAtom, toggleActivityLogAtom } from "@/stores/shortcuts";
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
  const activityToggle = useAtomValue(toggleActivityLogAtom);

  const sidebarPanelRef = usePanelRef();
  const rightPanelRef = usePanelRef();
  const activityPanelRef = usePanelRef();

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

  useEffect(() => {
    if (activityToggle > 0) {
      const panel = activityPanelRef.current;
      if (panel) {
        if (panel.isCollapsed()) panel.expand();
        else panel.collapse();
      }
    }
  }, [activityToggle]);

  function handleToggleSidebar() {
    const panel = sidebarPanelRef.current;
    if (!panel) return;
    if (sidebarCollapsed) {
      panel.expand();
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

      <div className="flex flex-1 overflow-hidden p-1.5">
        <ResizablePanelGroup orientation="horizontal" className="flex-1">
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

          <ResizableHandle />

          {/* Center: Terminal + Activity Log (vertical split) */}
          <ResizablePanel id="center" defaultSize="42%" minSize="25%">
            <ResizablePanelGroup orientation="vertical">
              <ResizablePanel id="terminal" defaultSize="75%" minSize="30%">
                <div className="flex h-full overflow-hidden rounded-lg" style={{ background: "#141415" }}>
                  <TerminalManager />
                </div>
              </ResizablePanel>

              <ResizableHandle />

              <ResizablePanel
                id="activity"
                panelRef={activityPanelRef}
                defaultSize="25%"
                collapsible
                collapsedSize={28}
              >
                <div className="flex h-full flex-col overflow-hidden rounded-lg bg-card">
                  <ActivityLog />
                </div>
              </ResizablePanel>
            </ResizablePanelGroup>
          </ResizablePanel>

          <ResizableHandle />

          {/* Right panel */}
          <ResizablePanel
            id="right"
            panelRef={rightPanelRef}
            defaultSize="43%"
            minSize="15%"
            maxSize="55%"
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

      <StatusBar />

      <Toasts />
      <SettingsPanel open={settingsOpen} onOpenChange={setSettingsOpen} />
      <CommandPalette />
    </div>
  );
}
