import { useState } from "react";
import { useAtom, useAtomValue } from "jotai";
import { planVisibleAtom } from "@/stores/plan";
import { activeSessionAtom } from "@/stores/session";
import { TopBar } from "./TopBar";
import { NavSidebar } from "./NavSidebar";
import { StatusBar } from "./StatusBar";
import { Toasts } from "./Toasts";
import { TerminalPanel } from "@/components/terminal/TerminalPanel";
import { TaskPanel } from "@/components/tasks/TaskPanel";
import { ActivityLog } from "@/components/activity/ActivityLog";
import { ModifiedFiles } from "@/components/files/ModifiedFiles";
import { PlanPanel } from "@/components/plan/PlanPanel";
import { SettingsPanel } from "@/components/settings/SettingsPanel";
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/resizable";
import { usePanelRef } from "react-resizable-panels";

type LeftTab = "tasks" | "files" | "activity";

const COLLAPSED_SIZE_PX = 40;

function CollapsedLeftBar({
  onExpand,
  activeTab,
}: {
  onExpand: () => void;
  activeTab: LeftTab;
}) {
  return (
    <div className="flex h-full w-full flex-col items-center gap-2 rounded-xl bg-card py-2">
      <button
        onClick={onExpand}
        className="flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
        aria-label="Expand sidebar"
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="9 18 15 12 9 6" />
        </svg>
      </button>
      {(["tasks", "files", "activity"] as const).map((tab) => (
        <button
          key={tab}
          className={`flex size-8 items-center justify-center rounded-md transition-colors ${
            activeTab === tab
              ? "text-foreground bg-accent/50"
              : "text-muted-foreground hover:bg-accent hover:text-foreground"
          }`}
          aria-label={tab}
          title={tab}
        >
          {tab === "tasks" && (
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M9 11l3 3L22 4" /><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
            </svg>
          )}
          {tab === "files" && (
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" /><path d="M14 2v4a2 2 0 0 0 2 2h4" />
            </svg>
          )}
          {tab === "activity" && (
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
            </svg>
          )}
        </button>
      ))}
    </div>
  );
}

function CollapsedPlanBar({ onExpand }: { onExpand: () => void }) {
  return (
    <div className="flex h-full w-full flex-col items-center gap-2 rounded-xl bg-card py-2">
      <button
        onClick={onExpand}
        className="flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
        aria-label="Expand plan"
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="15 18 9 12 15 6" />
        </svg>
      </button>
      <button
        onClick={onExpand}
        className="flex size-8 items-center justify-center rounded-md text-primary"
        aria-label="Plan"
        title="Plan"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" /><path d="M14 2v6h6" /><path d="M16 13H8" /><path d="M16 17H8" /><path d="M10 9H8" />
        </svg>
      </button>
    </div>
  );
}

export function Workspace() {
  const [leftTab, setLeftTab] = useState<LeftTab>("tasks");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [leftCollapsed, setLeftCollapsed] = useState(false);

  const [planVisible, setPlanVisible] = useAtom(planVisibleAtom);
  const session = useAtomValue(activeSessionAtom);
  const leftPanelRef = usePanelRef();
  const planPanelRef = usePanelRef();

  function handleTogglePlan() {
    const panel = planPanelRef.current;
    if (!panel) return;
    if (planVisible) {
      panel.collapse();
    } else {
      panel.expand();
    }
  }

  function handleToggleLeft() {
    const panel = leftPanelRef.current;
    if (!panel) return;
    if (leftCollapsed) {
      panel.expand();
    } else {
      panel.collapse();
    }
  }

  return (
    <div className="flex h-full flex-col bg-background">
      <TopBar />

      <div className="flex flex-1 overflow-hidden p-1.5 gap-1.5">
        <NavSidebar onOpenSettings={() => setSettingsOpen(true)} />

        <ResizablePanelGroup orientation="horizontal" className="flex-1">
          {/* Left panel: Tasks / Files / Activity */}
          <ResizablePanel
            id="left"
            panelRef={leftPanelRef}
            defaultSize="20%"
            minSize="10%"
            maxSize="30%"
            collapsible
            collapsedSize={COLLAPSED_SIZE_PX}
            onResize={(size) => {
              setLeftCollapsed(size.inPixels <= COLLAPSED_SIZE_PX);
            }}
          >
            {leftCollapsed ? (
              <CollapsedLeftBar
                onExpand={handleToggleLeft}
                activeTab={leftTab}
              />
            ) : (
              <div className="flex h-full flex-col overflow-hidden rounded-xl bg-card">
                <div className="flex h-9 shrink-0 items-center border-b border-border/50">
                  <div className="flex flex-1 items-stretch h-full">
                    {(["tasks", "files", "activity"] as const).map((tab) => (
                      <button
                        key={tab}
                        onClick={() => setLeftTab(tab)}
                        className={`flex-1 text-xs font-medium capitalize transition-colors ${
                          leftTab === tab
                            ? "text-foreground"
                            : "text-muted-foreground hover:text-foreground"
                        }`}
                      >
                        {tab}
                      </button>
                    ))}
                  </div>
                  <button
                    onClick={handleToggleLeft}
                    className="flex size-6 shrink-0 mr-1.5 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
                    aria-label="Collapse sidebar"
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <polyline points="15 18 9 12 15 6" />
                    </svg>
                  </button>
                </div>
                <div className="flex-1 overflow-hidden">
                  {leftTab === "tasks" && <TaskPanel />}
                  {leftTab === "files" && <ModifiedFiles />}
                  {leftTab === "activity" && <ActivityLog />}
                </div>
              </div>
            )}
          </ResizablePanel>

          <ResizableHandle />

          {/* Center: Terminal */}
          <ResizablePanel id="terminal" defaultSize="50%" minSize="20%">
            <div className="flex h-full overflow-hidden rounded-xl bg-card">
              <TerminalPanel cwd={session?.cwd} />
            </div>
          </ResizablePanel>

          <ResizableHandle />

          {/* Right panel: Plan */}
          <ResizablePanel
            id="plan"
            panelRef={planPanelRef}
            defaultSize="30%"
            minSize="10%"
            maxSize="50%"
            collapsible
            collapsedSize={COLLAPSED_SIZE_PX}
            onResize={(size) => {
              setPlanVisible(size.inPixels > COLLAPSED_SIZE_PX);
            }}
          >
            {!planVisible ? (
              <CollapsedPlanBar onExpand={handleTogglePlan} />
            ) : (
              <div className="flex h-full overflow-hidden rounded-xl bg-card">
                <PlanPanel onCollapse={handleTogglePlan} />
              </div>
            )}
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>

      <StatusBar />

      <Toasts />
      <SettingsPanel open={settingsOpen} onOpenChange={setSettingsOpen} />
    </div>
  );
}
