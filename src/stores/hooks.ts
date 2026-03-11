import { type getDefaultStore } from "jotai";
import { listen } from "@/lib/tauri";
import type { HookEvent, SessionInfo, CostSummary, Task, ActivityEntry } from "@/lib/types";
import { sessionsAtom, activeSessionIndexAtom, sessionModeAtom, costSummaryAtom } from "./session";
import { taskMapAtom } from "./tasks";
import { fileMapAtom, type ModifiedFile } from "./files";
import { planContentAtom, planOriginalAtom, planPathAtom, planDiffAtom, planVisibleAtom, planModeAtom } from "./plan";
import { activityAtom } from "./activity";
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
  const unlisteners: UnlistenFn[] = [];

  // Hook events from Claude CLI (flat structure from backend)
  unlisteners.push(
    await listen<HookEvent>("hook:event", (event) => {
      const { event_type, tool_name, session_id, stop_reason } = event;

      switch (event_type) {
        case "pre_tool_use": {
          set(sessionModeAtom, tool_name ?? "tool");
          set(activityAtom, createActivity("tool_use", `Tool: ${tool_name ?? "unknown"}`, session_id));
          break;
        }
        case "post_tool_use": {
          set(sessionModeAtom, "active");
          set(activityAtom, createActivity("tool_use", `Tool done: ${tool_name ?? "unknown"}`, session_id));
          break;
        }
        case "stop": {
          set(sessionModeAtom, "idle");
          set(activityAtom, createActivity("session", `Stopped: ${stop_reason ?? "completed"}`));
          break;
        }
        case "task_completed": {
          set(activityAtom, createActivity("task", `Task completed: ${tool_name ?? "unknown"}`));
          break;
        }
        case "session_start": {
          set(sessionModeAtom, "active");
          set(activityAtom, createActivity("session", `Session started: ${session_id.slice(0, 8)}`));
          break;
        }
        case "session_end": {
          set(sessionModeAtom, "idle");
          set(activityAtom, createActivity("session", `Session ended: ${session_id.slice(0, 8)}`));
          break;
        }
        case "user_prompt_submit": {
          set(activityAtom, createActivity("session", "User prompt submitted"));
          break;
        }
        default: {
          set(activityAtom, createActivity("session", `Event: ${event_type}`));
        }
      }
    }),
  );

  // Session start (emitted separately by backend with full SessionInfo)
  unlisteners.push(
    await listen<SessionInfo>("session:start", (session) => {
      set(sessionsAtom, (prev) => {
        const exists = prev.find((s) => s.id === session.id);
        if (exists) {
          return prev.map((s) => (s.id === session.id ? { ...s, active: true } : s));
        }
        return [...prev, session];
      });
      set(activeSessionIndexAtom, () => {
        const sessions = store.get(sessionsAtom);
        return sessions.length > 0 ? sessions.length - 1 : 0;
      });
    }),
  );

  // Session end
  unlisteners.push(
    await listen<string>("session:end", (sessionId) => {
      set(sessionsAtom, (prev) =>
        prev.map((s) => (s.id === sessionId ? { ...s, active: false } : s)),
      );
    }),
  );

  // Cost updates
  unlisteners.push(
    await listen<CostSummary>("cost:update", (cost) => {
      set(costSummaryAtom, cost);
    }),
  );

  // Task list updates (emitted by backend after processing TaskCreate/TaskUpdate)
  unlisteners.push(
    await listen<{ session_id: string; tasks: Task[] }>("tasks:update", (payload) => {
      set(taskMapAtom, (prev) => ({
        ...prev,
        [payload.session_id]: payload.tasks,
      }));
    }),
  );

  // File modifications (emitted by backend after Write/Edit/MultiEdit)
  unlisteners.push(
    await listen<{ session_id: string; path: string; tool: string }>("files:modified", (payload) => {
      const entry: ModifiedFile = {
        path: payload.path,
        tool: payload.tool,
        timestamp: Date.now(),
      };
      set(fileMapAtom, (prev) => {
        const existing = prev[payload.session_id] ?? [];
        // Dedupe by path — keep latest
        const filtered = existing.filter((f) => f.path !== payload.path);
        return { ...prev, [payload.session_id]: [...filtered, entry] };
      });
      set(activityAtom, createActivity("file_modified", `Modified: ${payload.path.split("/").pop() ?? payload.path}`, payload.path));
    }),
  );

  // Plan ready (emitted by backend after detecting ExitPlanMode)
  unlisteners.push(
    await listen<{ path: string; content: string }>("plan:ready", (payload) => {
      set(planPathAtom, payload.path);
      set(planContentAtom, payload.content);
      set(planOriginalAtom, payload.content);
      set(planModeAtom, "view");
      set(planVisibleAtom, true);
      set(planDiffAtom, []);
      set(activityAtom, createActivity("plan", "Plan ready for review"));
    }),
  );

  return unlisteners;
}
