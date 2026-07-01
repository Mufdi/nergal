import { useEffect, useRef, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { FloatingPanel } from "./FloatingPanel";
import { invoke } from "@/lib/tauri";
import { activeWorkspaceAtom } from "@/stores/workspace";
import { obsidianConfigAtom } from "@/stores/obsidian";
import { toastsAtom } from "@/stores/toast";
import { focusZoneAtom } from "@/stores/shortcuts";
import * as terminalService from "@/components/terminal/terminalService";
import {
  QUICK_CAPTURE_PANEL_ID,
  clampToViewport,
  loadQuickCaptureGeometry,
  quickCaptureGeometryAtom,
  quickCaptureOpacityAtom,
  quickCaptureOpenAtom,
  saveQuickCaptureGeometry,
} from "@/stores/quickCapture";

let geometryBootstrapped = false;

export function QuickCapturePanel() {
  const open = useAtomValue(quickCaptureOpenAtom);
  const setOpen = useSetAtom(quickCaptureOpenAtom);
  const geometry = useAtomValue(quickCaptureGeometryAtom);
  const setGeometry = useSetAtom(quickCaptureGeometryAtom);
  const opacity = useAtomValue(quickCaptureOpacityAtom);
  const activeWorkspace = useAtomValue(activeWorkspaceAtom);
  const cfg = useAtomValue(obsidianConfigAtom);
  const setToasts = useSetAtom(toastsAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (geometryBootstrapped) return;
    geometryBootstrapped = true;
    void loadQuickCaptureGeometry();
  }, []);

  useEffect(() => {
    if (!open) {
      setText("");
      return;
    }
    requestAnimationFrame(() => textareaRef.current?.focus());
  }, [open]);

  useEffect(() => {
    function reclamp() {
      setGeometry((prev) => clampToViewport(prev));
    }
    window.addEventListener("resize", reclamp);
    return () => window.removeEventListener("resize", reclamp);
  }, [setGeometry]);

  async function submit() {
    if (!text.trim() || busy) return;
    if (!activeWorkspace) {
      setToasts({
        message: "No active workspace",
        description: "Open a workspace before capturing.",
        type: "error",
      });
      return;
    }
    if (!cfg?.quick_capture_path) {
      setToasts({
        message: "Quick capture path not set",
        description: "Settings → Obsidian Integration → Quick capture channel.",
        type: "info",
      });
      return;
    }
    setBusy(true);
    try {
      const path = await invoke<string>("obsidian_quick_capture", {
        workspaceId: activeWorkspace.id,
        text,
      });
      const filename = path.split("/").pop() ?? path;
      setToasts({ message: `Captured to ${filename}`, type: "success" });
      setOpen(false);
    } catch (err) {
      setToasts({
        message: "Capture failed",
        description: String(err),
        type: "error",
      });
    } finally {
      setBusy(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      void submit();
    }
  }

  return (
    <FloatingPanel
      panelId={QUICK_CAPTURE_PANEL_ID}
      open={open}
      onClose={() => {
        // Closing an overlay returns focus to the terminal prompt (patterns.md §focus).
        setOpen(false);
        setFocusZone("terminal");
        requestAnimationFrame(() => terminalService.focusActive());
      }}
      geometry={geometry}
      onGeometryChange={(next) => {
        setGeometry(next);
        void saveQuickCaptureGeometry(next, opacity);
      }}
      opacity={opacity}
      zIndex={56}
      minWidth={320}
      minHeight={180}
      accent
      title={<span className="font-medium">Quick capture</span>}
    >
      <div className="flex h-full flex-col gap-2 p-2">
        <textarea
          ref={textareaRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="A thought, a snippet, a TODO…"
          className="flex-1 resize-none rounded bg-background/60 p-2 text-sm text-foreground outline-none placeholder:text-muted-foreground/60 focus:ring-1 focus:ring-primary/40"
        />
        <div className="flex items-center justify-between text-[10px] text-muted-foreground">
          <span>
            <kbd className="rounded bg-secondary px-1 py-0.5">Enter</kbd> save ·{" "}
            <kbd className="rounded bg-secondary px-1 py-0.5">Shift+Enter</kbd> newline ·{" "}
            <kbd className="rounded bg-secondary px-1 py-0.5">Esc</kbd> cancel
          </span>
          <span>{busy ? "Saving…" : `${text.length} chars`}</span>
        </div>
      </div>
    </FloatingPanel>
  );
}
