import { useState, useEffect, useRef } from "react";
import { useSetAtom } from "jotai";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import { KeyHints } from "@/components/ui/KeyHints";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
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
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    setError(null);
    setMerging(false);
    invoke<string[]>("list_branches", { workspaceId })
      .then((result) => {
        // Filter out nergal/* worktree branches
        const filtered = result.filter((b) => !b.startsWith("nergal/"));
        setBranches(filtered);
        if (filtered.length > 0 && !targetBranch) {
          setTargetBranch(filtered[0]);
        }
        requestAnimationFrame(() => listRef.current?.focus());
      })
      .catch(() => setBranches([]));
  }, [open, workspaceId]);

  function handleListKeyDown(e: React.KeyboardEvent<HTMLDivElement>) {
    if (branches.length === 0) return;
    const idx = branches.indexOf(targetBranch);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      e.stopPropagation();
      const next = idx < 0 ? 0 : (idx + 1) % branches.length;
      setTargetBranch(branches[next]);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      const prev = idx < 0 ? branches.length - 1 : (idx - 1 + branches.length) % branches.length;
      setTargetBranch(branches[prev]);
    } else if (e.key === "Enter" && targetBranch && !merging) {
      // BUG-16: bubbling would click the hovered sidebar row.
      e.preventDefault();
      e.stopPropagation();
      handleMerge();
    }
  }

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
          <div
            ref={listRef}
            tabIndex={0}
            onKeyDown={handleListKeyDown}
            className="flex flex-col gap-1 max-h-48 overflow-y-auto outline-none rounded focus:ring-1 focus:ring-orange-500/50"
          >
            {branches.map((b) => (
              <button
                key={b}
                onClick={() => setTargetBranch(b)}
                className={`rounded-md px-3 py-2 text-left text-[11px] transition-colors ${
                  targetBranch === b
                    ? "border border-orange-500 bg-orange-500/10 text-foreground"
                    : "border border-border bg-card text-foreground/70 hover:bg-secondary hover:text-foreground"
                }`}
              >
                {b}
              </button>
            ))}
          </div>
          <KeyHints
            hints={[
              { keys: "↑↓", label: "navigate" },
              { keys: "Enter", label: "merge" },
              { keys: "Esc", label: "cancel" },
            ]}
          />

          {error && (
            <p className="text-[11px] text-red-400">{error}</p>
          )}
        </div>

        <DialogFooter className="flex-nowrap gap-1.5">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => onOpenChange(false)}
          >
            Cancel <Kbd keys="esc" className="ml-1.5" />
          </Button>
          <Button
            size="sm"
            onClick={handleMerge}
            disabled={merging || !targetBranch}
          >
            {merging ? "Merging..." : "Merge"}
            <Kbd keys="enter" tone="onPrimary" className="ml-1.5" />
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
