import { useCallback } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeConflictedFilesAtom, gitChipModeAtom } from "@/stores/git";
import { ConflictsPanel } from "@/components/git/ConflictsPanel";
import { CheckCircle2 } from "lucide-react";

interface ConflictsChipProps {
  sessionId: string;
}

export function ConflictsChip({ sessionId }: ConflictsChipProps) {
  const conflicts = useAtomValue(activeConflictedFilesAtom);
  const setChipModeMap = useSetAtom(gitChipModeAtom);

  const onResolved = useCallback(() => {
    setChipModeMap((prev) => ({ ...prev, [sessionId]: "prs" }));
  }, [sessionId, setChipModeMap]);

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
