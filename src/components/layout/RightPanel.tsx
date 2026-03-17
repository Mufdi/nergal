import { useState, useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeTabAtom, openTabsAtom, activeTabIdAtom, type RightPanelTab } from "@/stores/rightPanel";
import { activePlanAtom, planStateMapAtom, sessionPlansAtom } from "@/stores/plan";
import { activeSessionIdAtom } from "@/stores/workspace";
import { PlanPanel } from "@/components/plan/PlanPanel";
import { TranscriptViewer } from "@/components/session/TranscriptViewer";
import { invoke } from "@/lib/tauri";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

interface RightPanelProps {
  collapsed: boolean;
  onToggle: () => void;
}

export function RightPanel({ collapsed, onToggle }: RightPanelProps) {
  const activeTab = useAtomValue(activeTabAtom);
  const openTabs = useAtomValue(openTabsAtom);
  const setActiveTabId = useSetAtom(activeTabIdAtom);

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
        {openTabs.map((tab) => (
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
              <PanelTabIcon type={tab.type} />
            </TooltipTrigger>
            <TooltipContent side="left">{tab.label}</TooltipContent>
          </Tooltip>
        ))}
      </div>
    );
  }

  if (!activeTab) {
    return (
      <div className="flex h-full flex-col rounded-lg bg-card">
        <PanelHeader onToggle={onToggle} />
        <div className="flex flex-1 items-center justify-center">
          <span className="text-[11px] text-muted-foreground">No panel open</span>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full overflow-hidden rounded-lg bg-card">
      {/* Main content */}
      <div className="flex flex-1 flex-col overflow-hidden">
        <PanelHeader onToggle={onToggle} label={activeTab.label} />
        <div className="flex-1 overflow-y-auto">
          <PanelContent tab={activeTab} />
        </div>
      </div>

      {/* Plan file sidebar on the right */}
      {activeTab.type === "plan" && <PlanFileSidebar />}
    </div>
  );
}

function PanelHeader({ onToggle, label }: { onToggle: () => void; label?: string }) {
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
      {label && <span className="text-[11px] font-medium text-foreground/80">{label}</span>}
    </div>
  );
}

function PlanFileSidebar() {
  const [allPlans, setAllPlans] = useState<{ name: string; path: string }[]>([]);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const sessionPlans = useAtomValue(sessionPlansAtom);
  const plan = useAtomValue(activePlanAtom);
  const setPlanStateMap = useSetAtom(planStateMapAtom);

  // Session-specific plans from registerPlanAtom
  const plans = sessionId ? (sessionPlans[sessionId] ?? []) : [];

  // Also fetch all plans for the workspace (fallback when session has none registered yet)
  useEffect(() => {
    invoke<{ name: string; path: string; modified: number }[]>("list_plans")
      .then((result) => setAllPlans(result.map((p) => ({ name: p.name, path: p.path }))))
      .catch(() => {});
  }, [plan.path]);

  const displayPlans = plans.length > 0 ? plans : allPlans.slice(0, 20);
  const activeName = plan.path?.split("/").pop()?.replace(".md", "") ?? "";

  function handleSelect(path: string) {
    if (!sessionId) return;
    invoke<{ path: string; content: string }>("load_plan", { sessionId, path })
      .then((result) => {
        setPlanStateMap((prev) => ({
          ...prev,
          [sessionId]: {
            content: result.content,
            original: result.content,
            path: result.path,
            mode: "view" as const,
            diff: [],
          },
        }));
      })
      .catch(() => {});
  }

  if (displayPlans.length === 0) return null;

  return (
    <div className="flex w-10 shrink-0 flex-col items-center gap-0.5 border-l border-border/50 py-1.5 overflow-y-auto">
      {displayPlans.map((p) => {
        const isActive = p.name === activeName;
        return (
          <Tooltip key={p.path}>
            <TooltipTrigger
              render={
                <button
                  onClick={() => handleSelect(p.path)}
                  className={`flex size-7 items-center justify-center rounded transition-colors ${
                    isActive
                      ? "bg-secondary text-foreground"
                      : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
                  }`}
                  aria-label={p.name}
                />
              }
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" /><path d="M14 2v6h6" />
              </svg>
            </TooltipTrigger>
            <TooltipContent side="left" className="max-w-48 truncate">{p.name}</TooltipContent>
          </Tooltip>
        );
      })}
    </div>
  );
}

function PanelContent({ tab }: { tab: RightPanelTab }) {
  switch (tab.type) {
    case "plan":
      return <PlanPanel />;
    case "transcript":
      return tab.sessionId ? <TranscriptViewer sessionId={tab.sessionId} /> : null;
    case "diff":
      return <PlaceholderView label="Diff view" />;
    case "spec":
      return <PlaceholderView label="Spec view" />;
    case "git":
      return <PlaceholderView label="Git view" />;
    default:
      return null;
  }
}

function PlaceholderView({ label }: { label: string }) {
  return (
    <div className="flex h-full w-full items-center justify-center">
      <span className="text-[11px] text-muted-foreground">{label} — coming soon</span>
    </div>
  );
}

function PanelTabIcon({ type }: { type: string }) {
  switch (type) {
    case "plan":
      return (
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" /><path d="M14 2v6h6" /><path d="M16 13H8" /><path d="M16 17H8" /><path d="M10 9H8" />
        </svg>
      );
    default:
      return null;
  }
}
