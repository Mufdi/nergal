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
import { planDocumentsAtom, defaultPlanState } from "@/stores/plan";
import { PlanPanel } from "@/components/plan/PlanPanel";
import { TaskPanel } from "@/components/tasks/TaskPanel";
import { TranscriptViewer } from "@/components/session/TranscriptViewer";
import { DiffView } from "@/components/plan/DiffView";
import { PlanListView } from "@/components/panel/PlanListView";
import { FileListView } from "@/components/panel/FileListView";
import { SpecListView } from "@/components/panel/SpecListView";
import { SpecPanel } from "@/components/spec/SpecPanel";
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
} from "lucide-react";
import {
  Tooltip,
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
      <div className="flex h-full w-full flex-col items-center gap-1 bg-background py-2">
        <button
          onClick={onToggle}
          className="flex size-7 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
          aria-label="Expand panel"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="15 18 9 12 15 6" />
          </svg>
        </button>
        {tabs.map((tab) => {
          const Icon = TAB_ICONS[tab.type];
          return (
            <Tooltip key={tab.id}>
              <TooltipTrigger
                render={
                  <button
                    onClick={() => { setActiveTabId(tab.id); onToggle(); }}
                    className="flex size-7 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                    aria-label={tab.label}
                  />
                }
              >
                <Icon size={14} />
              </TooltipTrigger>
              <TooltipContent side="left">{tab.label}</TooltipContent>
            </Tooltip>
          );
        })}
      </div>
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
              <div className="w-44 shrink-0 overflow-y-auto border-l border-border/50">
                <SidebarContent type={activeTab.type} />
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  if (activePanelView) {
    const hasSidebar = activePanelView === "diff" || activePanelView === "spec";
    const sidebarHint = activePanelView === "diff" ? "Select a file to view diff" : "Select a change to view";
    const SidebarComponent = activePanelView === "diff" ? FileListView : SpecListView;
    return (
      <div className="flex h-full overflow-hidden rounded-lg bg-card" data-focus-zone="panel" tabIndex={-1} onMouseDown={handlePanelFocus}>
        <div className="flex flex-1 flex-col overflow-hidden">
          <PanelHeader onToggle={onToggle} label={viewPanelLabel(activePanelView)} />
          {tabs.length > 0 && <TabBar />}
          <div className="flex flex-1 overflow-hidden">
            {hasSidebar ? (
              <>
                <div className="flex flex-1 items-center justify-center">
                  <span className="text-[11px] text-muted-foreground">{sidebarHint}</span>
                </div>
                <div className="w-44 shrink-0 overflow-y-auto border-l border-border/50">
                  <SidebarComponent />
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
      return <PlanListView />;
    case "file":
    case "diff":
      return <FileListView />;
    case "spec":
      return <SpecListView />;
    default:
      return null;
  }
}

function ViewPanelContent({ view }: { view: TabType }) {
  switch (view) {
    case "plan":
      return <PlanListView />;
    case "file":
      return <FileListView />;
    case "diff":
      return null;
    case "tasks":
      return <TaskPanel />;
    case "git":
      return <PlaceholderView label="Git view" />;
    case "spec":
      return null;
    case "transcript":
      return <PlaceholderView label="Transcript view" />;
    default:
      return null;
  }
}

function DocumentContent({ tab }: { tab: Tab }) {
  switch (tab.type) {
    case "plan":
      return <PlanContentWrapper tabId={tab.id} path={tab.data?.path as string} />;
    case "tasks":
      return <TaskPanel />;
    case "transcript": {
      const sessionId = tab.data?.sessionId as string | undefined;
      return sessionId ? <TranscriptViewer sessionId={sessionId} /> : null;
    }
    case "diff": {
      const diffPath = tab.data?.path as string | undefined;
      const diffSession = tab.data?.sessionId as string | undefined;
      return diffPath && diffSession
        ? <DiffView filePath={diffPath} sessionId={diffSession} />
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
      return <PlaceholderView label="Git view" />;
    case "file":
      return <PlaceholderView label={`File: ${(tab.data?.path as string) ?? "unknown"}`} />;
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

function PlaceholderView({ label }: { label: string }) {
  return (
    <div className="flex h-full w-full items-center justify-center">
      <span className="text-[11px] text-muted-foreground">{label} — coming soon</span>
    </div>
  );
}
