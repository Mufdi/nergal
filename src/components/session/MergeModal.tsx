import { useState, useEffect } from "react";
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
}

export function MergeModal({
  open,
  onOpenChange,
  session,
  workspaceId,
  onMerged,
}: MergeModalProps) {
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
      await invoke("merge_session", { sessionId: session.id, targetBranch });
      onMerged();
      onOpenChange(false);
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
          <select
            value={targetBranch}
            onChange={(e) => setTargetBranch(e.target.value)}
            className="h-8 rounded-md border border-border bg-card px-2 text-[11px] text-foreground outline-none focus:ring-1 focus:ring-orange-500/50"
            style={{ color: "#ededef", backgroundColor: "#141415" }}
          >
            {branches.map((b) => (
              <option key={b} value={b} style={{ color: "#ededef", backgroundColor: "#1c1c1e" }}>
                {b}
              </option>
            ))}
          </select>

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
