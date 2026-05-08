import { useState, useEffect, useCallback, useRef } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import {
  activeTabAtom,
  activeTabsAtom,
  activePanelViewAtom,
  setDirtyAction,
  setTabPathAction,
  tabOpenedSignalAtom,
  filePickerOpenAtom,
  type Tab,
  type TabType,
} from "@/stores/rightPanel";
import { focusZoneAtom, previousNonTerminalZoneAtom } from "@/stores/shortcuts";
import { configAtom } from "@/stores/config";
import { useFocusPulse } from "@/hooks/useFocusPulse";
import { activeSessionIdAtom } from "@/stores/workspace";
import { AnnotationsDrawer } from "@/components/plan/AnnotationsDrawer";
import { PlanPanel } from "@/components/plan/PlanPanel";
import { TranscriptViewer } from "@/components/session/TranscriptViewer";
import { DiffView } from "@/components/plan/DiffView";
import { PlanListView } from "@/components/panel/PlanListView";
// PlanSidebar will be replaced by annotations drawer in the plan panel
import { FileListView } from "@/components/panel/FileListView";
import { SpecListView } from "@/components/panel/SpecListView";
import { SpecPanel } from "@/components/spec/SpecPanel";
import { GitPanel } from "@/components/git/GitPanel";
import { FileBrowser } from "@/components/files/FileBrowser";
import { CodeEditor } from "@/components/editor/CodeEditor";
import { DagGraph } from "@/components/activity/DagGraph";
import { DOCK_SLOT_ATTR } from "@/components/browser/BrowserHost";
import { browserModeForSessionAtom, browserSetModeAction } from "@/stores/browser";
import { openZenModeAtom } from "@/stores/zenMode";
import { activeSessionFilesAtom } from "@/stores/files";
import { TabBar } from "@/components/ui/TabBar";
import { FolderOpen } from "lucide-react";

const PICKER_TYPES: TabType[] = ["plan", "file", "diff", "spec"];

interface RightPanelProps {
  collapsed: boolean;
}

export function RightPanel({ collapsed }: RightPanelProps) {
  const activeTab = useAtomValue(activeTabAtom);
  const tabs = useAtomValue(activeTabsAtom);
  const activePanelView = useAtomValue(activePanelViewAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const setPreviousZone = useSetAtom(previousNonTerminalZoneAtom);
  const focusZone = useAtomValue(focusZoneAtom);
  const isFocused = focusZone === "panel";
  const focusPulseEnabled = useAtomValue(configAtom).panel_focus_pulse;
  const isPulsing = useFocusPulse(isFocused);
  const showAccent = focusPulseEnabled ? isPulsing : isFocused;
  const borderClass = showAccent ? "border-primary" : "border-border";
  const [pickerOpen, setPickerOpen] = useAtom(filePickerOpenAtom);
  const [annotationsDrawerOpen, setAnnotationsDrawerOpen] = useState(false);

  // Toggle annotations drawer via custom event (from shortcuts registry or PlanPanel)
  useEffect(() => {
    function handleToggle() {
      setAnnotationsDrawerOpen((prev) => !prev);
    }
    document.addEventListener("cluihud:toggle-annotations-drawer", handleToggle);
    return () => document.removeEventListener("cluihud:toggle-annotations-drawer", handleToggle);
  }, []);

  // Drawer self-focuses via its own callback ref — no parent focus management needed

  // Auto-open drawer when annotations appear
  useEffect(() => {
    if (activeTab?.type === "plan" || activeTab?.type === "spec") {
      const handleAnnotationAdded = () => setAnnotationsDrawerOpen(true);
      document.addEventListener("cluihud:annotation-added", handleAnnotationAdded);
      return () => document.removeEventListener("cluihud:annotation-added", handleAnnotationAdded);
    }
  }, [activeTab?.type]);
  const tabOpenedSignal = useAtomValue(tabOpenedSignalAtom);

  // Close picker and focus panel when any tab is opened/selected
  useEffect(() => {
    if (tabOpenedSignal > 0) {
      setPickerOpen(false);
      setFocusZone("panel");
      requestAnimationFrame(() => {
        const panel = document.querySelector("[data-focus-zone='panel']") as HTMLElement | null;
        if (panel) panel.focus();
      });
    }
  }, [tabOpenedSignal]);

  // Listen for file picker shortcut + Esc to close
  useEffect(() => {
    function handlePickerToggle() {
      setPickerOpen((prev) => !prev);
    }
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape" && pickerOpen) {
        e.preventDefault();
        setPickerOpen(false);
      }
    }
    document.addEventListener("cluihud:toggle-file-picker", handlePickerToggle);
    window.addEventListener("keydown", handleKeyDown, true);
    return () => {
      document.removeEventListener("cluihud:toggle-file-picker", handlePickerToggle);
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [pickerOpen]);

  function handlePanelFocus() {
    setFocusZone("panel");
    setPreviousZone("panel");
  }

  function handlePanelKeyDown(e: React.KeyboardEvent) {
    if (e.ctrlKey || e.altKey || pickerOpen) return;
    if (!activeTab) return;

    const SCROLL_KEYS = ["ArrowDown", "ArrowUp", "PageDown", "PageUp", "Home", "End"];
    if (!SCROLL_KEYS.includes(e.key)) return;

    // Find the scrollable content area
    const panel = e.currentTarget as HTMLElement;
    const scrollable = panel.querySelector("[data-scrollable], .overflow-y-auto") as HTMLElement | null;
    if (!scrollable || scrollable.scrollHeight <= scrollable.clientHeight) return;

    e.preventDefault();
    const page = scrollable.clientHeight - 40;

    switch (e.key) {
      case "ArrowDown": scrollable.scrollBy(0, 24); break;
      case "ArrowUp": scrollable.scrollBy(0, -24); break;
      case "PageDown": scrollable.scrollBy({ top: page, behavior: "smooth" }); break;
      case "PageUp": scrollable.scrollBy({ top: -page, behavior: "smooth" }); break;
      case "Home": scrollable.scrollTo({ top: 0, behavior: "smooth" }); break;
      case "End": scrollable.scrollTo({ top: scrollable.scrollHeight, behavior: "smooth" }); break;
    }
  }

  if (collapsed) {
    // Render an empty bg-card island so the ResizablePanel slot shows the
    // theme's card surface instead of letting the workspace canvas
    // (--background) bleed through. Without this, light theme leaks a gray
    // vertical strip across the right column.
    return <div className="h-full w-full rounded-lg border-2 border-border bg-card cluihud-panel-focus" />;
  }

  const hasPicker = activeTab && PICKER_TYPES.includes(activeTab.type);

  if (activeTab) {
    const showAnnotationsDrawer = activeTab.type === "plan" || activeTab.type === "spec";
    return (
      <div className="flex h-full flex-col gap-1">
        <div className={`relative flex flex-1 flex-col overflow-hidden rounded-lg border-2 ${borderClass} bg-card cluihud-panel-focus`} data-focus-zone="panel" tabIndex={-1} onMouseDown={handlePanelFocus} onKeyDown={handlePanelKeyDown}>
          {/* Level 1: Tabs + actions */}
          <div className="flex shrink-0 items-center border-b border-border/50">
            <div className="flex-1 overflow-hidden">
              <TabBar />
            </div>
            {hasPicker && (
              <button
                onClick={() => setPickerOpen(!pickerOpen)}
                className={`flex size-7 shrink-0 items-center justify-center transition-colors ${
                  pickerOpen
                    ? "text-foreground bg-secondary"
                    : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                }`}
                aria-label={pickerOpen ? "Close file picker" : "Open file picker"}
              >
                <FolderOpen size={12} />
              </button>
            )}
          </div>

          {/* Content */}
          <div className="flex-1 overflow-hidden">
            <DocumentContent tab={activeTab} />
          </div>

          {hasPicker && pickerOpen && (
            <FilePickerOverlay
              type={activeTab.type}
              onClose={() => setPickerOpen(false)}
            />
          )}
        </div>

        {showAnnotationsDrawer && (
          <AnnotationsDrawer
            open={annotationsDrawerOpen}
            onToggle={() => setAnnotationsDrawerOpen(!annotationsDrawerOpen)}
          />
        )}
      </div>
    );
  }

  if (activePanelView) {
    const hasPanelPicker = PICKER_TYPES.includes(activePanelView);
    return (
      <div className={`relative flex h-full flex-col overflow-hidden rounded-lg border-2 ${borderClass} bg-card cluihud-panel-focus`} data-focus-zone="panel" tabIndex={-1} onMouseDown={handlePanelFocus}>
        {tabs.length > 0 ? (
          <div className="flex shrink-0 items-center border-b border-border/50">
            <div className="flex-1 overflow-hidden">
              <TabBar />
            </div>
          </div>
        ) : (
          <div className="flex h-8 shrink-0 items-center px-3 border-b border-border/50">
            <span className="text-[11px] font-medium text-foreground/80">{viewPanelLabel(activePanelView)}</span>
          </div>
        )}
        <div className="flex-1 overflow-hidden">
          {hasPanelPicker ? (
            <div className="flex h-full items-center justify-center px-6">
              <NavigablePickerContainer type={activePanelView} />
            </div>
          ) : (
            <div className="h-full overflow-y-auto">
              <ViewPanelContent view={activePanelView} />
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className={`flex h-full flex-col rounded-lg border-2 ${borderClass} bg-card cluihud-panel-focus`}>
      {tabs.length > 0 && (
        <div className="flex shrink-0 items-center border-b border-border/50">
          <div className="flex-1 overflow-hidden">
            <TabBar />
          </div>
        </div>
      )}
      <div className="flex flex-1 items-center justify-center">
        <span className="text-[11px] text-muted-foreground">No panel open</span>
      </div>
    </div>
  );
}

// ── File picker overlay ──

function NavigablePickerContainer({ type, className }: { type: TabType; className?: string }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const selectedIdxRef = useRef(0);

  function getItems(): HTMLElement[] {
    if (!containerRef.current) return [];
    return Array.from(containerRef.current.querySelectorAll("[data-nav-item]"));
  }

  useEffect(() => {
    selectedIdxRef.current = 0;
    // Wait for children to render before highlighting + focusing
    const timer = setTimeout(() => {
      const items = getItems();
      if (items[0]) items[0].setAttribute("data-nav-selected", "true");
      containerRef.current?.focus();
    }, 50);
    return () => clearTimeout(timer);
  }, [type]);

  function updateSelection(idx: number) {
    const items = getItems();
    for (const item of items) item.removeAttribute("data-nav-selected");
    if (items[idx]) {
      items[idx].setAttribute("data-nav-selected", "true");
      items[idx].scrollIntoView({ block: "nearest" });
    }
    selectedIdxRef.current = idx;
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    const items = getItems();
    if (items.length === 0) return;
    const idx = selectedIdxRef.current;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      updateSelection(Math.min(idx + 1, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      updateSelection(Math.max(idx - 1, 0));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      const selected = items[idx];
      if (!selected || selected.dataset.navExpanded !== "false") return;
      // Use chevron if present (SpecListView), otherwise click item (FileBrowser dirs)
      const chevron = selected.querySelector("[data-nav-chevron]") as HTMLElement | null;
      if (chevron) {
        chevron.click();
      } else {
        selected.click();
      }
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      const selected = items[idx];
      if (!selected || selected.dataset.navExpanded !== "true") return;
      const chevron = selected.querySelector("[data-nav-chevron]") as HTMLElement | null;
      if (chevron) {
        chevron.click();
      } else {
        selected.click();
      }
    } else if (e.key === "Enter") {
      e.preventDefault();
      items[idx]?.click();
    }
  }

  return (
    <div
      ref={containerRef}
      className={`cluihud-glow w-full max-w-xs max-h-[70%] overflow-y-auto rounded-lg border-2 border-primary bg-card shadow-lg outline-none ${className ?? ""}`}
      data-nav-container
      tabIndex={-1}
      onKeyDown={handleKeyDown}
    >
      <PickerContent type={type} />
    </div>
  );
}

function FilePickerOverlay({ type, onClose }: { type: TabType; onClose: () => void }) {
  return (
    <>
      <div className="absolute inset-0 z-30 bg-scrim cluihud-blur-sm" onClick={onClose} />
      <div className="absolute inset-0 z-40 flex items-center justify-center px-6 pointer-events-none">
        <NavigablePickerContainer type={type} className="pointer-events-auto" />
      </div>
    </>
  );
}

function PickerContent({ type }: { type: TabType }) {
  switch (type) {
    case "plan":
      return <PlanListView />;
    case "file":
      return <FileBrowser />;
    case "diff":
      return <FileListView />;
    case "spec":
      return <SpecListView />;
    default:
      return null;
  }
}

// ── Helpers ──

function viewPanelLabel(view: TabType): string {
  const labels: Record<TabType, string> = {
    plan: "Plans",
    file: "Files",
    diff: "Diff",
    spec: "Spec",
    tasks: "Tasks",
    git: "Git",
    transcript: "Transcript",
    browser: "Browser",
  };
  return labels[view];
}


function ViewPanelContent({ view }: { view: TabType }) {
  switch (view) {
    case "plan":
      return <PlanListView />;
    case "file":
      return <FileBrowser />;
    case "diff":
      return null;
    case "tasks":
      return null;
    case "git":
      return <GitPanelWrapper />;
    case "spec":
      return null;
    case "transcript":
      return <DagGraph />;
    case "browser":
      return <BrowserDockSlot />;
    default:
      return null;
  }
}

function DocumentContent({ tab }: { tab: Tab }) {
  switch (tab.type) {
    case "plan":
      return <PlanContentWrapper tabId={tab.id} path={tab.data?.path as string} />;
    case "tasks":
      return null;
    case "transcript": {
      const sessionId = tab.data?.sessionId as string | undefined;
      return sessionId ? <TranscriptViewer sessionId={sessionId} /> : <DagGraph />;
    }
    case "diff": {
      const diffPath = tab.data?.path as string | undefined;
      const diffSession = tab.data?.sessionId as string | undefined;
      return diffPath && diffSession
        ? <DiffWithExpand tabId={tab.id} filePath={diffPath} sessionId={diffSession} />
        : <PlaceholderView label="Diff view" />;
    }
    case "spec": {
      const specChange = tab.data?.changeName as string | undefined;
      const specSession = tab.data?.sessionId as string | undefined;
      const specPath = tab.data?.specPath as string | undefined;
      return specChange && specSession
        ? <SpecContentWrapper key={tab.id} tabId={tab.id} changeName={specChange} sessionId={specSession} initialSpecPath={specPath} />
        : <PlaceholderView label="Select a change" />;
    }
    case "git":
      return <GitPanelWrapper />;
    case "file": {
      const filePath = tab.data?.path as string | undefined;
      const fileSession = tab.data?.sessionId as string | undefined;
      return filePath && fileSession
        ? <CodeEditor key={tab.id} filePath={filePath} sessionId={fileSession} />
        : <PlaceholderView label={`File: ${filePath ?? "unknown"}`} />;
    }
    case "browser":
      return <BrowserDockSlot />;
    default:
      return null;
  }
}

/// When the user has the browser tab active in the dock, render the slot
/// that BrowserHost portals BrowserPanel into. When mode === "floating",
/// the iframe lives inside the FloatingPanel chrome; the dock slot shows a
/// placeholder card with a "Return to dock" affordance so the tab is never
/// visually empty.
function BrowserDockSlot() {
  const mode = useAtomValue(browserModeForSessionAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const setMode = useSetAtom(browserSetModeAction);
  if (mode === "floating") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 bg-background p-6 text-center text-xs text-muted-foreground">
        <p>Browser is in floating mode.</p>
        <button
          type="button"
          onClick={() => sessionId && setMode({ sessionId, mode: "dock" })}
          disabled={!sessionId}
          className="rounded-md border border-border/60 px-3 py-1.5 text-foreground transition hover:bg-secondary/60 disabled:opacity-40"
        >
          Return to dock
        </button>
      </div>
    );
  }
  return <div className="relative h-full w-full" {...{ [DOCK_SLOT_ATTR]: "" }} />;
}

function PlanContentWrapper({ path }: { tabId: string; path: string }) {
  return <PlanPanel path={path} />;
}


function SpecContentWrapper({ tabId, changeName, sessionId, initialSpecPath }: { tabId: string; changeName: string; sessionId: string; initialSpecPath?: string }) {
  const setDirty = useSetAtom(setDirtyAction);
  const handleDirtyChange = useCallback((dirty: boolean) => {
    setDirty({ tabId, dirty });
  }, [tabId, setDirty]);
  return <SpecPanel changeName={changeName} sessionId={sessionId} initialSpecPath={initialSpecPath} onDirtyChange={handleDirtyChange} />;
}


function GitPanelWrapper() {
  const sessionId = useAtomValue(activeSessionIdAtom);
  const setZenMode = useSetAtom(openZenModeAtom);
  const files = useAtomValue(activeSessionFilesAtom);
  useEffect(() => {
    function onExpandGit(ev: Event) {
      const detail = (ev as CustomEvent<{ sessionId: string }>).detail;
      if (!sessionId || detail?.sessionId !== sessionId) return;
      const paths = files.map((f) => f.path);
      if (paths.length === 0) return;
      setZenMode({ filePath: paths[0], sessionId, files: paths });
    }
    document.addEventListener("cluihud:expand-zen-git", onExpandGit);
    return () => document.removeEventListener("cluihud:expand-zen-git", onExpandGit);
  }, [sessionId, files, setZenMode]);
  return sessionId ? <GitPanel sessionId={sessionId} /> : null;
}

function DiffWithExpand({ tabId, filePath, sessionId }: { tabId: string; filePath: string; sessionId: string }) {
  const setZenMode = useSetAtom(openZenModeAtom);
  const setTabPath = useSetAtom(setTabPathAction);
  const files = useAtomValue(activeSessionFilesAtom);
  const filePaths = files.map((f) => f.path);
  useEffect(() => {
    function onExpand(ev: Event) {
      const detail = (ev as CustomEvent<{ filePath: string; sessionId: string }>).detail;
      if (detail?.filePath === filePath && detail?.sessionId === sessionId) {
        setZenMode({ filePath, sessionId, files: filePaths });
      }
    }
    function onExpandGit(ev: Event) {
      const detail = (ev as CustomEvent<{ sessionId: string }>).detail;
      if (detail?.sessionId === sessionId && filePaths.length > 0) {
        setZenMode({ filePath: filePaths[0], sessionId, files: filePaths });
      }
    }
    document.addEventListener("cluihud:expand-zen", onExpand);
    document.addEventListener("cluihud:expand-zen-git", onExpandGit);
    return () => {
      document.removeEventListener("cluihud:expand-zen", onExpand);
      document.removeEventListener("cluihud:expand-zen-git", onExpandGit);
    };
  }, [filePath, sessionId, filePaths, setZenMode]);

  const handleNavFile = useCallback((direction: "prev" | "next") => {
    if (filePaths.length === 0) return;
    const idx = filePaths.indexOf(filePath);
    if (idx === -1) return;
    const nextIdx = direction === "next"
      ? (idx + 1) % filePaths.length
      : (idx - 1 + filePaths.length) % filePaths.length;
    const next = filePaths[nextIdx];
    if (next && next !== filePath) setTabPath({ tabId, path: next });
  }, [tabId, filePath, filePaths, setTabPath]);

  return (
    <div className="h-full">
      <DiffView
        filePath={filePath}
        sessionId={sessionId}
        onOpenZen={() => setZenMode({ filePath, sessionId, files: filePaths })}
        onNavFile={filePaths.length > 1 ? handleNavFile : undefined}
      />
    </div>
  );
}

function PlaceholderView({ label }: { label: string }) {
  return (
    <div className="flex h-full items-center justify-center">
      <span className="text-sm text-muted-foreground">{label}</span>
    </div>
  );
}
