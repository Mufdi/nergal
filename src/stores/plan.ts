import { atom } from "jotai";
import type { PlanMode, DiffLine } from "@/lib/types";
import { activeSessionIdAtom } from "./workspace";

export type PlanSidebarTab = "files" | "annotations";
export const planSidebarTabAtom = atom<PlanSidebarTab>("files");

export interface PlanState {
  content: string;
  original: string;
  path: string;
  mode: PlanMode;
  diff: DiffLine[];
  claudeSessionId: string;
}

export const defaultPlanState: PlanState = { content: "", original: "", path: "", mode: "view", diff: [], claudeSessionId: "" };

export const planStateMapAtom = atom<Record<string, PlanState>>({});

// Derived for active session
export const activePlanAtom = atom<PlanState>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return defaultPlanState;
  return get(planStateMapAtom)[id] ?? defaultPlanState;
});

// Write atoms for updating active session's plan
export const setPlanContentAtom = atom(null, (get, set, content: string) => {
  const id = get(activeSessionIdAtom);
  if (!id) return;
  set(planStateMapAtom, (prev) => ({
    ...prev,
    [id]: { ...(prev[id] ?? defaultPlanState), content },
  }));
});

export const setPlanModeAtom = atom(null, (get, set, mode: PlanMode) => {
  const id = get(activeSessionIdAtom);
  if (!id) return;
  set(planStateMapAtom, (prev) => ({
    ...prev,
    [id]: { ...(prev[id] ?? defaultPlanState), mode },
  }));
});

// Plan documents keyed by file path (supports multiple plans open as document tabs)
export const planDocumentsAtom = atom<Record<string, PlanState>>({});

export const setPlanDocContentAtom = atom(null, (_get, set, params: { path: string; content: string }) => {
  set(planDocumentsAtom, (prev) => ({
    ...prev,
    [params.path]: { ...(prev[params.path] ?? defaultPlanState), content: params.content },
  }));
});

export const setPlanDocModeAtom = atom(null, (_get, set, params: { path: string; mode: PlanMode }) => {
  set(planDocumentsAtom, (prev) => ({
    ...prev,
    [params.path]: { ...(prev[params.path] ?? defaultPlanState), mode: params.mode },
  }));
});

// Session plans registry
export interface SessionPlan {
  path: string;
  name: string;
}

export const sessionPlansAtom = atom<Record<string, SessionPlan[]>>({});

export const registerPlanAtom = atom(
  null,
  (_get, set, payload: { sessionId: string; path: string }) => {
    const name = payload.path.split("/").pop()?.replace(".md", "") ?? payload.path;
    set(sessionPlansAtom, (prev) => {
      const existing = prev[payload.sessionId] ?? [];
      if (existing.some((p) => p.path === payload.path)) return prev;
      return { ...prev, [payload.sessionId]: [...existing, { path: payload.path, name }] };
    });
  },
);
