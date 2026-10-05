import type { ActivityEntry } from "./types";

export interface PromptCacheStatus {
  warm: boolean | null;
  caching_observed: boolean | null;
  ttl: string | null;
  expires_at: number | null;
  requests: number | null;
  misses: number | null;
  hit_ratio: number | null;
  miss_recache_tokens: number | null;
  last_miss_at: number | null;
  last_miss_causes: string[];
}

const SUBAGENT_TOOLS = new Set(["Agent", "Task"]);

export function notificationAttentionMessage(type?: string | null): string {
  if (type === "agent_needs_input") return "Background agent needs input";
  return `Claude needs attention: ${type ?? "input"}`;
}

export function shouldClearOnBackgroundCompletion(value: string | undefined): boolean {
  return value === "agent_needs_input";
}

export function shouldClearOnTurnEvent(value: string | undefined): boolean {
  return value !== "agent_needs_input";
}

/// Status snapshots arrive on every CC render; they update the status bar, not the timeline.
export function shouldLogGenericActivity(eventType: string): boolean {
  return eventType !== "agent_status" && eventType !== "statusline";
}

/// `Agent` is not in the PostToolUse matcher, so a call that never spawned a
/// subagent (old settings, rejected type, interrupt) has no other closing event.
export function closeUnclaimedSubagentToolRows(list: ActivityEntry[], now: number): ActivityEntry[] {
  let changed = false;
  const next = list.map((e) => {
    if (e.type !== "tool_use" || e.status !== "running" || !e.toolName || !SUBAGENT_TOOLS.has(e.toolName)) return e;
    changed = true;
    return { ...e, status: "done" as const, durationMs: now - e.timestamp };
  });
  return changed ? next : list;
}

export function pickToolRowToComplete(list: ActivityEntry[], toolName?: string): number {
  let fallback = -1;
  for (let i = list.length - 1; i >= 0; i--) {
    const e = list[i];
    if (e.type !== "tool_use" || e.status !== "running") continue;
    if (toolName && e.toolName === toolName) return i;
    if (fallback === -1 && !(e.toolName && SUBAGENT_TOOLS.has(e.toolName))) fallback = i;
  }
  return fallback;
}

export function applySubagentStart(
  list: ActivityEntry[],
  p: { agentId: string; agentType: string; id: string; timestamp: number },
): ActivityEntry[] {
  const message = `Subagent: ${p.agentType}`;
  for (let i = list.length - 1; i >= 0; i--) {
    const e = list[i];
    if (
      e.type === "tool_use" &&
      e.status === "running" &&
      e.toolName &&
      SUBAGENT_TOOLS.has(e.toolName) &&
      !e.agentId
    ) {
      const next = [...list];
      next[i] = { ...e, type: "subagent", agentId: p.agentId, message };
      return next;
    }
  }
  return [
    ...list,
    {
      id: p.id,
      timestamp: p.timestamp,
      type: "subagent",
      message,
      status: "running",
      agentId: p.agentId,
    },
  ];
}

/// SubagentHandback is how a subagent delivers its final report in auto mode;
/// its Stop payload then carries an empty last_assistant_message.
export function handbackReport(toolName: string | undefined, toolInput: unknown): string | undefined {
  if (toolName !== "SubagentHandback" || typeof toolInput !== "object" || toolInput === null) return undefined;
  const message = (toolInput as { message?: unknown }).message;
  return typeof message === "string" && message.trim() !== "" ? message : undefined;
}

export function applySubagentStop(
  list: ActivityEntry[],
  p: {
    agentId: string;
    agentType: string;
    message?: string | null;
    effort?: string | null;
    id: string;
    timestamp: number;
  },
): ActivityEntry[] {
  const label = `Subagent: ${p.agentType}${p.effort ? ` · ${p.effort}` : ""}`;
  const handback = list.find(
    (e) => e.ownerAgentId === p.agentId && e.toolName === "SubagentHandback" && e.detail,
  )?.detail;
  const report = handback ?? (p.message ? p.message : undefined);

  // No PostToolUse is guaranteed for the subagent's own tools (e.g. SubagentHandback
  // is outside the matcher), so its Stop is the only closing event for them.
  const closed = list.map((e) =>
    e.type === "tool_use" && e.status === "running" && e.ownerAgentId === p.agentId
      ? { ...e, status: "done" as const, durationMs: p.timestamp - e.timestamp }
      : e,
  );

  // A row converted in place keeps its start time and sinks below the newest-first
  // viewport, so a finished subagent re-surfaces at the head stamped with the stop time.
  for (let i = closed.length - 1; i >= 0; i--) {
    const e = closed[i];
    if (e.type === "subagent" && e.status === "running" && e.agentId === p.agentId) {
      const done: ActivityEntry = {
        ...e,
        message: label,
        status: "done",
        durationMs: p.timestamp - e.timestamp,
        timestamp: p.timestamp,
        detail: report ?? e.detail,
      };
      return [...closed.slice(0, i), ...closed.slice(i + 1), done];
    }
  }
  return [
    ...closed,
    {
      id: p.id,
      timestamp: p.timestamp,
      type: "subagent",
      message: label,
      status: "done",
      agentId: p.agentId,
      detail: report,
    },
  ];
}

const MISS_CAUSE_LABELS: Record<string, string> = {
  tools_changed: "tool definitions changed",
  system_prompt_changed: "system prompt changed",
  ttl_expired_5m: "idle past the 5m TTL",
  ttl_expired_1h: "idle past the 1h TTL",
  likely_server_side: "likely server-side eviction",
};

export function formatMissCauses(causes: string[]): string {
  return causes.map((c) => MISS_CAUSE_LABELS[c] ?? c.replace(/_/g, " ")).join(", ");
}

export function promptCacheChip(pc: PromptCacheStatus | null | undefined): string | null {
  if (!pc || pc.caching_observed === false || typeof pc.hit_ratio !== "number") return null;
  const pct = `${Math.round(pc.hit_ratio * 100)}%`;
  return pc.warm === false ? `${pct} cold` : pct;
}

export function effortChipText(
  setting: string | null | undefined,
  lastResponse: string | null | undefined,
): string | null {
  if (!setting && !lastResponse) return null;
  if (!setting) return lastResponse ?? null;
  if (!lastResponse) return setting;
  return setting === lastResponse ? setting : `${setting} ran ${lastResponse}`;
}
