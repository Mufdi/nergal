import { useEffect, useRef, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { Search, FileText, Send, NotebookPen, Pin } from "lucide-react";
import {
  searchModalOpenAtom,
  searchQueryAtom,
  searchResultsAtom,
  searchLoadingAtom,
  searchScopeAtom,
  runSearchAtom,
  type SearchHit,
} from "@/stores/search";
import { activeWorkspaceAtom, activeSessionIdAtom } from "@/stores/workspace";
import { vaultSearchScopeAtom, obsidianConfigAtom } from "@/stores/obsidian";
import { focusZoneAtom } from "@/stores/shortcuts";
import { toastsAtom } from "@/stores/toast";
import { openInObsidian } from "@/lib/obsidian";
import { pinNoteAtom } from "@/stores/pinnedNotes";
import { invoke } from "@/lib/tauri";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";
import * as terminalService from "@/components/terminal/terminalService";
import {
  scratchpadActiveTabIdAtom,
  scratchpadContentAtom,
  scratchpadOpenAtom,
  persistTabContent,
} from "@/stores/scratchpad";
import { appStore } from "@/stores/jotaiStore";

const DEBOUNCE_MS = 200;

function noteWikiName(hit: SearchHit): string {
  const base = hit.path.split("/").pop() ?? hit.path;
  return base.replace(/\.md$/i, "");
}

function displayLabel(hit: SearchHit): string {
  return hit.title ?? noteWikiName(hit);
}

export function VaultSearchModal() {
  const isOpen = useAtomValue(searchModalOpenAtom);
  const setOpen = useSetAtom(searchModalOpenAtom);
  const query = useAtomValue(searchQueryAtom);
  const setQuery = useSetAtom(searchQueryAtom);
  const setScope = useSetAtom(searchScopeAtom);
  const results = useAtomValue(searchResultsAtom);
  const loading = useAtomValue(searchLoadingAtom);
  const runSearch = useSetAtom(runSearchAtom);
  const workspace = useAtomValue(activeWorkspaceAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const setToasts = useSetAtom(toastsAtom);
  const pinNote = useSetAtom(pinNoteAtom);
  const scopeMode = useAtomValue(vaultSearchScopeAtom);
  const setScopeMode = useSetAtom(vaultSearchScopeAtom);
  const obsidianConfig = useAtomValue(obsidianConfigAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const subdir = obsidianConfig?.search_subdir?.trim() || null;

  function toggleScope() {
    if (!subdir) return; // nothing configured to scope to
    setScopeMode((m) => (m === "subdir" ? "whole" : "subdir"));
    void runSearch();
  }

  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // #7 locks the scope to Vault; the engine supports others for future Cmd+P.
  useEffect(() => {
    if (isOpen) {
      setScope({ kind: "vault" });
      setSelectedIndex(0);
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [isOpen, setScope]);

  useEffect(() => {
    if (!isOpen) return;
    const handle = setTimeout(() => void runSearch(), DEBOUNCE_MS);
    return () => clearTimeout(handle);
  }, [isOpen, query, runSearch]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [results]);

  useEffect(() => {
    if (!isOpen || !listRef.current) return;
    const selected = listRef.current.querySelector("[data-search-selected]") as HTMLElement | null;
    if (selected) selected.scrollIntoView({ block: "nearest" });
  }, [isOpen, selectedIndex]);

  function close() {
    setOpen(false);
    // Hand focus back to the PTY so typing resumes immediately — same pattern
    // the scratchpad uses on close (rAF lets React finish unmounting first).
    requestAnimationFrame(() => {
      setFocusZone("terminal");
      terminalService.focusActive();
    });
  }

  function openHit(hit: SearchHit) {
    if (!workspace) return;
    close();
    openInObsidian(workspace.id, hit.path).catch((err) => {
      setToasts({ message: "Open in Obsidian failed", description: String(err), type: "error" });
    });
  }

  async function sendToAgent(hit: SearchHit) {
    if (!activeSessionId) {
      setToasts({ message: "Send to agent", description: "No active session.", type: "info" });
      return;
    }
    close();
    try {
      await invoke("write_to_session_pty", {
        sessionId: activeSessionId,
        // The agent only sees the PTY — it can't resolve a vault [[name]], so
        // send the readable absolute path. No trailing \r: the citation lands in
        // the prompt for the user to keep typing before submitting.
        data: `> Source: ${hit.path} `,
      });
      terminalService.focusActive();
    } catch (err) {
      setToasts({ message: "Send to agent failed", description: String(err), type: "error" });
    }
  }

  async function pinToSession(hit: SearchHit) {
    if (!activeSessionId) {
      setToasts({ message: "Pin to session", description: "No active session.", type: "info" });
      return;
    }
    close();
    try {
      await pinNote({ sessionId: activeSessionId, path: hit.path });
      setToasts({
        message: "Pinned to session",
        description: `${displayLabel(hit)} — injected at the next spawn/resume.`,
        type: "success",
      });
    } catch (err) {
      setToasts({ message: "Pin failed", description: String(err), type: "error" });
    }
  }

  async function citeInScratchpad(hit: SearchHit) {
    const tabId = appStore.get(scratchpadActiveTabIdAtom);
    if (!tabId) {
      setToasts({
        message: "Cite in scratchpad",
        description: "Open the scratchpad (Ctrl+Alt+L) and create a tab first.",
        type: "info",
      });
      return;
    }
    const current = appStore.get(scratchpadContentAtom)[tabId] ?? "";
    const sep = current.length > 0 && !current.endsWith("\n") ? "\n" : "";
    const updated = `${current}${sep}> Source: [[${noteWikiName(hit)}]]\n`;
    await persistTabContent(tabId, updated);
    appStore.set(scratchpadOpenAtom, true);
    close();
  }

  useEffect(() => {
    if (!isOpen) return;
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
        return;
      }
      // event.code (not key) — WebKitGTK layout quirk, consistent with the app.
      if ((e.ctrlKey || e.metaKey) && e.code === "KeyD") {
        e.preventDefault();
        toggleScope();
        return;
      }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((prev) => Math.min(prev + 1, results.length - 1));
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((prev) => Math.max(prev - 1, 0));
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        const hit = results[selectedIndex];
        if (!hit) return;
        if (e.shiftKey) void pinToSession(hit);
        else if (e.altKey) void citeInScratchpad(hit);
        else if (e.ctrlKey || e.metaKey) openHit(hit);
        else void sendToAgent(hit);
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen, selectedIndex, results]);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex justify-center pt-[20vh]" onClick={close}>
      <div className="fixed inset-0 bg-scrim cluihud-blur-md" />
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Search the vault"
        className="cluihud-glow relative z-10 flex max-h-[70vh] w-full max-w-lg flex-col rounded-lg border-2 border-primary bg-card shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-border/50 px-3 py-2">
          <Search className="size-4 shrink-0 text-muted-foreground" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search the vault..."
            aria-label="Search the vault"
            className="flex-1 bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground/50"
          />
          {subdir && (
            <span className="shrink-0 text-[10px] text-muted-foreground/60" aria-hidden>
              Ctrl+D
            </span>
          )}
          <button
            type="button"
            onClick={toggleScope}
            disabled={!subdir}
            title={
              subdir
                ? `Scope (Ctrl+D): ${scopeMode === "subdir" ? subdir : "whole vault"}`
                : "Set a search subdir in Settings → Obsidian to scope"
            }
            className="rounded bg-secondary px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground enabled:hover:text-foreground disabled:opacity-60"
          >
            {scopeMode === "subdir" && subdir ? subdir : "Vault"}
          </button>
        </div>
        <div ref={listRef} className="flex-1 overflow-y-auto py-1">
          {loading && results.length === 0 && (
            <div className="flex items-center justify-center py-6">
              <span className="text-xs text-muted-foreground">Searching…</span>
            </div>
          )}
          {!loading && query.trim() !== "" && results.length === 0 && (
            <div className="flex items-center justify-center py-6">
              <span className="text-xs text-muted-foreground">No notes found</span>
            </div>
          )}
          {query.trim() === "" && results.length === 0 && (
            <div className="flex flex-col items-center gap-1 py-6">
              <span className="text-xs text-muted-foreground">Type to search your vault</span>
              <span className="text-[10px] text-muted-foreground/60">
                Enter: send · Ctrl+Enter: open · Alt+Enter: cite · Shift+Enter: pin
              </span>
            </div>
          )}
          {results.map((hit, idx) => {
            const isSelected = idx === selectedIndex;
            return (
              <div
                key={`${hit.path}:${hit.lineNumber ?? 0}`}
                data-search-selected={isSelected ? "true" : undefined}
                onMouseEnter={() => setSelectedIndex(idx)}
                className={`group flex items-center justify-between gap-3 px-3 py-1.5 transition-colors ${
                  isSelected ? "bg-secondary" : "hover:bg-secondary/50"
                }`}
              >
                <button
                  type="button"
                  onClick={() => openHit(hit)}
                  className="flex min-w-0 flex-1 items-center gap-2 text-left"
                >
                  <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="flex min-w-0 flex-col">
                    {/* Lead with the file name (the user searches by name); the
                        matched content line + exact path follow (omnisearch). */}
                    <span className="truncate text-xs text-foreground">{noteWikiName(hit)}</span>
                    {hit.lineText && hit.lineText !== noteWikiName(hit) && (
                      <span className="truncate text-[10px] text-muted-foreground">{hit.lineText}</span>
                    )}
                    <span className="truncate text-[9px] text-muted-foreground/50">{hit.path}</span>
                  </span>
                </button>
                <span
                  className={`flex shrink-0 items-center gap-1 ${
                    isSelected ? "opacity-100" : "opacity-0 group-hover:opacity-100"
                  }`}
                >
                  <ActionButton title="Pin to session (Shift+Enter)" onClick={() => void pinToSession(hit)}>
                    <Pin className="size-3" />
                  </ActionButton>
                  <ActionButton title="Send to agent (Enter)" onClick={() => void sendToAgent(hit)}>
                    <Send className="size-3" />
                  </ActionButton>
                  <ActionButton title="Cite in scratchpad (Alt+Enter)" onClick={() => void citeInScratchpad(hit)}>
                    <NotebookPen className="size-3" />
                  </ActionButton>
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function ActionButton({
  title,
  onClick,
  children,
}: {
  title: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            aria-label={title}
            onClick={onClick}
            className="rounded p-1 text-muted-foreground transition-colors hover:bg-background/80 hover:text-foreground"
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent side="bottom" className="text-[10px]">{title}</TooltipContent>
    </Tooltip>
  );
}
