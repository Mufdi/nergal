import { useAtom, useSetAtom } from "jotai";
import { configAtom } from "@/stores/config";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import { Switch } from "@/components/ui/switch";
import { SummarySection } from "./SummarySection";

export function McpSection() {
  const [config, setConfig] = useAtom(configAtom);
  const setToasts = useSetAtom(toastsAtom);

  async function toggle(enabled: boolean) {
    setConfig((p) => ({ ...p, mcp_server_enabled: enabled })); // optimistic
    try {
      await invoke("mcp_set_enabled", { enabled });
      setToasts({
        message: "MCP server",
        description: enabled
          ? "Enabled — registered with Claude Code."
          : "Disabled — deregistered from Claude Code.",
        type: "success",
      });
    } catch (e) {
      setConfig((p) => ({ ...p, mcp_server_enabled: !enabled })); // rollback
      setToasts({ message: "MCP server", description: String(e), type: "error" });
    }
  }

  const worktreesEnabled = config.agent_spawned_worktrees?.enabled ?? false;
  async function toggleWorktrees(enabled: boolean) {
    setConfig((p) => ({
      ...p,
      agent_spawned_worktrees: {
        enabled,
        request_timeout_secs: p.agent_spawned_worktrees?.request_timeout_secs ?? 3600,
        max_pending_per_session: p.agent_spawned_worktrees?.max_pending_per_session ?? 3,
        soft_worktree_cap: p.agent_spawned_worktrees?.soft_worktree_cap ?? 0,
      },
    })); // optimistic
    try {
      await invoke("agent_worktrees_set_enabled", { enabled });
      setToasts({
        message: "Agent-spawned worktrees",
        description: enabled
          ? "Enabled — agents can request a worktree session (you approve each one)."
          : "Disabled — requests are refused.",
        type: "success",
      });
    } catch (e) {
      setConfig((p) => ({
        ...p,
        agent_spawned_worktrees: {
          ...(p.agent_spawned_worktrees ?? {
            request_timeout_secs: 3600,
            max_pending_per_session: 3,
            soft_worktree_cap: 0,
          }),
          enabled: !enabled,
        },
      })); // rollback
      setToasts({ message: "Agent-spawned worktrees", description: String(e), type: "error" });
    }
  }

  const crossEnabled = config.cross_session?.enabled ?? false;
  async function toggleCrossSession(enabled: boolean) {
    setConfig((p) => ({
      ...p,
      cross_session: {
        enabled,
        max_hops: p.cross_session?.max_hops ?? 4,
        msg_budget: p.cross_session?.msg_budget ?? 30,
        deadline_secs: p.cross_session?.deadline_secs ?? 1800,
      },
    })); // optimistic
    try {
      await invoke("cross_session_set_enabled", { enabled });
      setToasts({
        message: "Cross-session messaging",
        description: enabled
          ? "Enabled — agents can message each other's live sessions."
          : "Disabled — all delivery halted.",
        type: "success",
      });
    } catch (e) {
      setConfig((p) => ({
        ...p,
        cross_session: { ...(p.cross_session ?? { max_hops: 4, msg_budget: 30, deadline_secs: 1800 }), enabled: !enabled },
      })); // rollback
      setToasts({ message: "Cross-session messaging", description: String(e), type: "error" });
    }
  }

  return (
    <div className="space-y-4 pt-1">
      <div className="flex items-start gap-3 text-sm">
        <Switch
          id="setting-mcp_server_enabled"
          checked={config.mcp_server_enabled}
          onCheckedChange={toggle}
          aria-label="Expose session directory to agents"
          className="mt-1"
        />
        <label htmlFor="setting-mcp_server_enabled" className="flex flex-col gap-0.5 cursor-pointer">
          <span className="font-medium">Expose session directory to agents (MCP)</span>
          <span className="text-xs text-muted-foreground">
            Lets the agents you run discover their sibling sessions
            (<code className="text-[11px]">whoami</code>, <code className="text-[11px]">list_sessions</code>,
            {" "}<code className="text-[11px]">get_session</code>) over a local, uid-restricted socket, and
            registers a <code className="text-[11px]">nergal mcp</code> server with Claude Code.
          </span>
        </label>
      </div>
      <p className="rounded-md border border-border/40 bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
        When enabled, any process running as your user that reaches the socket can read every live
        session's metadata (workspace paths, branch, recently-touched files) across all workspaces.
        The uid is the only access boundary — this is a single-user desktop posture. Off by default.
      </p>
      <div className="flex items-start gap-3 border-t border-border/40 pt-4 text-sm">
        <Switch
          id="setting-cross_session_enabled"
          checked={crossEnabled}
          onCheckedChange={toggleCrossSession}
          aria-label="Enable cross-session messaging"
          className="mt-1"
        />
        <label htmlFor="setting-cross_session_enabled" className="flex flex-col gap-0.5 cursor-pointer">
          <span className="font-medium">Cross-session messaging</span>
          <span className="text-xs text-muted-foreground">
            Lets a live session's agent message another live session
            (<code className="text-[11px]">send_to_session</code>, <code className="text-[11px]">read_messages</code>).
            nergal routes delivery by waking the target's terminal. Relayed context is labeled advisory,
            never carrying your authority. This is the master kill-switch — off halts all delivery.
            Requires the MCP server above to be enabled.
          </span>
        </label>
      </div>
      <div className="flex items-start gap-3 border-t border-border/40 pt-4 text-sm">
        <Switch
          id="setting-agent_spawned_worktrees_enabled"
          checked={worktreesEnabled}
          onCheckedChange={toggleWorktrees}
          aria-label="Enable agent-spawned worktree sessions"
          className="mt-1"
        />
        <label
          htmlFor="setting-agent_spawned_worktrees_enabled"
          className="flex flex-col gap-0.5 cursor-pointer"
        >
          <span className="font-medium">Agent-spawned worktree sessions</span>
          <span className="text-xs text-muted-foreground">
            Lets an agent <em>request</em> a new dedicated worktree session
            (<code className="text-[11px]">create_worktree_session</code>) under an active
            workspace. Every request waits for your explicit approval in a gate that shows the
            requesting session, prompt, target agent and permission preset — nothing is created
            until you approve. The most resource-sensitive capability; off by default. Requires
            the MCP server above to be enabled.
          </span>
        </label>
      </div>
      <SummarySection />
    </div>
  );
}
