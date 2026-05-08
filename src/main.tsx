import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Provider } from "jotai";
import { appStore } from "@/stores/jotaiStore";
import { TooltipProvider } from "@/components/ui/tooltip";
import { App } from "./App";
import { applyCachedTheme } from "@/lib/themes";
import "./styles/globals.css";

// data-theme drives both CSS tokens and the `dark:` Tailwind variant.
// Single source of truth — no `.dark` class needed. Strip the legacy class
// in case a stale instance still has it from before this refactor.
document.documentElement.classList.remove("dark");
// Apply the cached theme synchronously to avoid the default-theme flash
// while React waits for `get_config` to round-trip over Tauri IPC. The
// authoritative value still arrives via configAtom in App.tsx and will
// re-apply if it diverges from the cache.
applyCachedTheme();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Provider store={appStore}>
      <TooltipProvider>
        <App />
      </TooltipProvider>
    </Provider>
  </StrictMode>,
);
