import { useState, useEffect, useRef } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { settingsOpenAtom } from "@/stores/config";
import { TopBar } from "./TopBar";
import { Sidebar } from "./Sidebar";
import { RightPanel } from "./RightPanel";
import { StatusBar } from "./StatusBar";
import { Toaster } from "sileo";
import { TerminalManager } from "@/components/terminal/TerminalManager";
import * as terminalService from "@/components/terminal/terminalService";
import { SettingsPanel } from "@/components/settings/SettingsPanel";
import { ActivityDrawer } from "@/components/activity/ActivityDrawer";
import { ZenMode } from "@/components/zen/ZenMode";
import { expandRightPanelAtom, activePanelViewAtom, openTabAction } from "@/stores/rightPanel";
import { activeSessionIdAtom } from "@/stores/workspace";
import { planReviewStatusMapAtom, planStateMapAtom } from "@/stores/plan";
import { toggleSidebarAtom, toggleRightPanelAtom, focusZoneAtom } from "@/stores/shortcuts";

import { layoutPresetAtom, PRESET_SIZES, sessionLayoutPresetAtom, type LayoutPreset } from "@/stores/layout";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { CommandPalette } from "@/components/command/CommandPalette";
import { ShipDialog } from "@/components/git/ShipDialog";
import { ScratchpadPanel } from "@/components/scratchpad/ScratchpadPanel";
import { BrowserFloating } from "@/components/browser/BrowserFloating";
import { BrowserHost } from "@/components/browser/BrowserHost";
import { activeConflictedFilesAtom, gitChipModeAtom } from "@/stores/git";
import { selectedConflictFileMapAtom } from "@/stores/conflict";
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/resizable";
import { usePanelRef } from "react-resizable-panels";

const COLLAPSED_SIZE_PX = 0;

export function Workspace() {
  useKeyboardShortcuts();

  const [settingsOpen, setSettingsOpen] = useAtom(settingsOpenAtom);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [rightCollapsed, setRightCollapsed] = useState(true);
  const expandSignal = useAtomValue(expandRightPanelAtom);
  const sidebarToggle = useAtomValue(toggleSidebarAtom);
  const rightToggle = useAtomValue(toggleRightPanelAtom);
  const layoutPreset = useAtomValue(layoutPresetAtom);
  const setSessionPreset = useSetAtom(sessionLayoutPresetAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);

  const sidebarPanelRef = usePanelRef();
  const centerPanelRef = usePanelRef();
  const rightPanelRef = usePanelRef();

  const prevPresetRef = useRef<LayoutPreset | null>(null);
  const sidebarAutoCollapsedRef = useRef(false);

  // Force re-apply when session changes (the new session may have the same preset name but different panel state)
  const prevSessionRef = useRef<string | null>(null);

  const setActivePanelView = useSetAtom(activePanelViewAtom);
  const activeConflictedFiles = useAtomValue(activeConflictedFilesAtom);
  const setSelectedConflictMap = useSetAtom(selectedConflictFileMapAtom);
  const setChipModeMap = useSetAtom(gitChipModeAtom);

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

  const planReviewMap = useAtomValue(planReviewStatusMapAtom);
  const planStateMap = useAtomValue(planStateMapAtom);
  const openTab = useSetAtom(openTabAction);

  // Handle session switch: open pending plan review or clear panel view
  const prevSwitchSessionRef = useRef<string | null>(null);
  useEffect(() => {
    if (!activeSessionId || prevSwitchSessionRef.current === activeSessionId) return;
    const isFirstMount = prevSwitchSessionRef.current === null;
    prevSwitchSessionRef.current = activeSessionId;
    if (isFirstMount) return;

    // If new session has a pending plan review, open it
    if (planReviewMap[activeSessionId] === "pending_review") {
      const planState = planStateMap[activeSessionId];
      if (planState) {
        const planName = planState.path.split("/").pop()?.replace(".md", "") ?? "Plan";
        openTab({ tab: { id: `plan-${planState.path}`, type: "plan", label: planName, data: { path: planState.path } }, isPinned: true });
        setActivePanelView("plan");
        requestAnimationFrame(() => {
          const panel = rightPanelRef.current;
          if (panel) panel.expand();
        });
        return;
      }
    }

    // Standalone panel view (Ctrl+Shift+G etc.) is now persisted per-session
    // via `activePanelViewMapAtom`, so we no longer clear it on switch — the
    // layout-preset effect below picks up the new session's view and resizes
    // the right panel accordingly. Sessions that never opened a panel start
    // with view=null → "terminal-focus" preset → right collapsed (default).

    // Focus terminal on session switch so the user can type immediately.
    requestAnimationFrame(() => {
      setFocusZone("terminal");
      terminalService.focusActive();
    });
  }, [activeSessionId]);

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

  // Collapse right panel on mount and send focus to sidebar
  useEffect(() => {
    requestAnimationFrame(() => {
      const panel = rightPanelRef.current;
      if (panel) panel.collapse();
      setFocusZone("sidebar");
      const zone = document.querySelector("[data-focus-zone='sidebar']") as HTMLElement | null;
      if (zone) {
        const items = Array.from(zone.querySelectorAll<HTMLElement>("[data-nav-item]"));
        for (const item of items) item.removeAttribute("data-nav-selected");
        if (items[0]) items[0].setAttribute("data-nav-selected", "true");
        zone.focus();
      }
    });
  }, []);

  // Expand right panel when expandSignal changes
  useEffect(() => {
    if (expandSignal > 0) {
      const panel = rightPanelRef.current;
      if (panel) panel.expand();
    }
  }, [expandSignal]);

  // Auto-route to the Conflicts chip when conflicts appear in the active
  // session: pre-select the first file, set the chip, expand the right panel.
  // The user lands on the Conflicts chip immediately, ready to resolve.
  const prevConflictCountRef = useRef(0);
  useEffect(() => {
    if (!activeSessionId) return;
    const count = activeConflictedFiles.length;
    if (count > 0 && prevConflictCountRef.current === 0) {
      setSelectedConflictMap((prev) => ({ ...prev, [activeSessionId]: activeConflictedFiles[0] }));
      setChipModeMap((prev) => ({ ...prev, [activeSessionId]: "conflicts" }));
      setActivePanelView("git");
      requestAnimationFrame(() => {
        const panel = rightPanelRef.current;
        if (panel) panel.expand();
      });
    }
    prevConflictCountRef.current = count;
  }, [activeSessionId, activeConflictedFiles, setSelectedConflictMap, setChipModeMap, setActivePanelView]);

  // Auto-expand right panel when a plan review arrives for the active session
  const prevPlanReviewRef = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (!activeSessionId) return;
    const status = planReviewMap[activeSessionId];
    const prev = prevPlanReviewRef.current;
    prevPlanReviewRef.current = status;
    if (status === "pending_review" && prev !== "pending_review") {
      const planState = planStateMap[activeSessionId];
      if (planState) {
        const planName = planState.path.split("/").pop()?.replace(".md", "") ?? "Plan";
        openTab({ tab: { id: `plan-${planState.path}`, type: "plan", label: planName, data: { path: planState.path } }, isPinned: true });
        setActivePanelView("plan");
        requestAnimationFrame(() => {
          const panel = rightPanelRef.current;
          if (panel) panel.expand();
          setFocusZone("panel");
          const terminalInput = document.querySelector(
            "[data-focus-zone='terminal'] textarea",
          ) as HTMLElement | null;
          terminalInput?.blur();
          const panelEl = document.querySelector(
            "[data-focus-zone='panel']",
          ) as HTMLElement | null;
          panelEl?.focus();
        });
      }
    }
  }, [activeSessionId, planReviewMap, planStateMap]);

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
    <div className="flex h-full flex-col bg-background/65">
      <TopBar onOpenSettings={() => setSettingsOpen(true)} rightPanelVisible={!rightCollapsed} />

      <div className="flex flex-1 overflow-hidden p-2">
        <ResizablePanelGroup
          orientation="horizontal"
          className="flex-1"
        >
          {/* Sidebar */}
          <ResizablePanel
            id="sidebar"
            panelRef={sidebarPanelRef}
            defaultSize="15%"
            minSize="6%"
            maxSize="22%"
            collapsible
            collapsedSize={32}
            onResize={(size) => {
              setSidebarCollapsed(size.inPixels <= 32);
            }}
          >
            <Sidebar collapsed={sidebarCollapsed} />
          </ResizablePanel>

          <ResizableHandle />

          {/* Center: Terminal */}
          <ResizablePanel
            id="center"
            panelRef={centerPanelRef}
            defaultSize="42%"
            minSize="25%"
          >
            <div className="flex h-full flex-col gap-2 overflow-hidden">
              <div
                className="flex-1 overflow-hidden rounded bg-terminal-surface"
                data-focus-zone="terminal"
                onMouseDown={() => {
                  setFocusZone("terminal");
                  terminalService.focusActive();
                }}
              >
                <TerminalManager />
              </div>
              <ActivityDrawer />
            </div>
          </ResizablePanel>

          {!rightCollapsed && <ResizableHandle />}

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
            <RightPanel collapsed={rightCollapsed} />
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>

      <StatusBar />

      <ZenMode />
      <Toaster position="bottom-right" />
      <SettingsPanel open={settingsOpen} onOpenChange={setSettingsOpen} />
      <CommandPalette />
      <ShipDialog />
      <ScratchpadPanel />
      <BrowserFloating />
      <BrowserHost />
    </div>
  );
}
