import { atom } from "jotai";
import type { PlanMode, DiffLine } from "@/lib/types";
import { activeSessionIdAtom } from "./workspace";

export interface PlanState {
  content: string;
  original: string;
  path: string;
  mode: PlanMode;
  diff: DiffLine[];
}

const defaultPlanState: PlanState = { content: "", original: "", path: "", mode: "view", diff: [] };

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
