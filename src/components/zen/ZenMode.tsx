import { useEffect, useCallback } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { zenModeAtom, closeZenModeAtom, zenModeNavigateAtom, zenModeSelectFileAtom } from "@/stores/zenMode";
import { DiffView } from "@/components/plan/DiffView";
import { X, ChevronLeft, ChevronRight } from "lucide-react";

export function ZenMode() {
  const state = useAtomValue(zenModeAtom);
  const close = useSetAtom(closeZenModeAtom);
  const navigate = useSetAtom(zenModeNavigateAtom);
  const selectFile = useSetAtom(zenModeSelectFileAtom);

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
      className="fixed inset-0 z-40 flex"
      role="dialog"
      aria-label="Zen Mode diff review"
    >
      {/* Blur backdrop */}
      <div
        className="absolute inset-0 bg-background/60 backdrop-blur-md"
        onClick={close}
      />

      {/* Diff content area */}
      <div className="relative z-10 flex flex-1 flex-col m-4 mr-0">
        {/* Header */}
        <div className="flex h-9 items-center justify-between rounded-t-lg bg-card/95 border border-border px-3">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => navigate("prev")}
              className="rounded p-0.5 text-muted-foreground hover:text-foreground hover:bg-secondary"
              aria-label="Previous file"
            >
              <ChevronLeft className="size-4" />
            </button>
            <span className="text-xs text-foreground font-medium">{fileName}</span>
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
              aria-label="Close Zen Mode"
            >
              <X className="size-4" />
            </button>
          </div>
        </div>

        {/* Diff viewer */}
        <div className="flex-1 overflow-auto rounded-b-lg bg-card/95 border border-t-0 border-border">
          <DiffView filePath={state.filePath} sessionId={state.sessionId} />
        </div>
      </div>

      {/* Git sidebar */}
      <div className="relative z-10 w-56 flex flex-col m-4 ml-2">
        <div className="flex h-9 items-center rounded-t-lg bg-card/95 border border-border px-3">
          <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">Files</span>
        </div>
        <div className="flex-1 overflow-y-auto rounded-b-lg bg-card/95 border border-t-0 border-border">
          {state.files.map((file, idx) => {
            const name = file.split("/").pop() ?? file;
            const isActive = idx === state.currentIndex;
            return (
              <button
                key={file}
                type="button"
                onClick={() => selectFile(file)}
                className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors ${
                  isActive
                    ? "bg-primary/10 text-primary"
                    : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
                }`}
              >
                <span className={`size-1.5 flex-shrink-0 rounded-full ${isActive ? "bg-primary" : "bg-muted-foreground/40"}`} />
                <span className="truncate">{name}</span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
