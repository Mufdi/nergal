import { applyPlanReady, applyReviewClosed, documentBase } from "@/stores/planReady";
import { type getDefaultStore } from "jotai";
import { invoke, listen } from "@/lib/tauri";
import type { HookEvent, CostSummary, Task, ActivityEntry } from "@/lib/types";
import { costMapAtom, modeMapAtom, cwdMapAtom, agentStatusMapAtom, activeSessionIdAtom, lastResponseEffortMapAtom, type AgentStatus } from "./workspace";
import { taskMapAtom } from "./tasks";
import { applyProgramStatus, programStatusMapAtom, type ProgramStatusPayload } from "./programStatus";
import { fileMapAtom, type ModifiedFile } from "./files";
import { planStateMapAtom, planDocumentsAtom, PLAN_DOCUMENTS_MAX, registerPlanAtom, planReviewStatusMapAtom } from "./plan";
import { withBoundedEntry } from "@/lib/boundedRecord";
import { pendingAsksAtom, pendingAttentionAtom } from "./askUser";
import { openTabAction, expandRightPanelAtom, activePanelViewAtom } from "./rightPanel";
import { refreshGitInfoAtom, prSignalMapAtom } from "./git";
import { prSignature } from "./prSignal";
import {
  addActivityAtom,
  completeToolActivityAtom,
  startSubagentActivityAtom,
  completeSubagentActivityAtom,
  closeUnclaimedSubagentRowsAtom,
  closeMainToolRowsAtom,
} from "./activity";
import { buildToolUseEntry, handbackReport, notificationAttentionMessage, shouldLogGenericActivity, shouldClearOnBackgroundCompletion, shouldClearOnTurnEvent } from "@/lib/agentTelemetry";
import { toastsAtom } from "./toast";
import { KITTY_CAPPED_DESCRIPTION, KITTY_CAPPED_TITLE, kittyCappedNotice } from "./kittyCapped";
import { localhostPortsAtom } from "./browser";
import { providerStatusAtom, type ProviderStatus } from "./statusFeed";
import { loadWorktreeRequests } from "./worktreeGate";
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

/// Extract the file a tool acts on from its tool_input (Edit/Write/Read/…).
function filesFromToolInput(input: unknown): string[] {
  if (!input || typeof input !== "object") return [];
  const obj = input as Record<string, unknown>;
  const p = obj.file_path ?? obj.path ?? obj.notebook_path;
  return typeof p === "string" && p ? [p] : [];
}

/// The shell command a Bash tool ran, so the Activities panel shows the actual
/// command instead of a bare "Bash".
function commandFromToolInput(input: unknown): string | undefined {
  if (!input || typeof input !== "object") return undefined;
  const c = (input as Record<string, unknown>).command;
  return typeof c === "string" && c ? c : undefined;
}

/// Best-effort human labels for CC's background_tasks / session_crons objects
/// (CC-defined shapes) — tries common descriptive keys, joins for the detail.
function summarizeItems(items: Record<string, unknown>[] | undefined, keys: string[]): string | undefined {
  if (!items || items.length === 0) return undefined;
  const labels = items
    .map((it) => {
      for (const k of keys) {
        const v = it[k];
        if (typeof v === "string" && v) return v;
      }
      return null;
    })
    .filter((l): l is string => Boolean(l));
  return labels.length ? labels.join(", ") : undefined;
}

function clearKey<T extends Record<string, unknown>>(prev: T, key: string): T {
  if (!(key in prev)) return prev;
  const next = { ...prev };
  delete next[key];
  return next;
}

export async function setupHookListeners(store: Store): Promise<UnlistenFn[]> {
  const set = store.set;
  const get = store.get;
  const unlisteners: UnlistenFn[] = [];

  unlisteners.push(
    await listen<HookEvent>("hook:event", (event) => {
      const { event_type, tool_name, stop_reason } = event;
      if (!event.nergal_session_id) return;
      const sid = event.nergal_session_id;

      switch (event_type) {
        case "pre_tool_use": {
          set(modeMapAtom, (prev) => ({ ...prev, [sid]: tool_name ?? "tool" }));
          set(addActivityAtom, {
            sessionId: sid,
            entry: buildToolUseEntry({
              id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
              timestamp: Date.now(),
              toolName: tool_name,
              command: commandFromToolInput(event.tool_input),
              files: filesFromToolInput(event.tool_input),
              agentId: event.agent_id,
              agentType: event.agent_type,
              detail: handbackReport(tool_name, event.tool_input),
            }),
          });
          break;
        }
        case "post_tool_use": {
          set(modeMapAtom, (prev) => ({ ...prev, [sid]: "active" }));
          // Pair with the running PreToolUse entry (stamps duration + done).
          // Fall back to a standalone done entry if the pre was pruned/missing.
          const paired = set(completeToolActivityAtom, { sessionId: sid, toolName: tool_name });
          if (!paired) {
            set(addActivityAtom, {
              sessionId: sid,
              entry: {
                ...createActivity("tool_use", tool_name ?? "tool"),
                status: "done",
                toolName: tool_name,
                command: commandFromToolInput(event.tool_input),
                files: filesFromToolInput(event.tool_input),
              },
            });
          }
          set(refreshGitInfoAtom, sid);
          if (shouldClearOnTurnEvent(get(pendingAttentionAtom)[sid])) {
            set(pendingAttentionAtom, (prev) => clearKey(prev, sid));
          }
          break;
        }
        case "stop": {
          set(modeMapAtom, (prev) => ({ ...prev, [sid]: "idle" }));
          set(planReviewStatusMapAtom, (prev) => (prev[sid] === "answered" ? prev : { ...prev, [sid]: "idle" }));
          set(closeUnclaimedSubagentRowsAtom, sid);
          set(closeMainToolRowsAtom, sid);
          if (shouldClearOnTurnEvent(get(pendingAttentionAtom)[sid])) {
            set(pendingAttentionAtom, (prev) => clearKey(prev, sid));
          }
          set(pendingAsksAtom, (prev) => clearKey(prev, sid));
          set(addActivityAtom, { sessionId: sid, entry: createActivity("session", `Stopped: ${stop_reason ?? "completed"}`) });
          {
            const bg = event.background_tasks;
            if (bg && bg.length > 0) {
              set(addActivityAtom, {
                sessionId: sid,
                entry: createActivity(
                  "session",
                  `${bg.length} background task${bg.length > 1 ? "s" : ""} still running`,
                  summarizeItems(bg, ["description", "command", "name", "id"]),
                ),
              });
            }
            const crons = event.session_crons;
            if (crons && crons.length > 0) {
              set(addActivityAtom, {
                sessionId: sid,
                entry: createActivity(
                  "session",
                  `${crons.length} scheduled cron${crons.length > 1 ? "s" : ""} pending`,
                  summarizeItems(crons, ["prompt", "description", "schedule", "name", "id"]),
                ),
              });
            }
          }
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
          set(pendingAttentionAtom, (prev) => clearKey(prev, sid));
          set(pendingAsksAtom, (prev) => clearKey(prev, sid));
          set(addActivityAtom, { sessionId: sid, entry: createActivity("session", "Session ended") });
          break;
        }
        case "subagent_start": {
          if (!event.agent_id || !event.agent_type) break;
          set(startSubagentActivityAtom, {
            sessionId: sid,
            agentId: event.agent_id,
            agentType: event.agent_type,
            id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
            timestamp: Date.now(),
          });
          break;
        }
        case "subagent_stop": {
          if (!event.agent_id || !event.agent_type) break;
          set(completeSubagentActivityAtom, {
            sessionId: sid,
            agentId: event.agent_id,
            agentType: event.agent_type,
            message: event.agent_message,
            effort: event.effort_level,
            tokens: event.agent_tokens,
            id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
            timestamp: Date.now(),
          });
          break;
        }
        // The attention:pending / agent:background-completed listeners log notifications;
        // falling through to default would add a second, generic entry.
        case "notification": {
          break;
        }
        case "user_prompt_submit": {
          set(lastResponseEffortMapAtom, (prev) => clearKey(prev, sid));
          set(pendingAttentionAtom, (prev) => clearKey(prev, sid));
          // "Chat about this" abandons the AskUserQuestion tool with no result, so
          // PostToolUse never fires ask:user-resolved — clear the pending tint here.
          set(pendingAsksAtom, (prev) => clearKey(prev, sid));
          set(addActivityAtom, { sessionId: sid, entry: createActivity("session", "User prompt submitted") });
          break;
        }
        default: {
          if (!shouldLogGenericActivity(event_type)) break;
          set(addActivityAtom, { sessionId: sid, entry: createActivity("session", `Event: ${event_type}`) });
        }
      }
    }),
  );

  unlisteners.push(
    await listen<ProgramStatusPayload>("session:program-status", (payload) => {
      if (!payload.session_id) return;
      set(programStatusMapAtom, (prev) => applyProgramStatus(prev, payload));
    }),
  );

  unlisteners.push(
    await listen<CostSummary & { session_id: string }>("cost:update", (payload) => {
      const sid = payload.session_id;
      if (!sid) return;
      const { session_id: _, ...cost } = payload;
      set(costMapAtom, (prev) => ({ ...prev, [sid]: cost }));
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string; tasks: Task[] }>("tasks:update", (payload) => {
      const sid = payload.session_id;
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
    await listen<{ session_id: string }>("plan:review-closed", (payload) => {
      const sid = payload.session_id;
      if (!sid) return;
      const closed = applyReviewClosed(get(planReviewStatusMapAtom)[sid] ?? "idle");
      set(planReviewStatusMapAtom, (prev) => ({ ...prev, [sid]: closed.status }));
      if (closed.toast) {
        set(toastsAtom, {
          message: closed.toast,
          description: "Claude Code was answered in the terminal before the revision arrived.",
          type: "error",
        });
      }
    }),
  );

  unlisteners.push(
    await listen<{ path: string; content: string; session_id: string; decision_path?: string }>("plan:ready", (payload) => {
      const sid = payload.session_id;
      if (!sid) return;
      const applied = applyPlanReady(
        get(planStateMapAtom)[sid],
        get(planReviewStatusMapAtom)[sid],
        payload,
      );
      const docApplied = applyPlanReady(
        documentBase(get(planDocumentsAtom)[payload.path], get(planStateMapAtom)[sid], payload),
        applied.status,
        payload,
      );
      set(planStateMapAtom, (prev) => ({ ...prev, [sid]: applied.plan }));
      set(planDocumentsAtom, (prev) => withBoundedEntry(prev, payload.path, docApplied.plan, PLAN_DOCUMENTS_MAX));
      set(registerPlanAtom, { sessionId: sid, path: payload.path });
      set(planReviewStatusMapAtom, (prev) => ({ ...prev, [sid]: applied.status }));
      set(addActivityAtom, { sessionId: sid, entry: createActivity("plan", "Plan ready for review") });

      // Only open plan tab + expand panel if this is the active session
      const activeId = get(activeSessionIdAtom);
      if (activeId === sid) {
        const planName = payload.path.split("/").pop()?.replace(".md", "") ?? "Plan";
        set(openTabAction, { tab: { id: `plan-${payload.path}`, type: "plan", label: planName, data: { path: payload.path } }, isPinned: true });
        set(activePanelViewAtom, "plan");
        set(expandRightPanelAtom, (prev: number) => prev + 1);
      }
      notify("Plan ready", payload.path.split("/").pop() ?? "Plan");
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string }>("ask:user-pending", (payload) => {
      const sid = payload.session_id;
      if (!sid) return;
      set(pendingAsksAtom, (prev) => ({ ...prev, [sid]: true }));
      set(addActivityAtom, { sessionId: sid, entry: createActivity("session", "Claude is asking a question") });
      notify("Claude needs input", "Answer in the terminal");
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string }>("ask:user-resolved", (payload) => {
      const sid = payload.session_id;
      if (!sid) return;
      set(pendingAsksAtom, (prev) => {
        if (!(sid in prev)) return prev;
        const next = { ...prev };
        delete next[sid];
        return next;
      });
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string; cwd: string }>("cwd:changed", (payload) => {
      const sid = payload.session_id;
      if (!sid) return;
      set(cwdMapAtom, (prev) => ({ ...prev, [sid]: payload.cwd }));
      set(addActivityAtom, { sessionId: sid, entry: createActivity("session", `cwd: ${payload.cwd}`) });
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string; path: string; change_type: string }>("file:changed", (payload) => {
      const sid = payload.session_id;
      if (!sid) return;
      const filename = payload.path.split("/").pop() ?? payload.path;
      set(addActivityAtom, { sessionId: sid, entry: createActivity("file_modified", `Changed: ${filename}`, payload.path) });
    }),
  );

  unlisteners.push(
    await listen<AgentStatus & { session_id: string }>("agent:status-update", (payload) => {
      const sid = payload.session_id;
      if (!sid) return;
      const { session_id: _, ...data } = payload;
      set(agentStatusMapAtom, (prev) => ({ ...prev, [sid]: data }));
      const signature = prSignature(payload.github_pr);
      set(prSignalMapAtom, (prev) => (prev[sid] === signature ? prev : { ...prev, [sid]: signature }));
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string; tool_name?: string; reason?: string }>("permission:denied", (payload) => {
      const sid = payload.session_id;
      if (!sid) return;
      const tool = payload.tool_name ?? "unknown tool";
      const reason = payload.reason ?? "Auto-mode denied this action";
      // CC also self-denies inside auto-mode without ever asking the user
      // (internal safety guard). Those arrive with no preceding
      // `permission_prompt` notification, so pendingAttention is unset —
      // surface them in the activity log for audit but skip the toast +
      // desktop notification to keep the chrome quiet.
      const userWasAsked = !!get(pendingAttentionAtom)[sid];
      set(pendingAttentionAtom, (prev) => clearKey(prev, sid));
      set(addActivityAtom, { sessionId: sid, entry: createActivity("session", `Permission denied: ${tool}`, reason) });
      if (userWasAsked) {
        set(toastsAtom, { message: `Permission denied: ${tool}`, description: reason, type: "error" });
        notify("Permission denied", `${tool}: ${reason}`);
      }
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string; notification_type?: string; message?: string }>(
      "attention:pending",
      (payload) => {
        const sid = payload.session_id;
        if (!sid) return;
        const ntype = payload.notification_type ?? "input";
        set(pendingAttentionAtom, (prev) => ({ ...prev, [sid]: ntype }));
        const detail = payload.message ?? undefined;
        set(addActivityAtom, {
          sessionId: sid,
          entry: createActivity("session", notificationAttentionMessage(payload.notification_type), detail),
        });
        notify("Claude needs attention", detail ?? `Type: ${ntype}`);
      },
    ),
  );

  unlisteners.push(
    await listen<{ session_id: string; message?: string | null }>("agent:background-completed", (payload) => {
      const sid = payload.session_id;
      if (!sid) return;
      set(addActivityAtom, {
        sessionId: sid,
        entry: createActivity("subagent", "Background agent finished", payload.message ?? undefined),
      });
      notify("Background agent finished", payload.message ?? "");
      if (shouldClearOnBackgroundCompletion(get(pendingAttentionAtom)[sid])) {
        set(pendingAttentionAtom, (prev) => clearKey(prev, sid));
      }
    }),
  );

  unlisteners.push(
    await listen<{ session_id: string; effort: string }>("effort:last-response", (payload) => {
      const sid = payload.session_id;
      if (!sid) return;
      set(lastResponseEffortMapAtom, (prev) => ({ ...prev, [sid]: payload.effort }));
    }),
  );

  let kittyNotified: ReadonlySet<string> = new Set();
  unlisteners.push(
    await listen<{ session_id: string; pty_id: string }>("terminal:kitty-capped", (payload) => {
      const { notify, next } = kittyCappedNotice(kittyNotified, payload);
      kittyNotified = next;
      if (notify)
        set(toastsAtom, { message: KITTY_CAPPED_TITLE, description: KITTY_CAPPED_DESCRIPTION, type: "info" });
    }),
  );

  // Live preview browser: localhost dev-server ports detected by the Rust
  // scanner. Hysteresis is applied backend-side, so here we just mirror the
  // active set into the atom that the StatusBar chips read.
  unlisteners.push(
    await listen<number[]>("localhost:ports-changed", (payload) => {
      set(localhostPortsAtom, payload ?? []);
    }),
  );

  // Provider status feed (status.claude.com / status.openai.com). The
  // backend emits only on change, so an incident *transition* is toast-worthy
  // here; the persistent surface is the StatusBar chip reading the atom.
  unlisteners.push(
    await listen<ProviderStatus[]>("status:providers", (payload) => {
      const next = payload ?? [];
      const prev = get(providerStatusAtom);
      for (const s of next) {
        if (s.indicator === "none" || s.indicator === "unknown") continue;
        const before = prev.find((p) => p.provider === s.provider);
        if (!before || before.indicator !== s.indicator) {
          set(toastsAtom, {
            message: `${s.provider === "claude" ? "Claude" : "OpenAI"} status: ${s.description}`,
            type: "error",
          });
        }
      }
      set(providerStatusAtom, next);
    }),
  );

  // Agent-spawned-worktree gate: both events only signal "the queue changed";
  // re-fetch the enriched list (count + free disk are computed fresh server-side).
  unlisteners.push(
    await listen<{ request_id: string }>("worktree:request", () => {
      void loadWorktreeRequests(store);
    }),
  );
  unlisteners.push(
    await listen<{ request_id: string }>("worktree:resolved", () => {
      void loadWorktreeRequests(store);
    }),
  );

  // Cover startup race: the scanner's first emit fires ~4ms after backend
  // boot, often before this listener is registered, and Tauri events do
  // not buffer. Query current snapshot once, then deltas keep us in sync.
  try {
    const initial = await invoke<number[]>("browser_get_listening_ports");
    set(localhostPortsAtom, initial);
  } catch {
    /* tolerate failure — scanner deltas will eventually populate the atom */
  }
  // Initial gate load (covers requests enqueued before this boot).
  void loadWorktreeRequests(store);

  try {
    const demoted = await invoke<string[]>("take_reconciled_worktrees");
    if (demoted.length > 0) {
      set(toastsAtom, {
        message: "Worktree no longer exists",
        description: `Moved to the repo root, each keeping its conversation: ${demoted.join(", ")}.`,
        type: "info",
      });
    }
  } catch {}

  return unlisteners;
}
