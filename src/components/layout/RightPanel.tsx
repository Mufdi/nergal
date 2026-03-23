import { useState, useEffect, useCallback } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  activeTabAtom,
  activeTabsAtom,
  activeTabIdAtom,
  activePanelViewAtom,
  setDirtyAction,
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
import { PlanSidebar } from "@/components/plan/PlanSidebar";
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
  PanelRightClose,
  PanelRightOpen,
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

const SIDEBAR_TYPES: TabType[] = ["plan", "file", "diff", "spec"];

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
  const [sidebarOpen, setSidebarOpen] = useState(true);

  function handlePanelFocus() {
    setFocusZone("panel");
    setPreviousZone("panel");
  }

  if (collapsed) {
    return (
      <TooltipProvider delay={0}>
      <div className="flex h-full w-full flex-col items-center gap-0.5 bg-background py-1">
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

  const showSidebar = activeTab && SIDEBAR_TYPES.includes(activeTab.type);

  if (activeTab) {
    return (
      <div className="flex h-full overflow-hidden rounded-lg bg-card" data-focus-zone="panel" tabIndex={-1} onMouseDown={handlePanelFocus}>
        <div className="flex flex-1 flex-col overflow-hidden">
          <PanelHeader onToggle={onToggle} label={activeTab.label}>
            {showSidebar && (
              <button
                onClick={() => setSidebarOpen(!sidebarOpen)}
                className="flex size-6 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                aria-label={sidebarOpen ? "Hide file list" : "Show file list"}
              >
                {sidebarOpen ? <PanelRightClose size={12} /> : <PanelRightOpen size={12} />}
              </button>
            )}
          </PanelHeader>
          <TabBar />
          <div className="flex flex-1 overflow-hidden">
            <div className="min-w-0 flex-1 overflow-hidden">
              <DocumentContent tab={activeTab} />
            </div>
            {showSidebar && sidebarOpen && (
              <div className={`shrink-0 overflow-hidden border-l border-border/50 ${activeTab.type === "file" || activeTab.type === "plan" ? "w-52" : "w-44"}`}>
                <SidebarContent type={activeTab.type} />
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  if (activePanelView) {
    const sidebarViews: Record<string, { hint: string; Component: React.ComponentType }> = {
      plan: { hint: "Select a plan file", Component: PlanSidebar },
      diff: { hint: "Select a file to view diff", Component: FileListView },
      spec: { hint: "Select a change to view", Component: SpecListView },
      file: { hint: "Select a file to open", Component: FileBrowser },
    };
    const sidebar = sidebarViews[activePanelView];
    return (
      <div className="flex h-full overflow-hidden rounded-lg bg-card" data-focus-zone="panel" tabIndex={-1} onMouseDown={handlePanelFocus}>
        <div className="flex flex-1 flex-col overflow-hidden">
          <PanelHeader onToggle={onToggle} label={viewPanelLabel(activePanelView)} />
          {tabs.length > 0 && <TabBar />}
          <div className="flex flex-1 overflow-hidden">
            {sidebar ? (
              <>
                <div className="flex flex-1 items-center justify-center">
                  <span className="text-[11px] text-muted-foreground">{sidebar.hint}</span>
                </div>
                <div className="w-52 shrink-0 overflow-y-auto border-l border-border/50">
                  <sidebar.Component />
                </div>
              </>
            ) : (
              <div className="flex-1 overflow-y-auto">
                <ViewPanelContent view={activePanelView} />
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col rounded-lg bg-card">
      <PanelHeader onToggle={onToggle} />
      {tabs.length > 0 && <TabBar />}
      <div className="flex flex-1 items-center justify-center">
        <span className="text-[11px] text-muted-foreground">No panel open</span>
      </div>
    </div>
  );
}

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
    <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border/50 px-2">
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

function SidebarContent({ type }: { type: TabType }) {
  switch (type) {
    case "plan":
      return <PlanSidebar />;
    case "file":
      return <FileBrowser />;
    case "diff":
      return <FileListView />;
    case "spec":
      return <SpecListView />;
    case "git":
      return null;
    default:
      return null;
  }
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
  if (!sessionId) return <PlaceholderView label="No session active" />;
  return <GitPanel sessionId={sessionId} />;
}

function DiffWithExpand({ filePath, sessionId }: { filePath: string; sessionId: string }) {
  const openZen = useSetAtom(openZenModeAtom);
  const files = useAtomValue(activeSessionFilesAtom);

  function handleExpand() {
    const allPaths = files.map((f) => f.path);
    openZen({ filePath, sessionId, files: allPaths.length > 0 ? allPaths : [filePath] });
  }

  return (
    <div className="relative h-full">
      <DiffView filePath={filePath} sessionId={sessionId} />
      <button
        onClick={handleExpand}
        className="absolute right-2 top-2 z-10 flex size-6 items-center justify-center rounded bg-card/80 border border-border/50 text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
        aria-label="Expand to Zen Mode"
      >
        <Maximize2 size={12} />
      </button>
    </div>
  );
}

function PlaceholderView({ label }: { label: string }) {
  return (
    <div className="flex h-full w-full items-center justify-center">
      <span className="text-[11px] text-muted-foreground">{label} — coming soon</span>
    </div>
  );
}
