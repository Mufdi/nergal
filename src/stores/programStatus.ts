import { sessionScopedMapAtom } from "./sessionScope";

export type RootState = "idle" | "working" | "blocked" | "done" | "error";

export interface ProgramStatusEntry {
  state: RootState;
  kind: string | null;
  msg: string | null;
}

export interface ProgramStatusPayload {
  session_id: string;
  state: RootState | null;
  kind: string | null;
  msg: string | null;
}

export const programStatusMapAtom = sessionScopedMapAtom<ProgramStatusEntry>();

export function applyProgramStatus(
  prev: Record<string, ProgramStatusEntry>,
  payload: ProgramStatusPayload,
): Record<string, ProgramStatusEntry> {
  const { session_id: sid, state, kind, msg } = payload;
  if (state === null) {
    if (!(sid in prev)) return prev;
    const { [sid]: _removed, ...rest } = prev;
    return rest;
  }
  const current = prev[sid];
  if (current && current.state === state && current.kind === kind && current.msg === msg) return prev;
  return { ...prev, [sid]: { state, kind, msg } };
}
