import { useState, useEffect, useCallback } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  planDocumentsAtom,
  defaultPlanState,
  fetchPlanCapabilityAction,
} from "@/stores/plan";
import { activeSessionIdAtom } from "@/stores/workspace";
import { openTabAction, activeTabAtom } from "@/stores/rightPanel";
import { invoke } from "@/lib/tauri";
import { listen } from "@tauri-apps/api/event";
import type { PlanSummary, SessionPlansResponse } from "@/lib/types";
import { FileText } from "lucide-react";

type ListState =
  | { kind: "loading" }
  | { kind: "FileBased"; dir: string; plans: PlanSummary[] }
  | { kind: "NotApplicable" };

export function PlanListView() {
  const [listState, setListState] = useState<ListState>({ kind: "loading" });
  const sessionId = useAtomValue(activeSessionIdAtom);
  const activeTab = useAtomValue(activeTabAtom);
  const openTab = useSetAtom(openTabAction);
  const setPlanDocs = useSetAtom(planDocumentsAtom);
  const fetchCapability = useSetAtom(fetchPlanCapabilityAction);

  useEffect(() => {
    if (sessionId) fetchCapability(sessionId);
  }, [sessionId, fetchCapability]);

  const refetch = useCallback(() => {
    if (!sessionId) {
      setListState({ kind: "NotApplicable" });
      return;
    }
    invoke<SessionPlansResponse>("list_session_plans", { sessionId })
      .then((res) => {
        if (res.capability === "FileBased") {
          setListState({ kind: "FileBased", dir: res.dir, plans: res.plans });
        } else {
          setListState({ kind: "NotApplicable" });
        }
      })
      .catch(() => setListState({ kind: "NotApplicable" }));
  }, [sessionId]);

  useEffect(() => {
    setListState({ kind: "loading" });
    refetch();
  }, [refetch]);

  // Live refresh: plan watcher emits plan:event on create/modify. Broad
  // refetch covers both the "new plan landed" and "plan content changed"
  // cases without filtering by path (the panel only cares about the list).
  useEffect(() => {
    const unlisten = listen("plan:event", () => refetch());
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refetch]);

  function loadAndOpenPlan(path: string, name: string) {
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
        });
      })
      .catch(() => {});
  }

  if (listState.kind === "loading") {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[11px] text-muted-foreground">Loading plans…</span>
      </div>
    );
  }

  if (listState.kind === "NotApplicable") {
    return (
      <div className="flex h-full items-center justify-center px-4 text-center">
        <span className="text-[11px] text-muted-foreground">
          Plans are not available for this agent.
        </span>
      </div>
    );
  }

  if (listState.plans.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1.5 px-4 text-center">
        <span className="text-[11px] text-muted-foreground">No plans yet</span>
        <span className="text-[10px] text-muted-foreground/70">
          New plans will appear here automatically.
        </span>
        <span className="mt-1 text-[10px] font-mono text-muted-foreground/50 break-all">
          {listState.dir}
        </span>
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
      {listState.plans.map((plan) => {
        const isActive = activePath === plan.path;
        return (
          <button
            key={plan.path}
            data-nav-item
            onClick={() => loadAndOpenPlan(plan.path, plan.name)}
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

