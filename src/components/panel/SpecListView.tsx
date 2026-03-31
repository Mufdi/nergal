import { useState, useEffect, useCallback } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { invoke, listen } from "@/lib/tauri";
import { activeSessionIdAtom } from "@/stores/workspace";
import { openTabAction } from "@/stores/rightPanel";
import { ClipboardList, Archive, ChevronRight, ChevronDown, BookOpen } from "lucide-react";

interface SpecEntry {
  name: string;
  path: string;
}

interface OpenSpecChange {
  name: string;
  status: string;
  created: string;
  artifacts: string[];
  specs: SpecEntry[];
}

export function SpecListView() {
  const sessionId = useAtomValue(activeSessionIdAtom);
  const openTab = useSetAtom(openTabAction);
  const [changes, setChanges] = useState<OpenSpecChange[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(() => {
    if (!sessionId) return;
    setLoading(true);
    invoke<OpenSpecChange[]>("list_openspec_changes", { sessionId })
      .then(setChanges)
      .catch(() => setChanges([]))
      .finally(() => setLoading(false));
  }, [sessionId]);

  // Refresh on mount, on openspec:changed (watcher), and on files:modified (hook)
  useEffect(() => {
    refresh();
    const unlisteners: (() => void)[] = [];

    listen("openspec:changed", () => refresh())
      .then((fn) => unlisteners.push(fn));

    listen<{ path: string }>("files:modified", (payload) => {
      if (payload.path.includes("/openspec/")) refresh();
    }).then((fn) => unlisteners.push(fn));

    return () => { for (const fn of unlisteners) fn(); };
  }, [refresh]);

  function handleClick(change: OpenSpecChange) {
    openTab({
      tab: {
        id: `spec-${change.name}`,
        type: "spec",
        label: change.name === "_master" ? "Specs" : change.name,
        data: { changeName: change.name, sessionId },
      },
    });
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[11px] text-muted-foreground">Loading...</span>
      </div>
    );
  }

  if (changes.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[11px] text-muted-foreground">No OpenSpec changes</span>
      </div>
    );
  }

  const master = changes.filter((c) => c.status === "master");
  const active = changes.filter((c) => c.status === "active");
  const archived = changes.filter((c) => c.status === "archived");

  return (
    <div className="flex flex-col py-1">
      {master.length > 0 && (
        <>
          <div className="px-3 py-1.5">
            <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
              Specs
            </span>
          </div>
          {master.map((change) => (
            <ChangeItem key={change.name} change={change} onClick={handleClick} />
          ))}
        </>
      )}
      {active.length > 0 && (
        <>
          <div className="px-3 py-1.5">
            <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
              Active
            </span>
          </div>
          {active.map((change) => (
            <ChangeItem key={change.name} change={change} onClick={handleClick} />
          ))}
        </>
      )}
      {archived.length > 0 && (
        <>
          <div className="px-3 py-1.5 mt-1">
            <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
              Archived
            </span>
          </div>
          {archived.map((change) => (
            <ChangeItem key={change.name} change={change} onClick={handleClick} />
          ))}
        </>
      )}
    </div>
  );
}

function ChangeItem({
  change,
  onClick,
}: {
  change: OpenSpecChange;
  onClick: (c: OpenSpecChange) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const openTab = useSetAtom(openTabAction);
  const sessionId = useAtomValue(activeSessionIdAtom);

  const Icon = change.status === "archived"
    ? Archive
    : change.status === "master"
      ? BookOpen
      : ClipboardList;
  const hasSpecs = change.specs.length > 0;
  const displayName = change.name === "_master" ? "Consolidated" : change.name;

  function handleSpecClick(spec: SpecEntry) {
    openTab({
      tab: {
        id: `spec-${change.name}-${spec.name}`,
        type: "spec",
        label: spec.name,
        data: { changeName: change.name, sessionId, specPath: spec.path },
      },
    });
  }

  return (
    <div>
      <div className="flex w-full items-center hover:bg-secondary/50 transition-colors" data-nav-expandable>
        {hasSpecs ? (
          <button
            onClick={(e) => { e.stopPropagation(); setExpanded(!expanded); }}
            className="flex size-6 shrink-0 items-center justify-center text-muted-foreground/60"
          >
            {expanded ? <ChevronDown size={10} /> : <ChevronRight size={10} />}
          </button>
        ) : (
          <div className="w-6 shrink-0" />
        )}
        <button
          data-nav-item
          onClick={() => onClick(change)}
          className="flex min-w-0 flex-1 items-center gap-1.5 py-1.5 pr-3 text-left"
        >
          <Icon size={12} className="shrink-0 text-muted-foreground" />
          <span className="truncate text-[11px] text-foreground">{displayName}</span>
          {change.specs.length > 0 && (
            <span className="ml-auto shrink-0 text-[10px] text-muted-foreground/60">{change.specs.length}</span>
          )}
        </button>
      </div>
      {expanded && hasSpecs && (
        <div className="ml-6 border-l border-border/30 pl-1">
          {change.specs.map((spec) => (
            <button
              key={spec.path}
              data-nav-item
              onClick={() => handleSpecClick(spec)}
              className="flex w-full items-center gap-1.5 px-2 py-1 text-left transition-colors hover:bg-secondary/50"
            >
              <ClipboardList size={10} className="shrink-0 text-muted-foreground/60" />
              <span className="truncate text-[10px] text-foreground/80">{spec.name}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
