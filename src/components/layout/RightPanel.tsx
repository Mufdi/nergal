import { useState, useEffect, useCallback, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  activeTabAtom,
  activeTabsAtom,
  activeTabIdAtom,
  activePanelViewAtom,
  setDirtyAction,
  tabOpenedSignalAtom,
  type Tab,
  type TabType,
} from "@/stores/rightPanel";
import { focusZoneAtom, previousNonTerminalZoneAtom } from "@/stores/shortcuts";
import { activeSessionIdAtom } from "@/stores/workspace";
import { planDocumentsAtom, defaultPlanState } from "@/stores/plan";
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
import { openZenModeAtom } from "@/stores/zenMode";
import { activeSessionFilesAtom } from "@/stores/files";
import { TabBar } from "@/components/ui/TabBar";
import {
  FileText,
  FileCode,
  GitCompareArrows,
  ClipboardList,
  CheckSquare,
  GitBranch,
  ScrollText,
  FolderOpen,
  Maximize2,
} from "lucide-react";
import {
  Tooltip,
  TooltipProvider,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

const TAB_ICONS: Record<TabType, typeof FileText> = {
  plan: FileText,
  file: FileCode,
  diff: GitCompareArrows,
  spec: ClipboardList,
  tasks: CheckSquare,
  git: GitBranch,
  transcript: ScrollText,
};

const PICKER_TYPES: TabType[] = ["plan", "file", "diff", "spec"];

interface RightPanelProps {
  collapsed: boolean;
  onToggle: () => void;
}

export function RightPanel({ collapsed, onToggle }: RightPanelProps) {
  const activeTab = useAtomValue(activeTabAtom);
  const tabs = useAtomValue(activeTabsAtom);
  const activePanelView = useAtomValue(activePanelViewAtom);
  const setActiveTabId = useSetAtom(activeTabIdAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const setPreviousZone = useSetAtom(previousNonTerminalZoneAtom);
  const [pickerOpen, setPickerOpen] = useState(false);
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
    return (
      <TooltipProvider delay={0}>
      <div className="flex h-full w-full flex-col items-center gap-0.5 bg-card py-1">
        <button
          onClick={onToggle}
          className="flex size-4 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors mb-0.5"
          aria-label="Expand panel"
        >
          <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="15 18 9 12 15 6" />
          </svg>
        </button>
        {tabs.map((tab) => {
          const Icon = TAB_ICONS[tab.type];
          const isActive = tab.id === activeTab?.id;
          return (
            <Tooltip key={tab.id}>
              <TooltipTrigger
                render={
                  <button
                    onClick={() => { setActiveTabId(tab.id); onToggle(); }}
                    className={`flex size-4 items-center justify-center rounded transition-colors ${
                      isActive ? "text-foreground bg-secondary" : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                    }`}
                    aria-label={tab.label}
                  />
                }
              >
                <Icon size={12} />
              </TooltipTrigger>
              <TooltipContent side="left">{tab.label}</TooltipContent>
            </Tooltip>
          );
        })}
      </div>
      </TooltipProvider>
    );
  }

  const hasPicker = activeTab && PICKER_TYPES.includes(activeTab.type);

  if (activeTab) {
    return (
      <div className="relative flex h-full flex-col overflow-hidden rounded bg-card" data-focus-zone="panel" tabIndex={-1} onMouseDown={handlePanelFocus} onKeyDown={handlePanelKeyDown}>
        <PanelHeader onToggle={onToggle} label={activeTab.label}>
          {hasPicker && (
            <button
              onClick={() => setPickerOpen(!pickerOpen)}
              className={`flex size-6 items-center justify-center rounded transition-colors ${
                pickerOpen
                  ? "text-foreground bg-secondary"
                  : "text-muted-foreground hover:bg-secondary hover:text-foreground"
              }`}
              aria-label={pickerOpen ? "Close file picker" : "Open file picker"}
            >
              <FolderOpen size={12} />
            </button>
          )}
        </PanelHeader>
        <TabBar />
        <div className="flex-1 overflow-hidden">
          <DocumentContent tab={activeTab} />
        </div>

        {/* Floating file picker overlay */}
        {hasPicker && pickerOpen && (
          <FilePickerOverlay
            type={activeTab.type}
            onClose={() => setPickerOpen(false)}
          />
        )}
      </div>
    );
  }

  if (activePanelView) {
    const hasPanelPicker = PICKER_TYPES.includes(activePanelView);
    return (
      <div className="relative flex h-full flex-col overflow-hidden rounded bg-card" data-focus-zone="panel" tabIndex={-1} onMouseDown={handlePanelFocus}>
        <PanelHeader onToggle={onToggle} label={viewPanelLabel(activePanelView)} />
        {tabs.length > 0 && <TabBar />}
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
    <div className="flex h-full flex-col rounded bg-card">
      <PanelHeader onToggle={onToggle} />
      {tabs.length > 0 && <TabBar />}
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
      className={`w-full max-w-xs max-h-[70%] overflow-y-auto rounded border border-border bg-card shadow-2xl outline-none ${className ?? ""}`}
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
      <div className="absolute inset-0 z-30 backdrop-blur-sm bg-black/20" onClick={onClose} />
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
  };
  return labels[view];
}

function PanelHeader({ onToggle, label, children }: { onToggle: () => void; label?: string; children?: React.ReactNode }) {
  return (
    <div className="flex h-9 shrink-0 items-center gap-2 px-2">
      <button
        onClick={onToggle}
        className="flex size-6 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
        aria-label="Collapse panel"
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="9 18 15 12 9 6" />
        </svg>
      </button>
      {label && <span className="flex-1 text-[11px] font-medium text-foreground/80">{label}</span>}
      {!label && <span className="flex-1" />}
      {children}
    </div>
  );
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
        ? <DiffWithExpand filePath={diffPath} sessionId={diffSession} />
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
    default:
      return null;
  }
}

function PlanContentWrapper({ tabId, path }: { tabId: string; path: string }) {
  const docs = useAtomValue(planDocumentsAtom);
  const plan = path ? (docs[path] ?? defaultPlanState) : defaultPlanState;
  const setDirty = useSetAtom(setDirtyAction);

  const hasEdits = plan.content !== plan.original && plan.content.length > 0;

  useEffect(() => {
    setDirty({ tabId, dirty: hasEdits });
  }, [hasEdits, tabId, setDirty]);

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
  return sessionId ? <GitPanel sessionId={sessionId} /> : null;
}

function DiffWithExpand({ filePath, sessionId }: { filePath: string; sessionId: string }) {
  const setZenMode = useSetAtom(openZenModeAtom);
  const files = useAtomValue(activeSessionFilesAtom);
  const filePaths = files.map((f) => f.path);
  return (
    <div className="relative h-full">
      <DiffView filePath={filePath} sessionId={sessionId} />
      <button
        onClick={() => setZenMode({ filePath, sessionId, files: filePaths })}
        className="absolute right-2 top-2 z-10 flex size-6 items-center justify-center rounded bg-card/80 border border-border/50 text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
        aria-label="Expand to zen mode"
      >
        <Maximize2 size={11} />
      </button>
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
