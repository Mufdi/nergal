import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Provider } from "jotai";
import { appStore } from "@/stores/jotaiStore";
import { TooltipProvider } from "@/components/ui/tooltip";
import { App } from "./App";
import "./styles/globals.css";

document.documentElement.classList.add("dark");

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Provider store={appStore}>
      <TooltipProvider>
        <App />
      </TooltipProvider>
    </Provider>
  </StrictMode>,
);
