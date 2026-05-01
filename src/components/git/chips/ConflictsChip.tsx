import { useAtomValue, useSetAtom } from "jotai";
import { activeConflictedFilesAtom } from "@/stores/git";
import { openConflictsTabAction } from "@/stores/conflict";

interface ConflictsChipProps {
  sessionId: string;
}

/// Phase 7 stub: routes the user to the existing conflicts tab so the
/// resolution flow keeps working during the migration. Phase 7 replaces this
/// with an embedded picker + ConflictsPanel viewer.
export function ConflictsChip({ sessionId }: ConflictsChipProps) {
  const conflicts = useAtomValue(activeConflictedFilesAtom);
  const openConflictsTab = useSetAtom(openConflictsTabAction);

  if (conflicts.length === 0) {
    return (
      <div className="flex h-full items-center justify-center px-6">
        <div className="text-center">
          <p className="text-[11px] text-muted-foreground/80">No conflicts</p>
          <p className="mt-1 text-[10px] text-muted-foreground/50">All clear</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center justify-between border-b border-border/40 px-3 py-1">
        <span className="text-[10px] font-medium uppercase tracking-wider text-yellow-400">
          Conflicts ({conflicts.length})
        </span>
        <span className="text-[9px] text-muted-foreground/50">embedded viewer in phase 7</span>
      </div>
      <div className="flex-1 overflow-y-auto">
        {conflicts.map((path) => {
          const name = path.split("/").pop() ?? path;
          return (
            <button
              key={path}
              onClick={() => openConflictsTab({ sessionId, path })}
              className="flex w-full items-center gap-2 px-3 py-1 text-left transition-colors hover:bg-secondary/30"
            >
              <span className="shrink-0 font-mono text-[10px] font-bold text-yellow-400">C</span>
              <span className="min-w-0 flex-1 truncate text-[11px] text-foreground/80" title={path}>
                {name}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
