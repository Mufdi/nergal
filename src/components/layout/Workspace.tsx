import { useState, useCallback } from "react";
import { useAtom, useAtomValue } from "jotai";
import { planVisibleAtom, planContentAtom } from "@/stores/plan";
import { activeSessionAtom } from "@/stores/session";
import { NavSidebar } from "./NavSidebar";
import { StatusBar } from "./StatusBar";
import { Toasts } from "./Toasts";
import { TerminalPanel } from "@/components/terminal/TerminalPanel";
import { TaskPanel } from "@/components/tasks/TaskPanel";
import { ActivityLog } from "@/components/activity/ActivityLog";
import { PlanPanel } from "@/components/plan/PlanPanel";
import { SettingsPanel } from "@/components/settings/SettingsPanel";

type LeftTab = "tasks" | "activity";

export function Workspace() {
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [leftTab, setLeftTab] = useState<LeftTab>("tasks");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [leftPanelWidth, setLeftPanelWidth] = useState(280);
  const [rightPanelWidth, setRightPanelWidth] = useState(420);

  const [planVisible, setPlanVisible] = useAtom(planVisibleAtom);
  const planContent = useAtomValue(planContentAtom);
  const session = useAtomValue(activeSessionAtom);

  const hasPlan = planContent.length > 0;

  const handleLeftResize = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = leftPanelWidth;

    function onMouseMove(ev: MouseEvent) {
      const delta = ev.clientX - startX;
      setLeftPanelWidth(Math.max(180, Math.min(500, startWidth + delta)));
    }

    function onMouseUp() {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    }

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, [leftPanelWidth]);

  const handleRightResize = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = rightPanelWidth;

    function onMouseMove(ev: MouseEvent) {
      const delta = startX - ev.clientX;
      setRightPanelWidth(Math.max(300, Math.min(700, startWidth + delta)));
    }

    function onMouseUp() {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    }

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, [rightPanelWidth]);

  return (
    <div className="flex h-full flex-col bg-surface">
      <div className="flex flex-1 overflow-hidden">
        <NavSidebar
          collapsed={sidebarCollapsed}
          onToggle={() => setSidebarCollapsed((prev) => !prev)}
        />

        <div className="flex flex-1 overflow-hidden">
          {/* Left panel: tasks/activity */}
          <div
            className="flex flex-shrink-0 flex-col border-r border-border"
            style={{ width: leftPanelWidth }}
          >
            <div className="flex h-8 border-b border-border">
              <button
                onClick={() => setLeftTab("tasks")}
                className={`flex-1 text-xs ${
                  leftTab === "tasks" ? "bg-surface-raised text-text" : "text-text-muted hover:text-text"
                }`}
              >
                Tasks
              </button>
              <button
                onClick={() => setLeftTab("activity")}
                className={`flex-1 text-xs ${
                  leftTab === "activity" ? "bg-surface-raised text-text" : "text-text-muted hover:text-text"
                }`}
              >
                Activity
              </button>
              <button
                onClick={() => setSettingsOpen(true)}
                className="flex w-8 items-center justify-center text-xs text-text-muted hover:text-text"
                aria-label="Open settings"
              >
                *
              </button>
            </div>
            <div className="flex-1 overflow-hidden">
              {leftTab === "tasks" ? <TaskPanel /> : <ActivityLog />}
            </div>
          </div>

          {/* Left drag handle */}
          <div
            className="w-1 cursor-col-resize bg-border/50 hover:bg-accent/50"
            onMouseDown={handleLeftResize}
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize left panel"
          />

          {/* Center: terminal */}
          <main className="flex flex-1 overflow-hidden">
            <TerminalPanel cwd={session?.cwd} />
          </main>

          {/* Right panel: plan (conditional, resizable) */}
          {planVisible && (
            <>
              <div
                className="w-1 cursor-col-resize bg-border/50 hover:bg-accent/50"
                onMouseDown={handleRightResize}
                role="separator"
                aria-orientation="vertical"
                aria-label="Resize plan panel"
              />
              <div
                className="flex-shrink-0"
                style={{ width: rightPanelWidth }}
              >
                <PlanPanel />
              </div>
            </>
          )}
        </div>
      </div>

      <StatusBar
        hasPlan={hasPlan}
        planVisible={planVisible}
        onTogglePlan={() => setPlanVisible((v) => !v)}
      />

      <Toasts />

      {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
