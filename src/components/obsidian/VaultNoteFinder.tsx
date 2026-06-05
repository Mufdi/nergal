import { useCallback, useEffect, useRef, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { FileText, Pin, PinOff, Search } from "lucide-react";
import { activeWorkspaceAtom, activeSessionIdAtom } from "@/stores/workspace";
import {
  activeSessionPinnedNotesAtom,
  pinNoteAtom,
  unpinNoteAtom,
} from "@/stores/pinnedNotes";
import { openTabAction } from "@/stores/rightPanel";
import { type SearchHit } from "@/stores/search";
import {
  obsidianConfigAtom,
  vaultSearchScopeAtom,
} from "@/stores/obsidian";
import { toastsAtom } from "@/stores/toast";
import { focusZoneAtom } from "@/stores/shortcuts";
import { appStore } from "@/stores/jotaiStore";
import { invoke } from "@/lib/tauri";

const DEBOUNCE_MS = 200;

function noteName(path: string): string {
  const base = path.split("/").pop() ?? path;
  return base.replace(/\.md$/i, "");
}

/// Vault-note finder card, rendered inside the right panel: picker overlay
/// over a note tab (Ctrl+Shift+K) or panel-view content when no note tab
/// exists (Ctrl+Shift+Q). Pinned notes live as pinned tabs, so the finder
/// carries no pinned section.
export function VaultNoteFinder({ onClose, className }: { onClose: () => void; className?: string }) {
  const workspace = useAtomValue(activeWorkspaceAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const cfg = useAtomValue(obsidianConfigAtom);
  const pinned = useAtomValue(activeSessionPinnedNotesAtom);
  const pinNote = useSetAtom(pinNoteAtom);
  const unpinNote = useSetAtom(unpinNoteAtom);
  const openTab = useSetAtom(openTabAction);
  const setToasts = useSetAtom(toastsAtom);

  const scopeMode = useAtomValue(vaultSearchScopeAtom);
  const setScopeMode = useSetAtom(vaultSearchScopeAtom);
  const subdir = cfg?.search_subdir?.trim() || null;

  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchHit[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const seqRef = useRef(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Autofocus only when the panel is the active zone (user opened the
  // finder). The finder also mounts on session switches that restore an
  // obsidiannote view — stealing focus there broke the "switch lands on
  // the terminal prompt" contract (BUG-09 v0.2.0).
  useEffect(() => {
    requestAnimationFrame(() => {
      if (appStore.get(focusZoneAtom) === "panel") inputRef.current?.focus();
    });
  }, []);

  const runSearch = useCallback(async () => {
    const text = query.trim();
    const seq = ++seqRef.current;
    if (!text) {
      setResults([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    const vaultSubdir = scopeMode === "subdir" && subdir ? subdir : null;
    try {
      const hits = await invoke<SearchHit[]>("search", {
        query: { text, scopes: [{ kind: "vault" }], maxResults: 50 },
        activeWorkspaceId: workspace?.id ?? null,
        vaultSubdir,
      });
      if (seq === seqRef.current) {
        setResults(hits);
        setSelectedIndex(0);
      }
    } catch {
      if (seq === seqRef.current) setResults([]);
    } finally {
      if (seq === seqRef.current) setLoading(false);
    }
  }, [query, scopeMode, subdir, workspace?.id]);

  useEffect(() => {
    const handle = setTimeout(() => void runSearch(), DEBOUNCE_MS);
    return () => clearTimeout(handle);
  }, [runSearch]);

  useEffect(() => {
    const sel = listRef.current?.querySelector("[data-note-selected]") as HTMLElement | null;
    sel?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const openNote = useCallback(
    (path: string) => {
      openTab({
        tab: { id: `obsidiannote:${path}`, type: "obsidiannote", label: noteName(path), data: { path } },
      });
      setQuery("");
      setResults([]);
    },
    [openTab],
  );

  function toggleScope() {
    if (!subdir) return;
    setScopeMode((m) => (m === "subdir" ? "whole" : "subdir"));
    void runSearch();
  }

  async function pinToSession(hit: SearchHit) {
    if (!sessionId) {
      setToasts({ message: "Pin to session", description: "No active session.", type: "info" });
      return;
    }
    try {
      await pinNote({ sessionId, path: hit.path });
      openNote(hit.path);
    } catch (err) {
      setToasts({ message: "Pin failed", description: String(err), type: "error" });
    }
  }

  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.code === "KeyD") {
      e.preventDefault();
      toggleScope();
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((p) => Math.min(p + 1, results.length - 1));
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((p) => Math.max(p - 1, 0));
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      const hit = results[selectedIndex];
      if (!hit) return;
      if (e.shiftKey) void pinToSession(hit);
      else openNote(hit.path);
    }
  }

  if (!cfg?.vault_root) {
    return (
      <div className={`cluihud-glow rounded-lg border-2 border-primary bg-card px-4 py-3 text-xs text-muted-foreground ${className ?? ""}`}>
        Configure a vault in Settings → Obsidian.
      </div>
    );
  }

  return (
    <div
      role="dialog"
      aria-label="Find a vault note"
      className={`cluihud-glow flex max-h-[70%] w-full max-w-sm flex-col rounded-lg border-2 border-primary bg-card shadow-lg ${className ?? ""}`}
      onClick={(e) => e.stopPropagation()}
    >
      <div className="flex items-center gap-2 border-b border-border/50 px-3 py-2">
        <Search className="size-4 shrink-0 text-muted-foreground" />
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Find a vault note…"
          aria-label="Find a vault note"
          // Routes focusZone("panel") straight to the input.
          data-nav-container
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
          <div className="flex items-center justify-center py-6 text-xs text-muted-foreground">
            Searching…
          </div>
        )}
        {query.trim() === "" && results.length === 0 && (
          <div className="flex flex-col items-center gap-1 py-6">
            <span className="text-xs text-muted-foreground">Type to find a vault note</span>
            <span className="text-[10px] text-muted-foreground/60">
              Enter: open · Shift+Enter: pin
            </span>
          </div>
        )}
        {!loading && query.trim() !== "" && results.length === 0 && (
          <div className="flex items-center justify-center py-6 text-xs text-muted-foreground">
            No notes found
          </div>
        )}
        {results.map((hit, idx) => {
          const isPinned = pinned.includes(hit.path);
          const isSelected = idx === selectedIndex;
          return (
            <div
              key={`${hit.path}:${hit.lineNumber ?? 0}`}
              data-note-selected={isSelected ? "true" : undefined}
              onMouseEnter={() => setSelectedIndex(idx)}
              className={`group flex items-center gap-2 px-3 py-1.5 transition-colors ${
                isSelected ? "bg-secondary" : "hover:bg-secondary/50"
              }`}
            >
              <button
                type="button"
                onClick={() => openNote(hit.path)}
                className="flex min-w-0 flex-1 items-center gap-2 text-left"
              >
                <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="truncate text-xs text-foreground">
                  {hit.title ?? noteName(hit.path)}
                </span>
              </button>
              <button
                type="button"
                onClick={() =>
                  sessionId &&
                  (isPinned
                    ? unpinNote({ sessionId, path: hit.path })
                    : void pinToSession(hit))
                }
                disabled={!sessionId}
                title={isPinned ? "Unpin from session" : "Pin to session (Shift+Enter)"}
                className="rounded p-1 text-muted-foreground opacity-0 transition hover:bg-background/80 hover:text-foreground group-hover:opacity-100 disabled:opacity-30"
              >
                {isPinned ? <PinOff className="size-3" /> : <Pin className="size-3" />}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
