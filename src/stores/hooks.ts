import { type getDefaultStore } from "jotai";
import { listen } from "@/lib/tauri";
import type { HookEvent, CostSummary, Task, ActivityEntry } from "@/lib/types";
import { costMapAtom, modeMapAtom, workspacesAtom, activeSessionIdAtom } from "./workspace";
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

  // Hook events from Claude CLI (flat structure from backend)
  unlisteners.push(
    await listen<HookEvent>("hook:event", (event) => {
      const { event_type, tool_name, session_id, stop_reason } = event;

      switch (event_type) {
        case "pre_tool_use": {
          set(modeMapAtom, (prev) => ({ ...prev, [session_id]: tool_name ?? "tool" }));
          set(addActivityAtom, { sessionId: session_id, entry: createActivity("tool_use", `Tool: ${tool_name ?? "unknown"}`, session_id) });
          break;
        }
        case "post_tool_use": {
          set(modeMapAtom, (prev) => ({ ...prev, [session_id]: "active" }));
          set(addActivityAtom, { sessionId: session_id, entry: createActivity("tool_use", `Tool done: ${tool_name ?? "unknown"}`, session_id) });
          set(refreshGitInfoAtom, session_id);
          break;
        }
        case "stop": {
          set(modeMapAtom, (prev) => ({ ...prev, [session_id]: "idle" }));
          set(addActivityAtom, { sessionId: session_id, entry: createActivity("session", `Stopped: ${stop_reason ?? "completed"}`) });
          set(refreshGitInfoAtom, session_id);
          break;
        }
        case "task_completed": {
          set(addActivityAtom, { sessionId: session_id, entry: createActivity("task", `Task completed: ${tool_name ?? "unknown"}`) });
          break;
        }
        case "session_start": {
          set(modeMapAtom, (prev) => ({ ...prev, [session_id]: "active" }));
          // Update session status in workspaces if it exists
          set(workspacesAtom, (prev) =>
            prev.map((ws) => ({
              ...ws,
              sessions: ws.sessions.map((s) =>
                s.id === session_id ? { ...s, status: "running" as const, updated_at: Math.floor(Date.now() / 1000) } : s,
              ),
            })),
          );
          set(addActivityAtom, { sessionId: session_id, entry: createActivity("session", `Session started: ${session_id.slice(0, 8)}`) });
          break;
        }
        case "session_end": {
          set(modeMapAtom, (prev) => ({ ...prev, [session_id]: "idle" }));
          set(workspacesAtom, (prev) =>
            prev.map((ws) => ({
              ...ws,
              sessions: ws.sessions.map((s) =>
                s.id === session_id ? { ...s, status: "idle" as const, updated_at: Math.floor(Date.now() / 1000) } : s,
              ),
            })),
          );
          set(addActivityAtom, { sessionId: session_id, entry: createActivity("session", `Session ended: ${session_id.slice(0, 8)}`) });
          break;
        }
        case "user_prompt_submit": {
          set(addActivityAtom, { sessionId: session_id, entry: createActivity("session", "User prompt submitted") });
          break;
        }
        default: {
          set(addActivityAtom, { sessionId: session_id, entry: createActivity("session", `Event: ${event_type}`) });
        }
      }
    }),
  );

  // Cost updates (now includes session_id from backend)
  unlisteners.push(
    await listen<CostSummary & { session_id: string }>("cost:update", (payload) => {
      const { session_id, ...cost } = payload;
      set(costMapAtom, (prev) => ({ ...prev, [session_id]: cost }));
    }),
  );

  // Task list updates (already session-scoped)
  unlisteners.push(
    await listen<{ session_id: string; tasks: Task[] }>("tasks:update", (payload) => {
      set(taskMapAtom, (prev) => ({
        ...prev,
        [payload.session_id]: payload.tasks,
      }));
    }),
  );

  // File modifications (already session-scoped)
  unlisteners.push(
    await listen<{ session_id: string; path: string; tool: string }>("files:modified", (payload) => {
      const entry: ModifiedFile = {
        path: payload.path,
        tool: payload.tool,
        timestamp: Date.now(),
      };
      set(fileMapAtom, (prev) => {
        const existing = prev[payload.session_id] ?? [];
        const filtered = existing.filter((f) => f.path !== payload.path);
        return { ...prev, [payload.session_id]: [...filtered, entry] };
      });
      set(addActivityAtom, {
        sessionId: payload.session_id,
        entry: createActivity("file_modified", `Modified: ${payload.path.split("/").pop() ?? payload.path}`, payload.path),
      });
    }),
  );

  // Plan ready (emitted by backend after detecting ExitPlanMode)
  unlisteners.push(
    await listen<{ path: string; content: string; session_id: string }>("plan:ready", (payload) => {
      const cluihudSessionId = get(activeSessionIdAtom);
      if (!cluihudSessionId) return;
      const planData = {
        content: payload.content,
        original: payload.content,
        path: payload.path,
        mode: "view" as const,
        diff: [] as never[],
        claudeSessionId: payload.session_id,
      };
      set(planStateMapAtom, (prev) => ({ ...prev, [cluihudSessionId]: planData }));
      set(planDocumentsAtom, (prev) => ({ ...prev, [payload.path]: planData }));
      const planName = payload.path.split("/").pop()?.replace(".md", "") ?? "Plan";
      set(openTabAction, { tab: { id: `plan-${payload.path}`, type: "plan", label: planName, data: { path: payload.path } }, isPinned: true });
      set(registerPlanAtom, { sessionId: cluihudSessionId, path: payload.path });
      set(activePanelViewAtom, "plan");
      set(expandRightPanelAtom, (prev: number) => prev + 1);
      set(addActivityAtom, { sessionId: cluihudSessionId, entry: createActivity("plan", "Plan ready for review") });
    }),
  );

  return unlisteners;
}
