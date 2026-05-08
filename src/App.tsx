import { useEffect, Component, type ReactNode } from "react";
import { useAtomValue, useSetAtom, useStore } from "jotai";
import { Workspace } from "./components/layout/Workspace";
import { AskUserModal } from "./components/session/AskUserModal";
import { setupAgentListeners } from "./stores/agent";
import { setupHookListeners } from "./stores/hooks";
import { configAtom } from "./stores/config";
import { invoke } from "./lib/tauri";
import { applyTheme } from "./lib/themes";
import type { Config } from "./lib/types";
import { getCurrentWindow } from "@tauri-apps/api/window";

class ErrorBoundary extends Component<
  { children: ReactNode },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-4 bg-background p-8">
          <p className="text-sm font-medium text-destructive">Something went wrong</p>
          <pre className="max-w-lg overflow-auto rounded-lg bg-card p-4 text-xs text-muted-foreground">
            {this.state.error.message}
          </pre>
          <button
            onClick={() => this.setState({ error: null })}
            className="rounded-md bg-secondary px-3 py-1.5 text-xs text-foreground hover:bg-secondary/80 transition-colors"
          >
            Try again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

export function App() {
  const store = useStore();
  const setConfig = useSetAtom(configAtom);
  const config = useAtomValue(configAtom);
  const themeMode = config.theme_mode;
  const customThemes = config.custom_themes;

  useEffect(() => {
    getCurrentWindow().show().catch(() => {});
  }, []);

  useEffect(() => {
    invoke<Config>("get_config", {})
      .then((cfg) => setConfig(cfg))
      .catch(() => {});
  }, [setConfig]);

  useEffect(() => {
    applyTheme(themeMode, customThemes);
  }, [themeMode, customThemes]);

  useEffect(() => {
    document.documentElement.dataset.panelGlow = config.panel_glow ? "on" : "off";
  }, [config.panel_glow]);

  useEffect(() => {
    const unlisteners = setupHookListeners(store);
    const unlistenAgents = setupAgentListeners();
    return () => {
      unlisteners.then((fns) => {
        for (const fn of fns) fn();
      });
      unlistenAgents.then((fn) => fn());
    };
  }, [store]);

  return (
    <ErrorBoundary>
      <Workspace />
      <AskUserModal />
    </ErrorBoundary>
  );
}
