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
      {/* Status indicator */}
      {isCompleted ? (
        <Check className="size-3 shrink-0 text-muted-foreground/50" />
      ) : (
        <span
          className={`size-1.5 shrink-0 rounded-full ${STATUS_DOT_COLORS[session.status]}`}
          aria-hidden="true"
        />
      )}

      {/* Name or inline edit */}
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

      {/* Age */}
      <span className="shrink-0 text-[10px] text-muted-foreground/50 tabular-nums group-hover:hidden">
        {formatAge(session.updated_at)}
      </span>

      {/* Hover actions */}
      <span className="hidden shrink-0 items-center gap-0.5 group-hover:flex">
        <ActionIcon
          icon={Pencil}
          label="Rename"
          onClick={(e) => {
            e.stopPropagation();
            setEditName(session.name);
            setEditing(true);
          }}
        />
        <ActionIcon
          icon={Trash2}
          label="Delete"
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
        />
        {isWorktree && (
          <>
            <ActionIcon
              icon={Package}
              label="Commit"
              onClick={(e) => {
                e.stopPropagation();
                onCommit();
              }}
            />
            <ActionIcon
              icon={GitMerge}
              label="Merge"
              onClick={(e) => {
                e.stopPropagation();
                onMerge();
              }}
            />
          </>
        )}
      </span>

      {/* Type icon */}
      {isWorktree ? (
        <GitBranch className="size-3 shrink-0 text-muted-foreground/40" />
      ) : (
        <CircleDot className="size-3 shrink-0 text-muted-foreground/40" />
      )}
    </button>
  );
}

function ActionIcon({
  icon: Icon,
  label,
  onClick,
}: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  onClick: (e: React.MouseEvent) => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <span
            role="button"
            tabIndex={0}
            aria-label={label}
            onClick={onClick}
            className="flex size-4 items-center justify-center text-muted-foreground hover:text-foreground transition-colors"
          />
        }
      >
        <Icon className="size-2.5" />
      </TooltipTrigger>
      <TooltipContent side="bottom" className="text-[10px]">{label}</TooltipContent>
    </Tooltip>
  );
}

function formatAge(timestamp: number): string {
  const delta = Math.floor(Date.now() / 1000 - timestamp);
  if (delta < 60) return "now";
  if (delta < 3600) return `${Math.floor(delta / 60)}m`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h`;
  return `${Math.floor(delta / 86400)}d`;
}
