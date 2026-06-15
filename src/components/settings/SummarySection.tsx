import { useEffect, useState } from "react";
import { useSetAtom } from "jotai";
import { invoke } from "@/lib/tauri";
import { toastsAtom } from "@/stores/toast";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";

type Backend = "off" | "agent_cli" | "api_key";

interface SummarySettings {
  backend: Backend;
  agent_command: string | null;
  api_base_url: string | null;
  api_model: string | null;
  has_api_key: boolean;
  disabled_projects: string[];
}

// Opt-in AI session summaries (phase 6). Two mutually-exclusive backends; the
// Rust config is a single enum, so enabling one switch here flips the other off.
export function SummarySection() {
  const setToasts = useSetAtom(toastsAtom);
  const [backend, setBackend] = useState<Backend>("off");
  const [agentCommand, setAgentCommand] = useState("");
  const [apiBaseUrl, setApiBaseUrl] = useState("");
  const [apiModel, setApiModel] = useState("");
  const [hasApiKey, setHasApiKey] = useState(false);
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    invoke<SummarySettings>("summary_get_settings")
      .then((s) => {
        setBackend(s.backend);
        setAgentCommand(s.agent_command ?? "");
        setApiBaseUrl(s.api_base_url ?? "");
        setApiModel(s.api_model ?? "");
        setHasApiKey(s.has_api_key);
      })
      .catch(() => {})
      .finally(() => setLoaded(true));
  }, []);

  async function persist(next: Backend) {
    try {
      await invoke("summary_set_settings", {
        backend: next,
        agentCommand: agentCommand.trim() || null,
        apiBaseUrl: apiBaseUrl.trim() || null,
        apiModel: apiModel.trim() || null,
      });
    } catch (e) {
      setToasts({ message: "Summaries", description: String(e), type: "error" });
      throw e;
    }
  }

  async function pickBackend(target: Exclude<Backend, "off">, on: boolean) {
    const next: Backend = on ? target : "off";
    const prev = backend;
    setBackend(next); // optimistic
    try {
      await persist(next);
    } catch {
      setBackend(prev); // rollback
    }
  }

  async function saveFields() {
    try {
      await persist(backend);
      setToasts({ message: "Summaries", description: "Settings saved.", type: "success" });
    } catch {
      /* toast already shown */
    }
  }

  async function saveApiKey() {
    const key = apiKeyInput.trim();
    if (!key) return;
    try {
      await invoke("set_summary_api_key", { key });
      setHasApiKey(true);
      setApiKeyInput("");
      setToasts({ message: "Summaries", description: "API key stored in OS keyring.", type: "success" });
    } catch (e) {
      setToasts({ message: "Summaries", description: String(e), type: "error" });
    }
  }

  async function clearApiKey() {
    try {
      await invoke("clear_summary_api_key");
      setHasApiKey(false);
      setToasts({ message: "Summaries", description: "API key removed.", type: "success" });
    } catch (e) {
      setToasts({ message: "Summaries", description: String(e), type: "error" });
    }
  }

  if (!loaded) return null;

  return (
    <div className="space-y-4 border-t border-border/40 pt-4">
      <div className="space-y-1">
        <h3 className="text-sm font-medium">AI session summaries</h3>
        <p className="text-xs text-muted-foreground">
          Generate a short recap of each session, surfaced in the directory's{" "}
          <code className="text-[11px]">summary</code> field. Off by default. Pick one backend —
          the two are mutually exclusive.
        </p>
      </div>

      {/* Agent CLI mode */}
      <div className="flex items-start gap-3 text-sm">
        <Switch
          id="summary-agent-cli"
          checked={backend === "agent_cli"}
          onCheckedChange={(on) => pickBackend("agent_cli", on)}
          aria-label="Summarize via agent CLI"
          className="mt-1"
        />
        <label htmlFor="summary-agent-cli" className="flex flex-col gap-0.5 cursor-pointer">
          <span className="font-medium">Agent CLI (subscription, no API key)</span>
          <span className="text-xs text-muted-foreground">
            Runs your installed agent headlessly (<code className="text-[11px]">claude -p</code>) on
            your existing subscription. Consumes your plan's quota/rate limits.
          </span>
        </label>
      </div>
      {backend === "agent_cli" && (
        <div className="ml-10 flex items-center gap-2">
          <Input
            value={agentCommand}
            onChange={(e) => setAgentCommand(e.target.value)}
            placeholder="claude"
            aria-label="Agent command"
            className="max-w-[220px]"
          />
          <Button variant="outline" size="sm" onClick={saveFields}>
            Save
          </Button>
        </div>
      )}

      {/* API key mode */}
      <div className="flex items-start gap-3 text-sm">
        <Switch
          id="summary-api-key"
          checked={backend === "api_key"}
          onCheckedChange={(on) => pickBackend("api_key", on)}
          aria-label="Summarize via API key"
          className="mt-1"
        />
        <label htmlFor="summary-api-key" className="flex flex-col gap-0.5 cursor-pointer">
          <span className="font-medium">API key (any provider)</span>
          <span className="text-xs text-muted-foreground">
            Calls a provider-agnostic OpenAI-compatible endpoint. The key is stored in your OS
            keyring, never on disk.
          </span>
        </label>
      </div>
      {backend === "api_key" && (
        <div className="ml-10 space-y-2">
          <Input
            value={apiBaseUrl}
            onChange={(e) => setApiBaseUrl(e.target.value)}
            placeholder="https://api.openai.com/v1"
            aria-label="API base URL"
          />
          <Input
            value={apiModel}
            onChange={(e) => setApiModel(e.target.value)}
            placeholder="gpt-4o-mini"
            aria-label="Model"
            className="max-w-[260px]"
          />
          <div className="flex items-center gap-2">
            <Input
              type="password"
              value={apiKeyInput}
              onChange={(e) => setApiKeyInput(e.target.value)}
              placeholder={hasApiKey ? "•••••••• (stored)" : "API key"}
              aria-label="API key"
              className="max-w-[260px]"
            />
            <Button variant="outline" size="sm" onClick={saveApiKey} disabled={!apiKeyInput.trim()}>
              Save key
            </Button>
            {hasApiKey && (
              <Button variant="ghost" size="sm" onClick={clearApiKey}>
                Clear
              </Button>
            )}
          </div>
          <Button variant="outline" size="sm" onClick={saveFields}>
            Save endpoint
          </Button>
        </div>
      )}
    </div>
  );
}
