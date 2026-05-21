import { useEffect, useCallback, useRef, useState, useMemo } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { activeSessionIdAtom } from "@/stores/workspace";
import { fileBrowserStateMapAtom, openTabAction } from "@/stores/rightPanel";
import { invoke } from "@/lib/tauri";
import { ChevronRight, ChevronDown, File, Folder, Search } from "lucide-react";

interface DirEntry {
  name: string;
  is_dir: boolean;
  path: string;
}

const FILE_ICONS: Record<string, string> = {
  ts: "text-blue-400",
  tsx: "text-blue-400",
  js: "text-yellow-400",
  jsx: "text-yellow-400",
  rs: "text-orange-400",
  json: "text-green-400",
  md: "text-foreground/60",
  css: "text-purple-400",
  html: "text-red-400",
  toml: "text-muted-foreground",
};

const EMPTY_STATE = {
  rootEntries: [] as DirEntry[],
  expanded: [] as string[],
  children: {} as Record<string, DirEntry[]>,
  lastOpened: null as string | null,
};

export function FileBrowser() {
  const sessionId = useAtomValue(activeSessionIdAtom);
  const openTab = useSetAtom(openTabAction);
  const [stateMap, setStateMap] = useAtom(fileBrowserStateMapAtom);
  const persisted = sessionId ? stateMap[sessionId] ?? EMPTY_STATE : EMPTY_STATE;
  const { rootEntries, children, lastOpened } = persisted;
  const expanded = useMemo(() => new Set(persisted.expanded), [persisted.expanded]);
  const [filter, setFilter] = useState("");
  const [searchHits, setSearchHits] = useState<DirEntry[]>([]);
  const [searching, setSearching] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);

  const updateSession = useCallback(
    (
      partial: Partial<typeof EMPTY_STATE> | ((prev: typeof EMPTY_STATE) => typeof EMPTY_STATE),
    ) => {
      if (!sessionId) return;
      setStateMap((prev) => {
        const current = prev[sessionId] ?? EMPTY_STATE;
        const next =
          typeof partial === "function" ? partial(current) : { ...current, ...partial };
        return { ...prev, [sessionId]: next };
      });
    },
    [sessionId, setStateMap],
  );

  useEffect(() => {
    searchRef.current?.focus();
    if (lastOpened) {
      searchRef.current?.select();
    }
  }, [lastOpened]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      // Keeps "type to search" alive after the user navigates into the
      // tree with arrows — without this, the input loses focus on the
      // first arrow press and typing falls into the void.
      if (e.key.length !== 1) return;
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)) return;
      e.preventDefault();
      searchRef.current?.focus();
      setFilter((prev) => prev + e.key);
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  useEffect(() => {
    if (!sessionId) return;
    if (rootEntries.length > 0) return;
    invoke<DirEntry[]>("list_directory", { sessionId, path: "." })
      .then((entries) => updateSession({ rootEntries: entries }))
      .catch(() => {});
  }, [sessionId, rootEntries.length, updateSession]);

  useEffect(() => {
    const q = filter.trim();
    if (q.length === 0 || !sessionId) {
      setSearchHits([]);
      setSearching(false);
      return;
    }
    const handle = setTimeout(() => {
      setSearching(true);
      invoke<DirEntry[]>("search_files", { sessionId, query: q })
        .then(setSearchHits)
        .catch(() => setSearchHits([]))
        .finally(() => setSearching(false));
    }, 150);
    return () => clearTimeout(handle);
  }, [filter, sessionId]);

  const loadDir = useCallback(
    (dirPath: string) => {
      if (!sessionId || children[dirPath]) return;
      invoke<DirEntry[]>("list_directory", { sessionId, path: dirPath })
        .then((entries) =>
          updateSession((prev) => ({
            ...prev,
            children: { ...prev.children, [dirPath]: entries },
          })),
        )
        .catch(() => {});
    },
    [sessionId, children, updateSession],
  );

  function toggleDir(dirPath: string) {
    updateSession((prev) => {
      const next = new Set(prev.expanded);
      if (next.has(dirPath)) {
        next.delete(dirPath);
      } else {
        next.add(dirPath);
      }
      return { ...prev, expanded: Array.from(next) };
    });
    if (!expanded.has(dirPath)) loadDir(dirPath);
  }

  function openFile(filePath: string) {
    if (!sessionId) return;
    const name = filePath.split("/").pop() ?? filePath;
    openTab({
      tab: { id: `file:${filePath}`, type: "file", label: name, data: { path: filePath, sessionId } },
    });
    updateSession({ lastOpened: filePath });
  }

  function renderEntries(entries: DirEntry[], depth: number) {
    return entries.map((entry) => {
      const ext = entry.name.split(".").pop()?.toLowerCase() ?? "";
      const iconColor = FILE_ICONS[ext] ?? "text-muted-foreground";
      const isExpanded = expanded.has(entry.path);
      const isLastOpened = entry.path === lastOpened;

      if (entry.is_dir) {
        return (
          <div key={entry.path}>
            <button
              type="button"
              data-nav-item
              data-nav-dir={entry.path}
              data-nav-expanded={isExpanded ? "true" : "false"}
              onClick={() => toggleDir(entry.path)}
              className="flex w-full items-center gap-1 rounded px-1 py-0.5 text-left text-xs text-foreground/80 hover:bg-secondary/50 transition-colors"
              style={{ paddingLeft: `${depth * 12 + 4}px` }}
            >
              {isExpanded ? <ChevronDown className="size-3 text-muted-foreground" /> : <ChevronRight className="size-3 text-muted-foreground" />}
              <Folder className="size-3 text-primary/60" />
              <span className="truncate">{entry.name}</span>
            </button>
            {isExpanded && children[entry.path] && renderEntries(children[entry.path], depth + 1)}
          </div>
        );
      }

      return (
        <button
          key={entry.path}
          type="button"
          data-nav-item
          onClick={() => openFile(entry.path)}
          className={`flex w-full items-center gap-1 rounded px-1 py-0.5 text-left text-xs hover:bg-secondary/50 hover:text-foreground transition-colors ${
            isLastOpened ? "bg-secondary/40 text-foreground" : "text-foreground/70"
          }`}
          style={{ paddingLeft: `${depth * 12 + 16}px` }}
        >
          <File className={`size-3 ${iconColor}`} />
          <span className="truncate">{entry.name}</span>
        </button>
      );
    });
  }

  function renderSearchHits() {
    if (searchHits.length === 0) {
      return (
        <p className="px-2 py-4 text-center text-xs text-muted-foreground">
          {searching ? "Searching…" : "No matches"}
        </p>
      );
    }
    return searchHits.map((hit) => {
      const ext = hit.name.split(".").pop()?.toLowerCase() ?? "";
      const iconColor = FILE_ICONS[ext] ?? "text-muted-foreground";
      const dir = hit.path.includes("/") ? hit.path.slice(0, hit.path.lastIndexOf("/")) : "";
      return (
        <button
          key={hit.path}
          type="button"
          data-nav-item
          onClick={() => openFile(hit.path)}
          className="flex w-full items-center gap-1.5 rounded px-2 py-0.5 text-left text-xs text-foreground/80 hover:bg-secondary/50 hover:text-foreground transition-colors"
        >
          <File className={`size-3 shrink-0 ${iconColor}`} />
          <span className="truncate">{hit.name}</span>
          {dir && (
            <span className="ml-auto truncate text-[10px] text-muted-foreground/70">{dir}</span>
          )}
        </button>
      );
    });
  }

  const showSearch = filter.trim().length > 0;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-1.5 border-b border-border/50 px-2 py-1.5">
        <Search className="size-3 text-muted-foreground" />
        <input
          ref={searchRef}
          type="text"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Search files..."
          className="flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground/50"
        />
      </div>

      <div className="flex-1 overflow-y-auto px-1 py-1">
        {showSearch ? (
          renderSearchHits()
        ) : rootEntries.length === 0 ? (
          <p className="px-2 py-4 text-center text-xs text-muted-foreground">No files</p>
        ) : (
          renderEntries(rootEntries, 0)
        )}
      </div>
    </div>
  );
}
