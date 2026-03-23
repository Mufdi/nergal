import { useState, useEffect, useCallback } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionIdAtom } from "@/stores/workspace";
import { openTabAction } from "@/stores/rightPanel";
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

export function FileBrowser() {
  const sessionId = useAtomValue(activeSessionIdAtom);
  const openTab = useSetAtom(openTabAction);
  const [filter, setFilter] = useState("");
  const [rootEntries, setRootEntries] = useState<DirEntry[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [children, setChildren] = useState<Record<string, DirEntry[]>>({});

  useEffect(() => {
    if (!sessionId) return;
    invoke<DirEntry[]>("list_directory", { sessionId, path: "." })
      .then(setRootEntries)
      .catch(() => {});
  }, [sessionId]);

  const loadDir = useCallback((dirPath: string) => {
    if (!sessionId || children[dirPath]) return;
    invoke<DirEntry[]>("list_directory", { sessionId, path: dirPath })
      .then((entries) => setChildren((prev) => ({ ...prev, [dirPath]: entries })))
      .catch(() => {});
  }, [sessionId, children]);

  function toggleDir(dirPath: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(dirPath)) {
        next.delete(dirPath);
      } else {
        next.add(dirPath);
        loadDir(dirPath);
      }
      return next;
    });
  }

  function previewFile(filePath: string) {
    if (!sessionId) return;
    const name = filePath.split("/").pop() ?? filePath;
    openTab({
      tab: { id: `file:${filePath}`, type: "file", label: name, data: { path: filePath, sessionId } },
      isPinned: false,
    });
  }

  function pinFile(filePath: string) {
    if (!sessionId) return;
    const name = filePath.split("/").pop() ?? filePath;
    openTab({
      tab: { id: `file:${filePath}`, type: "file", label: name, data: { path: filePath, sessionId } },
      isPinned: true,
    });
  }

  function renderEntries(entries: DirEntry[], depth: number) {
    const filtered = filter
      ? entries.filter((e) => e.name.toLowerCase().includes(filter.toLowerCase()))
      : entries;

    return filtered.map((entry) => {
      const ext = entry.name.split(".").pop()?.toLowerCase() ?? "";
      const iconColor = FILE_ICONS[ext] ?? "text-muted-foreground";
      const isExpanded = expanded.has(entry.path);

      if (entry.is_dir) {
        return (
          <div key={entry.path}>
            <button
              type="button"
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
          onClick={() => previewFile(entry.path)}
          onDoubleClick={() => pinFile(entry.path)}
          className="flex w-full items-center gap-1 rounded px-1 py-0.5 text-left text-xs text-foreground/70 hover:bg-secondary/50 hover:text-foreground transition-colors"
          style={{ paddingLeft: `${depth * 12 + 16}px` }}
        >
          <File className={`size-3 ${iconColor}`} />
          <span className="truncate">{entry.name}</span>
        </button>
      );
    });
  }

  return (
    <div className="flex h-full flex-col">
      {/* Search */}
      <div className="flex items-center gap-1.5 border-b border-border/50 px-2 py-1.5">
        <Search className="size-3 text-muted-foreground" />
        <input
          type="text"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter files..."
          className="flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground/50"
        />
      </div>

      {/* Tree */}
      <div className="flex-1 overflow-y-auto px-1 py-1">
        {rootEntries.length === 0 ? (
          <p className="px-2 py-4 text-center text-xs text-muted-foreground">No files</p>
        ) : (
          renderEntries(rootEntries, 0)
        )}
      </div>
    </div>
  );
}
