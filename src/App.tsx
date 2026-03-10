import { useEffect } from "react";
import { useSetAtom, useStore } from "jotai";
import { Workspace } from "./components/layout/Workspace";
import { setupHookListeners } from "./stores/hooks";
import { configAtom } from "./stores/config";
import { invoke } from "./lib/tauri";
import type { Config } from "./lib/types";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function App() {
  const store = useStore();
  const setConfig = useSetAtom(configAtom);

  useEffect(() => {
    // Show window once React is mounted — avoids blank white screen
    getCurrentWindow().show().catch(() => {});
  }, []);

  useEffect(() => {
    invoke<Config>("get_config", {})
      .then((cfg) => setConfig(cfg))
      .catch(() => {});
  }, [setConfig]);

  useEffect(() => {
    const unlisteners = setupHookListeners(store);
    return () => {
      unlisteners.then((fns) => {
        for (const fn of fns) fn();
      });
    };
  }, [store]);

  return <Workspace />;
}
