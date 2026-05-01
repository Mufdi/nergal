import { useCallback } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeConflictedFilesAtom, gitChipModeAtom } from "@/stores/git";
import { workspacesAtom } from "@/stores/workspace";
import { ConflictsPanel } from "@/components/git/ConflictsPanel";
import { CheckCircle2 } from "lucide-react";

interface ConflictsChipProps {
  sessionId: string;
}

export function ConflictsChip({ sessionId }: ConflictsChipProps) {
  const conflicts = useAtomValue(activeConflictedFilesAtom);
  const workspaces = useAtomValue(workspacesAtom);
  const setChipModeMap = useSetAtom(gitChipModeAtom);

  const onResolved = useCallback(() => {
    let workspaceId: string | null = null;
    for (const ws of workspaces) {
      if (ws.sessions.some((s) => s.id === sessionId)) {
        workspaceId = ws.id;
        break;
      }
    }
    if (!workspaceId) return;
    setChipModeMap((prev) => ({ ...prev, [workspaceId!]: "prs" }));
  }, [workspaces, sessionId, setChipModeMap]);

  if (conflicts.length === 0) {
    return (
      <div className="flex h-full items-center justify-center px-6">
        <div className="text-center">
          <CheckCircle2 size={20} className="mx-auto mb-2 text-green-400/60" />
          <p className="text-[11px] text-muted-foreground/80">No conflicts</p>
          <p className="mt-1 text-[10px] text-muted-foreground/50">All clear · resolving any will land you on PRs</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-hidden">
      <ConflictsPanel sessionId={sessionId} onResolved={onResolved} />
    </div>
  );
}
