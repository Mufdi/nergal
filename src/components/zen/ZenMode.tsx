import { useState, useEffect, useCallback } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { zenModeAtom, closeZenModeAtom, zenModeNavigateAtom } from "@/stores/zenMode";
import { DiffView } from "@/components/plan/DiffView";
import { GitPanel } from "@/components/git/GitPanel";
import { X, ChevronLeft, ChevronRight } from "lucide-react";

type SidebarTab = "changes" | "history";

/// Full-screen diff review overlay with git sidebar.
export function GitFullView() {
  const state = useAtomValue(zenModeAtom);
  const close = useSetAtom(closeZenModeAtom);
  const navigate = useSetAtom(zenModeNavigateAtom);
  const [sidebarTab, setSidebarTab] = useState<SidebarTab>("changes");

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (!state.open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "ArrowRight" && e.altKey) {
      e.preventDefault();
      navigate("next");
    } else if (e.key === "ArrowLeft" && e.altKey) {
      e.preventDefault();
      navigate("prev");
    }
  }, [state.open, close, navigate]);

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  if (!state.open || !state.filePath || !state.sessionId) return null;

  const fileName = state.filePath.split("/").pop() ?? state.filePath;

  return (
    <div
      className="fixed inset-0 z-40 flex overflow-hidden"
      role="dialog"
      aria-label="Git full diff review"
    >
      {/* Blur backdrop */}
      <div
        className="absolute inset-0 bg-background/60 backdrop-blur-md"
        onClick={close}
      />

      {/* Diff content area */}
      <div className="relative z-10 flex min-w-0 flex-1 flex-col m-3 mr-0 overflow-hidden">
        {/* Header */}
        <div className="flex h-9 shrink-0 items-center justify-between rounded-t-lg bg-card/95 border border-border px-3">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => navigate("prev")}
              className="rounded p-0.5 text-muted-foreground hover:text-foreground hover:bg-secondary"
              aria-label="Previous file"
            >
              <ChevronLeft className="size-4" />
            </button>
            <span className="text-xs text-foreground font-medium truncate">{fileName}</span>
            <span className="text-[10px] text-muted-foreground">
              ({state.currentIndex + 1}/{state.files.length})
            </span>
            <button
              type="button"
              onClick={() => navigate("next")}
              className="rounded p-0.5 text-muted-foreground hover:text-foreground hover:bg-secondary"
              aria-label="Next file"
            >
              <ChevronRight className="size-4" />
            </button>
          </div>
          <div className="flex items-center gap-1">
            <span className="text-[10px] text-muted-foreground">Esc to close</span>
            <button
              type="button"
              onClick={close}
              className="rounded p-0.5 text-muted-foreground hover:text-foreground hover:bg-secondary"
              aria-label="Close"
            >
              <X className="size-4" />
            </button>
          </div>
        </div>

        {/* Diff viewer — side by side */}
        <div className="flex-1 overflow-hidden rounded-b-lg bg-card/95 border border-t-0 border-border">
          <DiffView key={state.filePath} filePath={state.filePath} sessionId={state.sessionId} sideBySide />
        </div>
      </div>

      {/* Git sidebar with tabs */}
      <div className="relative z-10 w-72 shrink-0 flex flex-col m-3 ml-1.5 overflow-hidden rounded-lg bg-card/95 border border-border">
        {/* Tab bar */}
        <div className="flex shrink-0 border-b border-border/50">
          {(["changes", "history"] as const).map((tab) => (
            <button
              key={tab}
              type="button"
              onClick={() => setSidebarTab(tab)}
              className={`flex-1 py-1.5 text-center text-[10px] font-medium uppercase tracking-wider transition-colors ${
                sidebarTab === tab
                  ? "text-foreground border-b-2 border-primary"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {tab}
            </button>
          ))}
        </div>

        {/* Tab content */}
        <div className="flex-1 overflow-hidden">
          {sidebarTab === "changes" ? (
            <GitPanelChangesOnly sessionId={state.sessionId} />
          ) : (
            <GitPanelHistoryOnly sessionId={state.sessionId} />
          )}
        </div>
      </div>
    </div>
  );
}

/// Stripped-down git panel showing only staged/unstaged/untracked + commit bar.
function GitPanelChangesOnly({ sessionId }: { sessionId: string }) {
  return (
    <div className="h-full overflow-hidden">
      <GitPanel sessionId={sessionId} hideHistory />
    </div>
  );
}

/// Stripped-down git panel showing only history.
function GitPanelHistoryOnly({ sessionId }: { sessionId: string }) {
  return (
    <div className="h-full overflow-hidden">
      <GitPanel sessionId={sessionId} hideChanges />
    </div>
  );
}

// Re-export for backwards compat with Workspace import
export { GitFullView as ZenMode };
