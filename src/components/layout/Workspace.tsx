import { useState, useEffect, useRef } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { configAtom, settingsOpenAtom } from "@/stores/config";
import { useFocusPulse } from "@/hooks/useFocusPulse";
import { TopBar } from "./TopBar";
import { Sidebar } from "./Sidebar";
import { RightPanel } from "./RightPanel";
import { StatusBar } from "./StatusBar";
import { Toaster } from "sileo";
import { TerminalManager } from "@/components/terminal/TerminalManager";
import { QuakeTerminal } from "@/components/quake/QuakeTerminal";
import * as terminalService from "@/components/terminal/terminalService";
import { SettingsPanel } from "@/components/settings/SettingsPanel";
import { ActivityDrawer } from "@/components/activity/ActivityDrawer";
import { ZenMode } from "@/components/zen/ZenMode";
import { expandRightPanelAtom, activePanelViewAtom, openTabAction, rightPanelCollapsedMapAtom, NO_SESSION_PANEL_KEY } from "@/stores/rightPanel";
import { activeSessionIdAtom } from "@/stores/workspace";
import { planReviewStatusMapAtom, planStateMapAtom } from "@/stores/plan";
import { toggleSidebarAtom, toggleRightPanelAtom, focusZoneAtom } from "@/stores/shortcuts";

import { layoutPresetAtom, PRESET_SIZES, sessionLayoutPresetAtom, terminalFullscreenAtom, type LayoutPreset } from "@/stores/layout";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { CommandPalette } from "@/components/command/CommandPalette";
import { ShipDialog } from "@/components/git/ShipDialog";
import { BranchRenameDialog } from "@/components/git/BranchRenameDialog";
import { ScratchpadPanel } from "@/components/scratchpad/ScratchpadPanel";
import { QuickCapturePanel } from "@/components/floating/QuickCapturePanel";
import { VaultSearchModal } from "@/components/search/VaultSearchModal";
import { ProjectBootstrapPrompt } from "@/components/session/ProjectBootstrapPrompt";
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
  const rightCollapsedMap = useAtomValue(rightPanelCollapsedMapAtom);
  const setRightCollapsedMap = useSetAtom(rightPanelCollapsedMapAtom);
  const expandSignal = useAtomValue(expandRightPanelAtom);
  const sidebarToggle = useAtomValue(toggleSidebarAtom);
  const rightToggle = useAtomValue(toggleRightPanelAtom);
  const layoutPreset = useAtomValue(layoutPresetAtom);
  const setSessionPreset = useSetAtom(sessionLayoutPresetAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const focusZone = useAtomValue(focusZoneAtom);
  const focusPulseEnabled = useAtomValue(configAtom).panel_focus_pulse;
  const isTerminalFocused = focusZone === "terminal";
  const terminalPulsing = useFocusPulse(isTerminalFocused);
  const terminalShowAccent = focusPulseEnabled ? terminalPulsing : isTerminalFocused;
  const terminalBorderClass = terminalShowAccent ? "border-primary" : "border-border";

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

  // Session-less panel state (global views like ClickUp) keys under the
  // sentinel so the right panel can expand before any session exists.
  const collapseKey = activeSessionId ?? NO_SESSION_PANEL_KEY;
  const rightCollapsed = rightCollapsedMap[collapseKey] ?? true;
  function setRightCollapsed(next: boolean) {
    setRightCollapsedMap((prev) =>
      prev[collapseKey] === next ? prev : { ...prev, [collapseKey]: next },
    );
  }

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

    // If new session has a pending plan review, open it — unless the user
    // explicitly hid the right panel in this session (saved gesture in
    // rightCollapsedMap). Re-expanding over that gesture was the one switch
    // path that ignored the override (BUG-06 v0.2.0).
    if (planReviewMap[activeSessionId] === "pending_review" && rightCollapsedMap[activeSessionId] !== true) {
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
    // Double rAF: the layout-preset effect's rAF (expand/collapse) and any
    // panel-content mounts it triggers run in between — asserting focus
    // after them keeps restored panel views from outracing the terminal
    // (BUG-09 v0.2.0).
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        setFocusZone("terminal");
        terminalService.focusActive();
      });
    });
  }, [activeSessionId]);

  // Apply layout preset when it changes or session switches
  useEffect(() => {
    const sessionChanged = prevSessionRef.current !== activeSessionId;
    prevSessionRef.current = activeSessionId ?? null;
    const presetChanged = prevPresetRef.current !== layoutPreset;

    if (!sessionChanged && !presetChanged) return;
    prevPresetRef.current = layoutPreset;

    const sizes = PRESET_SIZES[layoutPreset];
    setSessionPreset(layoutPreset);

    // Saved gesture wins on any session switch — preset frequently differs
    // between sessions, so guarding on !presetChanged would never fire and
    // the panel would re-expand on return.
    const userOverride =
      sessionChanged && activeSessionId
        ? rightCollapsedMap[activeSessionId]
        : undefined;

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

      if (userOverride === true) {
        if (right && !right.isCollapsed()) right.collapse();
        return;
      }
      if (userOverride === false) {
        if (right) {
          if (right.isCollapsed()) right.expand();
          if (sizes.right > 0) right.resize(`${sizes.right}%`);
        }
        if (center && sizes.center > 0) center.resize(`${sizes.center}%`);
        return;
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
  }, [layoutPreset, sidebarCollapsed, activeSessionId, rightCollapsedMap, setSessionPreset]);

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

  const terminalFullscreen = useAtomValue(terminalFullscreenAtom);

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
      requestAnimationFrame(() => {
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
    } else {
      // Focus shift MUST happen before collapse — unmounting the panel
      // subtree orphans activeElement to <body> and breaks key input.
      setFocusZone("terminal");
      requestAnimationFrame(() => terminalService.focusActive());
      panel.collapse();
    }
  }

  return (
    <div className="flex h-full flex-col bg-background">
      <TopBar onOpenSettings={() => setSettingsOpen(true)} rightPanelVisible={!rightCollapsed} />

      <div className="relative flex flex-1 overflow-hidden p-2">
        <QuakeTerminal />
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
            <div className="flex h-full flex-col gap-2">
              <div
                className={
                  terminalFullscreen
                    ? "fixed inset-0 z-[60] overflow-hidden bg-terminal-surface p-2"
                    : `flex-1 overflow-hidden rounded-lg border-2 ${terminalBorderClass} bg-terminal-surface p-2 cluihud-panel-focus`
                }
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
      <BranchRenameDialog />
      <ScratchpadPanel />
      <QuickCapturePanel />
      <VaultSearchModal />
      <ProjectBootstrapPrompt />
      <BrowserFloating />
      <BrowserHost />
    </div>
  );
}
