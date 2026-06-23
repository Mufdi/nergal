import { useState } from "react";
import { useAtomValue } from "jotai";
import {
  CircleDot,
  GitBranch,
  MessagesSquare,
  Pencil,
  Trash2,
} from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from "@/components/ui/tooltip";
import type { Session, Workspace } from "@/stores/workspace";
import { gitInfoMapAtom } from "@/stores/git";
import { pendingAsksAtom, pendingAttentionAtom } from "@/stores/askUser";
import { crossSessionUnreadMapAtom, crossSessionActiveParticipantsAtom } from "@/stores/crossSession";
import { SessionIndicator } from "./SessionIndicator";
import claudeIcon from "@/assets/agents/claude.svg";
import codexIcon from "@/assets/agents/codex.png";
import opencodeIcon from "@/assets/agents/opencode.svg";
import piIcon from "@/assets/agents/pi.svg";

const AGENT_ICONS: Record<string, { src: string; label: string }> = {
  "claude-code": { src: claudeIcon, label: "Claude Code" },
  codex: { src: codexIcon, label: "Codex" },
  opencode: { src: opencodeIcon, label: "OpenCode" },
  pi: { src: piIcon, label: "Pi" },
};

interface SessionRowProps {
  session: Session;
  workspace: Workspace;
  isActive: boolean;
  shortcutNumber?: number;
  onSelect: () => void;
  onRename: (newName: string) => void;
  onDelete: () => void;
}

export function SessionRow({
  session,
  workspace: _workspace,
  isActive,
  shortcutNumber,
  onSelect,
  onRename,
  onDelete,
}: SessionRowProps) {
  const [editing, setEditing] = useState(false);
  const [editName, setEditName] = useState(session.name);
  const gitInfoMap = useAtomValue(gitInfoMapAtom);
  const gitInfo = gitInfoMap[session.id];
  const linesAdded = gitInfo?.lines_added ?? 0;
  const linesRemoved = gitInfo?.lines_removed ?? 0;
  const hasLineChanges = linesAdded > 0 || linesRemoved > 0;
  const isCompleted = session.status === "completed";
  const pendingAsks = useAtomValue(pendingAsksAtom);
  const pendingAttention = useAtomValue(pendingAttentionAtom);
  const isAwaiting = !!pendingAsks[session.id] || !!pendingAttention[session.id];
  const crossUnread = useAtomValue(crossSessionUnreadMapAtom)[session.id] ?? 0;
  const inCrossThread = useAtomValue(crossSessionActiveParticipantsAtom).has(session.id);

  function handleRenameSubmit() {
    const trimmed = editName.trim();
    if (trimmed && trimmed !== session.name) {
      onRename(trimmed);
    }
    setEditing(false);
  }

  return (
    <div
      role="button"
      tabIndex={0}
      data-nav-item
      onClick={onSelect}
      onKeyDown={(e) => { if (e.key === "Enter") onSelect(); }}
      className={`group flex w-full items-center gap-1.5 ${shortcutNumber != null ? "pl-3" : "pl-7"} pr-3 py-1 text-left transition-colors cursor-pointer ${
        isActive
          ? "bg-secondary/60 text-foreground shadow-[inset_2px_0_0_0_var(--color-primary)]"
          : "hover:bg-secondary/60 hover:text-foreground text-foreground/70"
      } ${isCompleted ? "opacity-50" : ""} ${isAwaiting ? "cluihud-ask-pending" : ""}`}
    >
      {shortcutNumber != null && (
        <Tooltip>
          <TooltipTrigger render={<span className="flex size-4 shrink-0 items-center justify-center rounded-sm bg-muted/50 text-[9px] font-medium tabular-nums text-muted-foreground" />}>
            {shortcutNumber}
          </TooltipTrigger>
          <TooltipContent side="top" className="text-[10px]">Ctrl+{shortcutNumber}</TooltipContent>
        </Tooltip>
      )}
      {(() => {
        const agent = AGENT_ICONS[session.agent_id ?? "claude-code"];
        if (!agent) return null;
        return (
          <Tooltip>
            <TooltipTrigger render={<span className="inline-flex shrink-0" />}>
              <img
                src={agent.src}
                alt={agent.label}
                className="size-3.5 shrink-0"
                draggable={false}
              />
            </TooltipTrigger>
            <TooltipContent side="top" className="text-[10px]">{agent.label}</TooltipContent>
          </Tooltip>
        );
      })()}
      <SessionIndicator sessionId={session.id} sessionStatus={session.status} size="sm" />

      {(inCrossThread || crossUnread > 0) && (
        <Tooltip>
          <TooltipTrigger
            render={
              <span
                className={`flex shrink-0 items-center gap-0.5 rounded-full px-1 py-px ${
                  crossUnread > 0 ? "bg-primary/20 text-primary" : "bg-muted/50 text-muted-foreground"
                }`}
              />
            }
          >
            <MessagesSquare className="size-2.5" />
            {crossUnread > 0 && (
              <span className="text-[9px] font-semibold leading-none tabular-nums">{crossUnread}</span>
            )}
          </TooltipTrigger>
          <TooltipContent side="top" className="text-[10px]">
            {crossUnread > 0
              ? `${crossUnread} unread cross-session message${crossUnread > 1 ? "s" : ""}`
              : "In a live cross-session conversation"}
          </TooltipContent>
        </Tooltip>
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
        <TooltipProvider delay={0}>
          <Tooltip>
            <TooltipTrigger render={<span className="flex-1 truncate text-[11px]" />}>
              {session.name}
            </TooltipTrigger>
            <TooltipContent side="right" className="text-[10px]">
              {session.name}
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      )}

      <span data-row-meta className="shrink-0 flex items-center gap-1.5 text-[10px] tabular-nums group-hover:hidden">
        {hasLineChanges && (
          <span className="flex items-center gap-1">
            <span className="text-green-500">+{compactCount(linesAdded)}</span>
            <span className="text-red-500">-{compactCount(linesRemoved)}</span>
          </span>
        )}
        {!hasLineChanges && (
          <span className="text-muted-foreground/50">{formatAge(session.updated_at)}</span>
        )}
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
        {/* Delete only for worktree sessions — the main session is the repo root,
            not a disposable worktree, so deleting it from the sidebar is wrong. */}
        {session.worktree_path !== null && (
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
        )}
      </span>

      {session.worktree_path !== null ? (
        <GitBranch className="size-3 shrink-0 text-muted-foreground/40" />
      ) : (
        <CircleDot className="size-3 shrink-0 text-muted-foreground/40" />
      )}
    </div>
  );
}

function formatAge(timestamp: number): string {
  const delta = Math.floor(Date.now() / 1000 - timestamp);
  if (delta < 60) return "now";
  if (delta < 3600) return `${Math.floor(delta / 60)}m`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h`;
  return `${Math.floor(delta / 86400)}d`;
}

function compactCount(n: number): string {
  if (n < 1000) return n.toString();
  if (n < 10000) return `${(n / 1000).toFixed(1).replace(/\.0$/, "")}k`;
  return `${Math.floor(n / 1000)}k`;
}
