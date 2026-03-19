import { useState } from "react";
import {
  CircleDot,
  GitBranch,
  Pencil,
  Trash2,
  Package,
  GitMerge,
  Check,
} from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";
import type { Session, Workspace } from "@/stores/workspace";

const STATUS_DOT_COLORS: Record<Session["status"], string> = {
  idle: "bg-muted-foreground/60",
  running: "bg-green-500",
  needs_attention: "bg-orange-500",
  completed: "bg-muted-foreground/30",
};

interface SessionRowProps {
  session: Session;
  workspace: Workspace;
  isActive: boolean;
  onSelect: () => void;
  onRename: (newName: string) => void;
  onDelete: () => void;
  onCommit: () => void;
  onMerge: () => void;
}

export function SessionRow({
  session,
  workspace: _workspace,
  isActive,
  onSelect,
  onRename,
  onDelete,
  onCommit,
  onMerge,
}: SessionRowProps) {
  const [editing, setEditing] = useState(false);
  const [editName, setEditName] = useState(session.name);
  const isCompleted = session.status === "completed";
  const isWorktree = session.worktree_path !== null;

  function handleRenameSubmit() {
    const trimmed = editName.trim();
    if (trimmed && trimmed !== session.name) {
      onRename(trimmed);
    }
    setEditing(false);
  }

  return (
    <button
      onClick={onSelect}
      className={`group flex w-full items-center gap-1.5 pl-7 pr-3 py-1 text-left transition-colors ${
        isActive
          ? "bg-secondary/60 text-foreground"
          : "hover:bg-secondary/40 text-foreground/70"
      } ${isCompleted ? "opacity-50" : ""}`}
    >
      {isCompleted ? (
        <Check className="size-3 shrink-0 text-muted-foreground/50" />
      ) : (
        <span
          className={`size-1.5 shrink-0 rounded-full ${STATUS_DOT_COLORS[session.status]} ${session.status === "running" ? "animate-dot-pulse" : ""}`}
          aria-hidden="true"
        />
      )}

      {editing ? (
        <input
          type="text"
          value={editName}
          onChange={(e) => setEditName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleRenameSubmit();
            if (e.key === "Escape") setEditing(false);
            e.stopPropagation();
          }}
          onBlur={handleRenameSubmit}
          onClick={(e) => e.stopPropagation()}
          className="flex-1 h-4 bg-transparent border-b border-border text-[11px] text-foreground outline-none"
          autoFocus
        />
      ) : (
        <span className="flex-1 truncate text-[11px]">{session.name}</span>
      )}

      <span className="shrink-0 text-[10px] text-muted-foreground/50 tabular-nums group-hover:hidden">
        {formatAge(session.updated_at)}
      </span>

      <span className="hidden shrink-0 items-center gap-0.5 group-hover:flex">
        <Tooltip>
          <TooltipTrigger>
            <span
              role="button"
              tabIndex={0}
              aria-label="Rename"
              onClick={(e) => { e.stopPropagation(); setEditName(session.name); setEditing(true); }}
              className="flex size-4 items-center justify-center text-muted-foreground hover:text-foreground transition-colors"
            >
              <Pencil className="size-2.5" />
            </span>
          </TooltipTrigger>
          <TooltipContent side="top" className="text-[10px]">Rename</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger>
            <span
              role="button"
              tabIndex={0}
              aria-label="Delete"
              onClick={(e) => { e.stopPropagation(); onDelete(); }}
              className="flex size-4 items-center justify-center text-muted-foreground hover:text-foreground transition-colors"
            >
              <Trash2 className="size-2.5" />
            </span>
          </TooltipTrigger>
          <TooltipContent side="top" className="text-[10px]">Delete</TooltipContent>
        </Tooltip>
        {isWorktree && (
          <>
            <Tooltip>
              <TooltipTrigger>
                <span
                  role="button"
                  tabIndex={0}
                  aria-label="Commit"
                  onClick={(e) => { e.stopPropagation(); onCommit(); }}
                  className="flex size-4 items-center justify-center text-muted-foreground hover:text-foreground transition-colors"
                >
                  <Package className="size-2.5" />
                </span>
              </TooltipTrigger>
              <TooltipContent side="top" className="text-[10px]">Commit</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger>
                <span
                  role="button"
                  tabIndex={0}
                  aria-label="Merge"
                  onClick={(e) => { e.stopPropagation(); onMerge(); }}
                  className="flex size-4 items-center justify-center text-muted-foreground hover:text-foreground transition-colors"
                >
                  <GitMerge className="size-2.5" />
                </span>
              </TooltipTrigger>
              <TooltipContent side="top" className="text-[10px]">Merge</TooltipContent>
            </Tooltip>
          </>
        )}
      </span>

      {isWorktree ? (
        <GitBranch className="size-3 shrink-0 text-muted-foreground/40" />
      ) : (
        <CircleDot className="size-3 shrink-0 text-muted-foreground/40" />
      )}
    </button>
  );
}

function formatAge(timestamp: number): string {
  const delta = Math.floor(Date.now() / 1000 - timestamp);
  if (delta < 60) return "now";
  if (delta < 3600) return `${Math.floor(delta / 60)}m`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h`;
  return `${Math.floor(delta / 86400)}d`;
}
