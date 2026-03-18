import { type getDefaultStore } from "jotai";
import { listen } from "@/lib/tauri";
import type { HookEvent, CostSummary, Task, ActivityEntry } from "@/lib/types";
import { costMapAtom, modeMapAtom, activeSessionIdAtom } from "./workspace";
import { taskMapAtom } from "./tasks";
import { fileMapAtom, type ModifiedFile } from "./files";
import { planStateMapAtom, planDocumentsAtom, registerPlanAtom } from "./plan";
import { openTabAction, expandRightPanelAtom, activePanelViewAtom } from "./rightPanel";
import { refreshGitInfoAtom } from "./git";
import { addActivityAtom } from "./activity";
import type { UnlistenFn } from "@tauri-apps/api/event";

type Store = ReturnType<typeof getDefaultStore>;

function createActivity(type: ActivityEntry["type"], message: string, detail?: string): ActivityEntry {
  return {
    id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    timestamp: Date.now(),
    type,
    message,
    detail,
  };
}

export async function setupHookListeners(store: Store): Promise<UnlistenFn[]> {
  const set = store.set;
  const get = store.get;
  const unlisteners: UnlistenFn[] = [];

  unlisteners.push(
    await listen<HookEvent>("hook:event", (event) => {
      const { event_type, tool_name, stop_reason } = event;
      const sid = event.cluihud_session_id ?? get(activeSessionIdAtom);
      if (!sid) return;

      switch (event_type) {
        case "pre_tool_use": {
          set(modeMapAtom, (prev) => ({ ...prev, [sid]: tool_name ?? "tool" }));
          set(addActivityAtom, { sessionId: sid, entry: createActivity("tool_use", `Tool: ${tool_name ?? "unknown"}`) });
          break;
        }
        case "post_tool_use": {
          set(modeMapAtom, (prev) => ({ ...prev, [sid]: "active" }));
          set(addActivityAtom, { sessionId: sid, entry: createActivity("tool_use", `Tool done: ${tool_name ?? "unknown"}`) });
          set(refreshGitInfoAtom, sid);
          break;
        }
        case "stop": {
          set(modeMapAtom, (prev) => ({ ...prev, [sid]: "idle" }));
          set(addActivityAtom, { sessionId: sid, entry: createActivity("session", `Stopped: ${stop_reason ?? "completed"}`) });
          set(refreshGitInfoAtom, sid);
          break;
        }
        case "task_completed": {
          set(addActivityAtom, { sessionId: sid, entry: createActivity("task", `Task completed: ${tool_name ?? "unknown"}`) });
          break;
        }
        case "session_start": {
          set(modeMapAtom, (prev) => ({ ...prev, [sid]: "active" }));
          set(addActivityAtom, { sessionId: sid, entry: createActivity("session", "Session started") });
          break;
        }
        case "session_end": {
          set(modeMapAtom, (prev) => ({ ...prev, [sid]: "idle" }));
          set(addActivityAtom, { sessionId: sid, entry: createActivity("session", "Session ended") });
          break;
        }
        case "user_prompt_submit": {
          set(addActivityAtom, { sessionId: sid, entry: createActivity("session", "User prompt submitted") });
          break;
        }
        default: {
          set(addActivityAtom, { sessionId: sid, entry: createActivity("session", `Event: ${event_type}`) });
        }
      }
    }),
  );

  unlisteners.push(
    await listen<CostSummary & { session_id: string }>("cost:update", (_payload) => {
      const sid = get(activeSessionIdAtom);
      if (!sid) return;
      const { session_id: _, ...cost } = _payload;
      set(costMapAtom, (prev) => ({ ...prev, [sid]: cost }));
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string; tasks: Task[] }>("tasks:update", (payload) => {
      const sid = get(activeSessionIdAtom);
      if (!sid) return;
      set(taskMapAtom, (prev) => ({
        ...prev,
        [sid]: payload.tasks,
      }));
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string; path: string; tool: string }>("files:modified", (payload) => {
      const sid = get(activeSessionIdAtom);
      if (!sid) return;
      const entry: ModifiedFile = {
        path: payload.path,
        tool: payload.tool,
        timestamp: Date.now(),
      };
      set(fileMapAtom, (prev) => {
        const existing = prev[sid] ?? [];
        const filtered = existing.filter((f) => f.path !== payload.path);
        return { ...prev, [sid]: [...filtered, entry] };
      });
      set(addActivityAtom, {
        sessionId: sid,
        entry: createActivity("file_modified", `Modified: ${payload.path.split("/").pop() ?? payload.path}`, payload.path),
      });
    }),
  );

  unlisteners.push(
    await listen<{ path: string; content: string; session_id: string }>("plan:ready", (payload) => {
      const sid = get(activeSessionIdAtom);
      if (!sid) return;
      const planData = {
        content: payload.content,
        original: payload.content,
        path: payload.path,
        mode: "view" as const,
        diff: [] as never[],
        claudeSessionId: payload.session_id,
      };
      set(planStateMapAtom, (prev) => ({ ...prev, [sid]: planData }));
      set(planDocumentsAtom, (prev) => ({ ...prev, [payload.path]: planData }));
      const planName = payload.path.split("/").pop()?.replace(".md", "") ?? "Plan";
      set(openTabAction, { tab: { id: `plan-${payload.path}`, type: "plan", label: planName, data: { path: payload.path } }, isPinned: true });
      set(registerPlanAtom, { sessionId: sid, path: payload.path });
      set(activePanelViewAtom, "plan");
      set(expandRightPanelAtom, (prev: number) => prev + 1);
      set(addActivityAtom, { sessionId: sid, entry: createActivity("plan", "Plan ready for review") });
    }),
  );

  return unlisteners;
}
