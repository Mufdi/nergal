import { useAtom, useSetAtom } from "jotai";
import { configAtom } from "@/stores/config";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import { Switch } from "@/components/ui/switch";

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
            registers a <code className="text-[11px]">cluihud mcp</code> server with Claude Code.
          </span>
        </label>
      </div>
      <p className="rounded-md border border-border/40 bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
        When enabled, any process running as your user that reaches the socket can read every live
        session's metadata (workspace paths, branch, recently-touched files) across all workspaces.
        The uid is the only access boundary — this is a single-user desktop posture. Off by default.
      </p>
    </div>
  );
}
