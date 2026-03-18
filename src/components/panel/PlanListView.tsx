import { useState, useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { sessionPlansAtom, planDocumentsAtom, defaultPlanState } from "@/stores/plan";
import { activeSessionIdAtom } from "@/stores/workspace";
import { openTabAction, activeTabAtom } from "@/stores/rightPanel";
import { invoke } from "@/lib/tauri";
import { FileText } from "lucide-react";

export function PlanListView() {
  const [diskPlans, setDiskPlans] = useState<{ name: string; path: string }[]>([]);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const sessionPlans = useAtomValue(sessionPlansAtom);
  const activeTab = useAtomValue(activeTabAtom);
  const openTab = useSetAtom(openTabAction);
  const setPlanDocs = useSetAtom(planDocumentsAtom);

  const registeredPlans = sessionId ? (sessionPlans[sessionId] ?? []) : [];

  useEffect(() => {
    if (!sessionId) return;
    invoke<{ name: string; path: string; modified: number }[]>("list_session_plans", { sessionId })
      .then((result) => setDiskPlans(result.map((p) => ({ name: p.name, path: p.path }))))
      .catch(() => setDiskPlans([]));
  }, [sessionId]);

  const seen = new Set<string>();
  const allPlans: { name: string; path: string }[] = [];
  for (const p of registeredPlans) {
    if (!seen.has(p.path)) {
      seen.add(p.path);
      allPlans.push(p);
    }
  }
  for (const p of diskPlans) {
    if (!seen.has(p.path)) {
      seen.add(p.path);
      allPlans.push(p);
    }
  }

  function loadAndOpenPlan(path: string, name: string, pinned: boolean) {
    invoke<{ path: string; content: string; has_edits: boolean }>("load_plan", { sessionId, path })
      .then((result) => {
        setPlanDocs((prev) => ({
          ...prev,
          [result.path]: {
            ...(prev[result.path] ?? defaultPlanState),
            content: result.content,
            original: result.content,
            path: result.path,
          },
        }));
        openTab({
          tab: { id: `plan-${result.path}`, type: "plan", label: name, data: { path: result.path } },
          isPinned: pinned,
        });
      })
      .catch(() => {});
  }

  if (allPlans.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[11px] text-muted-foreground">No plans found</span>
      </div>
    );
  }

  const activePath = activeTab?.data?.path as string | undefined;

  return (
    <div className="flex flex-col py-1">
      <div className="px-3 py-1.5">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Plans
        </span>
      </div>
      {allPlans.map((plan) => {
        const isActive = activePath === plan.path;
        return (
          <button
            key={plan.path}
            onClick={() => loadAndOpenPlan(plan.path, plan.name, false)}
            onDoubleClick={() => loadAndOpenPlan(plan.path, plan.name, true)}
            className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-[11px] transition-colors ${
              isActive
                ? "bg-secondary/70 text-foreground"
                : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
            }`}
            aria-label={plan.name}
          >
            <FileText size={13} className="shrink-0" />
            <span className="truncate">{plan.name}</span>
          </button>
        );
      })}
    </div>
  );
}
