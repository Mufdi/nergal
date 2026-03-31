import { useState, useEffect } from "react";
import { useSetAtom } from "jotai";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import type { Session } from "@/stores/workspace";

interface MergeModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  session: Session;
  workspaceId: string;
  onMerged: () => void;
  onConflict: (targetBranch: string, detail: string) => void;
}

export function MergeModal({
  open,
  onOpenChange,
  session,
  workspaceId,
  onMerged,
  onConflict,
}: MergeModalProps) {
  const addToast = useSetAtom(toastsAtom);
  const [branches, setBranches] = useState<string[]>([]);
  const [targetBranch, setTargetBranch] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [merging, setMerging] = useState(false);

  useEffect(() => {
    if (!open) return;
    setError(null);
    setMerging(false);
    invoke<string[]>("list_branches", { workspaceId })
      .then((result) => {
        // Filter out cluihud/* worktree branches
        const filtered = result.filter((b) => !b.startsWith("cluihud/"));
        setBranches(filtered);
        if (filtered.length > 0 && !targetBranch) {
          setTargetBranch(filtered[0]);
        }
      })
      .catch(() => setBranches([]));
  }, [open, workspaceId]);

  async function handleMerge() {
    if (!targetBranch) return;
    setMerging(true);
    setError(null);
    try {
      const result = await invoke<{ success: boolean; conflict: boolean; message: string }>("merge_session", { sessionId: session.id, targetBranch });
      if (result.success) {
        addToast({ message: "Merge Complete", description: result.message, type: "success" });
        onMerged();
        onOpenChange(false);
      } else if (result.conflict) {
        onConflict(targetBranch, result.message);
        onOpenChange(false);
      } else {
        setError(result.message);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.toLowerCase().includes("conflict")) {
        setError("Merge conflicts detected. Resolve them in the terminal.");
      } else {
        setError(msg);
      }
    } finally {
      setMerging(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            Merge "{session.worktree_branch ?? session.name}"
          </DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <label className="text-[11px] text-muted-foreground">
            Merge into:
          </label>
          <div className="flex flex-col gap-1 max-h-48 overflow-y-auto">
            {branches.map((b) => (
              <button
                key={b}
                onClick={() => setTargetBranch(b)}
                className={`rounded-md px-3 py-2 text-left text-[11px] transition-colors ${
                  targetBranch === b
                    ? "border border-orange-500 bg-orange-500/10 text-foreground"
                    : "border border-border bg-card text-foreground/70 hover:bg-[#1c1c1e] hover:text-foreground"
                }`}
              >
                {b}
              </button>
            ))}
          </div>

          {error && (
            <p className="text-[11px] text-red-400">{error}</p>
          )}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            size="sm"
            onClick={() => onOpenChange(false)}
          >
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={handleMerge}
            disabled={merging || !targetBranch}
          >
            {merging ? "Merging..." : "Merge"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
