import { useCallback, useEffect, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { ExternalLink, Pin, PinOff } from "lucide-react";
import { activeWorkspaceAtom, activeSessionIdAtom } from "@/stores/workspace";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";
import { activeSessionPinnedNotesAtom, pinNoteAtom, unpinNoteAtom } from "@/stores/pinnedNotes";
import { setTabPathAction } from "@/stores/rightPanel";
import { focusZoneAtom } from "@/stores/shortcuts";
import { toastsAtom } from "@/stores/toast";
import { MarkdownView } from "@/components/plan/MarkdownView";
import { openInObsidian } from "@/lib/obsidian";
import { openObsidianHref } from "@/lib/markdown/obsidianMarkdown";
import { invoke } from "@/lib/tauri";

function noteName(path: string): string {
  const base = path.split("/").pop() ?? path;
  return base.replace(/\.md$/i, "");
}

/// Pull the `file` param out of an `obsidian://open?...&file=…` href.
function wikilinkTarget(href: string): string | null {
  try {
    return new URL(href).searchParams.get("file");
  } catch {
    const m = href.match(/[?&]file=([^&]+)/);
    return m ? decodeURIComponent(m[1]) : null;
  }
}

/// Reading view for a single vault note, rendered inside an `obsidiannote`
/// document tab. Wikilinks navigate by swapping the tab's own path (stays in
/// the same tab), Ctrl/Cmd+click opens Obsidian.
export function ObsidianNoteView({ tabId, path }: { tabId: string; path: string }) {
  const workspace = useAtomValue(activeWorkspaceAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const pinned = useAtomValue(activeSessionPinnedNotesAtom);
  const pinNote = useSetAtom(pinNoteAtom);
  const unpinNote = useSetAtom(unpinNoteAtom);
  const setTabPath = useSetAtom(setTabPathAction);
  const setToasts = useSetAtom(toastsAtom);
  const focusZone = useAtomValue(focusZoneAtom);

  const [body, setBody] = useState("");
  const [loading, setLoading] = useState(true);
  const isPinned = pinned.includes(path);

  // Contextual single-key verbs (`p` pin/unpin, `o` open in Obsidian), like the
  // conflict panel's o/t/b letters — only while the panel holds focus, never
  // from the terminal.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (focusZone !== "panel") return;
      const target = e.target as HTMLElement | null;
      if (target?.tagName === "INPUT" || target?.tagName === "TEXTAREA" || target?.closest(".cm-editor")) {
        return;
      }
      const bare = !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey;
      if (e.code === "KeyP" && bare) {
        e.preventDefault();
        if (!sessionId) return;
        if (isPinned) unpinNote({ sessionId, path });
        else pinNote({ sessionId, path });
      } else if (e.code === "KeyO" && bare) {
        e.preventDefault();
        if (!workspace) return;
        openInObsidian(workspace.id, path).catch((err) =>
          setToasts({ message: "Open in Obsidian failed", description: String(err), type: "error" }),
        );
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [focusZone, sessionId, isPinned, path, pinNote, unpinNote, workspace, setToasts]);

  useEffect(() => {
    if (!workspace) return;
    setLoading(true);
    invoke<string>("read_vault_note", { workspaceId: workspace.id, path })
      .then(setBody)
      .catch((err) => {
        setBody("");
        setToasts({ message: "Read note failed", description: String(err), type: "error" });
      })
      .finally(() => setLoading(false));
  }, [workspace, path, setToasts]);

  const onWikilinkNavigate = useCallback(
    async (href: string) => {
      if (!workspace) return;
      const name = wikilinkTarget(href);
      if (!name) {
        openObsidianHref(href);
        return;
      }
      try {
        const resolved = await invoke<string | null>("resolve_vault_note", {
          workspaceId: workspace.id,
          name,
        });
        if (resolved) setTabPath({ tabId, path: resolved });
        else openObsidianHref(href);
      } catch {
        openObsidianHref(href);
      }
    },
    [workspace, tabId, setTabPath],
  );

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-border/50 px-3 py-1.5">
        <span className="min-w-0 flex-1 truncate text-[11px] font-medium text-foreground/80">
          {noteName(path)}
        </span>
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                onClick={() =>
                  sessionId &&
                  (isPinned ? unpinNote({ sessionId, path }) : pinNote({ sessionId, path }))
                }
                disabled={!sessionId}
                className="rounded p-1 text-muted-foreground hover:bg-secondary hover:text-foreground disabled:opacity-50"
              />
            }
          >
            {isPinned ? <PinOff size={13} /> : <Pin size={13} />}
          </TooltipTrigger>
          <TooltipContent side="bottom" className="text-[10px]">{isPinned ? "Unpin from session (P)" : "Pin to session (P)"}</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                onClick={() =>
                  workspace &&
                  openInObsidian(workspace.id, path).catch((err) =>
                    setToasts({ message: "Open in Obsidian failed", description: String(err), type: "error" }),
                  )
                }
                className="rounded p-1 text-muted-foreground hover:bg-secondary hover:text-foreground"
              />
            }
          >
            <ExternalLink size={13} />
          </TooltipTrigger>
          <TooltipContent side="bottom" className="text-[10px]">Open in Obsidian (O)</TooltipContent>
        </Tooltip>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex flex-col items-center justify-center gap-2 px-6 py-6">
            <ProgressBar className="max-w-32" />
            <span className="text-xs text-muted-foreground">Loading…</span>
          </div>
        ) : (
          <MarkdownView content={body} onWikilinkNavigate={onWikilinkNavigate} />
        )}
      </div>
    </div>
  );
}
