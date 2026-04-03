import { type getDefaultStore } from "jotai";
import { listen } from "@/lib/tauri";
import type { HookEvent, CostSummary, Task, ActivityEntry } from "@/lib/types";
import { costMapAtom, modeMapAtom, cwdMapAtom, statusLineMapAtom, activeSessionIdAtom, type StatusLineData } from "./workspace";
import { taskMapAtom } from "./tasks";
import { fileMapAtom, type ModifiedFile } from "./files";
import { planStateMapAtom, planDocumentsAtom, registerPlanAtom, planReviewStatusMapAtom } from "./plan";
import { askUserAtom } from "./askUser";
import { openTabAction, expandRightPanelAtom, activePanelViewAtom } from "./rightPanel";
import { refreshGitInfoAtom } from "./git";
import { addActivityAtom } from "./activity";
import { toastsAtom } from "./toast";
import { notify } from "@/lib/notifications";
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
      if (!event.cluihud_session_id) return;
      const sid = event.cluihud_session_id;

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
          set(planReviewStatusMapAtom, (prev) => ({ ...prev, [sid]: "idle" }));
          set(addActivityAtom, { sessionId: sid, entry: createActivity("session", `Stopped: ${stop_reason ?? "completed"}`) });
          set(refreshGitInfoAtom, sid);
          notify("Claude stopped", stop_reason ?? "completed");
          break;
        }
        case "task_completed": {
          set(addActivityAtom, { sessionId: sid, entry: createActivity("task", `Task completed: ${tool_name ?? "unknown"}`) });
          notify("Task completed", tool_name ?? "unknown");
          break;
        }
        case "session_start": {
          set(modeMapAtom, (prev) => ({ ...prev, [sid]: "active" }));
          set(addActivityAtom, { sessionId: sid, entry: createActivity("session", "Session started") });
          break;
        }
        case "session_end": {
          set(modeMapAtom, (prev) => ({ ...prev, [sid]: "idle" }));
          set(planReviewStatusMapAtom, (prev) => ({ ...prev, [sid]: "idle" }));
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
      const sid = payload.session_id;
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
    await listen<{ path: string; content: string; session_id: string; decision_path?: string }>("plan:ready", (payload) => {
      const sid = get(activeSessionIdAtom);
      if (!sid) return;
      const planData = {
        content: payload.content,
        original: payload.content,
        path: payload.path,
        mode: "view" as const,
        diff: [] as never[],
        claudeSessionId: payload.session_id,
        decisionPath: payload.decision_path ?? "",
      };
      set(planStateMapAtom, (prev) => ({ ...prev, [sid]: planData }));
      set(planDocumentsAtom, (prev) => ({ ...prev, [payload.path]: planData }));
      const planName = payload.path.split("/").pop()?.replace(".md", "") ?? "Plan";
      set(openTabAction, { tab: { id: `plan-${payload.path}`, type: "plan", label: planName, data: { path: payload.path } }, isPinned: true });
      set(registerPlanAtom, { sessionId: sid, path: payload.path });
      set(activePanelViewAtom, "plan");
      set(expandRightPanelAtom, (prev: number) => prev + 1);
      set(planReviewStatusMapAtom, (prev) => ({ ...prev, [sid]: "pending_review" }));
      set(addActivityAtom, { sessionId: sid, entry: createActivity("plan", "Plan ready for review") });
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string; questions: Array<{ question: string; header: string; options: string[]; multi_select: boolean }>; decision_path: string }>("ask:user", (payload) => {
      const sid = get(activeSessionIdAtom);
      if (!sid) return;
      set(askUserAtom, {
        questions: payload.questions,
        decisionPath: payload.decision_path,
        sessionId: payload.session_id,
      });
      const firstQ = payload.questions[0]?.question ?? "Claude needs input";
      set(addActivityAtom, { sessionId: sid, entry: createActivity("session", "Claude is asking a question") });
      notify("Claude needs input", firstQ);
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string; cwd: string }>("cwd:changed", (payload) => {
      const sid = get(activeSessionIdAtom);
      if (!sid) return;
      set(cwdMapAtom, (prev) => ({ ...prev, [sid]: payload.cwd }));
      set(addActivityAtom, { sessionId: sid, entry: createActivity("session", `cwd: ${payload.cwd}`) });
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string; path: string; change_type: string }>("file:changed", (payload) => {
      const sid = get(activeSessionIdAtom);
      if (!sid) return;
      const filename = payload.path.split("/").pop() ?? payload.path;
      set(addActivityAtom, { sessionId: sid, entry: createActivity("file_modified", `Changed: ${filename}`, payload.path) });
    }),
  );

  unlisteners.push(
    await listen<StatusLineData & { session_id: string }>("statusline:update", (payload) => {
      const sid = get(activeSessionIdAtom);
      if (!sid) return;
      const { session_id: _, ...data } = payload;
      set(statusLineMapAtom, (prev) => ({ ...prev, [sid]: data }));
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string; tool_name?: string; reason?: string }>("permission:denied", (payload) => {
      const sid = get(activeSessionIdAtom);
      if (!sid) return;
      const tool = payload.tool_name ?? "unknown tool";
      const reason = payload.reason ?? "Auto-mode denied this action";
      set(toastsAtom, { message: `Permission denied: ${tool}`, description: reason, type: "error" });
      set(addActivityAtom, { sessionId: sid, entry: createActivity("session", `Permission denied: ${tool}`, reason) });
      notify("Permission denied", `${tool}: ${reason}`);
    }),
  );

  return unlisteners;
}
