import { useState, useEffect, useCallback } from "react";
import { invoke, listen } from "@/lib/tauri";
import { useSetAtom } from "jotai";
import { refreshGitInfoAtom } from "@/stores/git";
import { triggerShipAtom } from "@/stores/ship";
import { toastsAtom } from "@/stores/toast";
import { openZenModeAtom } from "@/stores/zenMode";
import { Textarea } from "@/components/ui/textarea";
import { Kbd } from "@/components/ui/kbd";
import { DiffView } from "@/components/plan/DiffView";
import {
  Plus,
  Minus,
  ChevronUp,
  ChevronDown,
  ChevronRight,
  Rocket,
  Upload,
} from "lucide-react";

interface ChangedFile {
  path: string;
  status: string;
}

interface GitFullStatus {
  staged: ChangedFile[];
  unstaged: ChangedFile[];
  untracked: string[];
}

const STATUS_COLORS: Record<string, string> = {
  Create: "text-green-400",
  Edit: "text-orange-400",
  Delete: "text-red-400",
  Rename: "text-blue-400",
};

const STATUS_LETTERS: Record<string, string> = {
  Create: "A",
  Edit: "M",
  Delete: "D",
  Rename: "R",
};

interface FilesChipProps {
  sessionId: string;
  ahead: number;
}

export function FilesChip({ sessionId, ahead }: FilesChipProps) {
  const [status, setStatus] = useState<GitFullStatus>({ staged: [], unstaged: [], untracked: [] });
  const [commitMsg, setCommitMsg] = useState("");
  const [committing, setCommitting] = useState(false);
  const [creatingPr] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedFile, setExpandedFile] = useState<string | null>(null);
  const refreshGit = useSetAtom(refreshGitInfoAtom);
  const triggerShip = useSetAtom(triggerShipAtom);
  const addToast = useSetAtom(toastsAtom);
  const openZenMode = useSetAtom(openZenModeAtom);

  const allFiles = [
    ...status.staged.map((f) => f.path),
    ...status.unstaged.map((f) => f.path),
    ...status.untracked,
  ];

  type NavEntry = { path: string; group: "staged" | "unstaged" | "untracked" };
  const flatNav: NavEntry[] = [
    ...status.staged.map((f) => ({ path: f.path, group: "staged" as const })),
    ...status.unstaged.map((f) => ({ path: f.path, group: "unstaged" as const })),
    ...status.untracked.map((p) => ({ path: p, group: "untracked" as const })),
  ];
  const [fileCursor, setFileCursor] = useState(0);
  useEffect(() => {
    if (fileCursor >= flatNav.length) setFileCursor(Math.max(0, flatNav.length - 1));
  }, [fileCursor, flatNav.length]);

  const refresh = useCallback(() => {
    invoke<GitFullStatus>("get_git_status", { sessionId })
      .then((s) => { setStatus(s); setError(null); })
      .catch((err: unknown) => setError(String(err)));
  }, [sessionId]);

  useEffect(() => {
    refresh();
    const unlisteners: (() => void)[] = [];
    listen("files:modified", () => refresh()).then((fn) => unlisteners.push(fn));
    const id = setInterval(() => { if (!committing) refresh(); }, 10000);
    return () => { for (const fn of unlisteners) fn(); clearInterval(id); };
  }, [refresh, committing]);

  function handleStageFile(path: string) {
    invoke("git_stage_file", { sessionId, path }).then(refresh).catch(() => {});
  }

  function handleUnstageFile(path: string) {
    invoke("git_unstage_file", { sessionId, path }).then(refresh).catch(() => {});
  }

  function handleStageAll() {
    invoke("git_stage_all", { sessionId }).then(refresh).catch(() => {});
  }

  function handleUnstageAll() {
    invoke("git_unstage_all", { sessionId }).then(refresh).catch(() => {});
  }

  function handleCommit() {
    if (!commitMsg.trim() || status.staged.length === 0) return;
    setCommitting(true);
    invoke<string>("git_commit", { sessionId, message: commitMsg.trim() })
      .then(() => {
        setCommitMsg("");
        refreshGit(sessionId);
        refresh();
      })
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setCommitting(false));
  }

  function handleShip(inlineMessage?: string | null) {
    triggerShip({ tick: Date.now(), sessionId, inlineMessage: inlineMessage ?? null });
  }

  function handlePush() {
    if (ahead === 0) {
      addToast({ message: "Push", description: "Nothing to push", type: "info" });
      return;
    }
    invoke("git_push", { sessionId })
      .then(() => { addToast({ message: "Push", description: "Pushed to remote", type: "success" }); refreshGit(sessionId); refresh(); })
      .catch((err: unknown) => addToast({ message: "Push failed", description: String(err), type: "error" }));
  }

  function openZen(filePath: string) {
    openZenMode({ filePath, sessionId, files: allFiles });
  }

  function toggleFileDiff(path: string) {
    setExpandedFile((prev) => (prev === path ? null : path));
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && e.shiftKey && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      const msg = commitMsg.trim();
      handleShip(msg && status.staged.length > 0 ? msg : null);
      return;
    }
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handleCommit();
    }
  }

  // List nav: ↑/↓ + j/k move, Space toggles stage, Enter opens Zen
  useEffect(() => {
    if (flatNav.length === 0) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
      if (e.code === "ArrowDown" || e.code === "KeyJ") {
        e.preventDefault();
        setFileCursor((i) => (i + 1) % flatNav.length);
        return;
      }
      if (e.code === "ArrowUp" || e.code === "KeyK") {
        e.preventDefault();
        setFileCursor((i) => (i - 1 + flatNav.length) % flatNav.length);
        return;
      }
      if (e.code === "Space") {
        e.preventDefault();
        const entry = flatNav[fileCursor];
        if (!entry) return;
        if (entry.group === "staged") handleUnstageFile(entry.path);
        else handleStageFile(entry.path);
        return;
      }
      if (e.code === "Enter") {
        const entry = flatNav[fileCursor];
        if (!entry) return;
        e.preventDefault();
        openZen(entry.path);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [flatNav.length, fileCursor]);

  const canCommit = status.staged.length > 0 && commitMsg.trim().length > 0 && !committing;
  const canPushOrShip = ahead > 0 || status.staged.length > 0;

  return (
    <div className="flex h-full flex-col">
      {error && (
        <div className="shrink-0 border-b border-border/50 px-3 py-1">
          <span className="text-[10px] text-red-400">{error}</span>
        </div>
      )}

      <div className="flex-1 overflow-y-auto">
        <FileSection
          title="Staged"
          count={status.staged.length}
          action={status.staged.length > 0 ? { label: "Unstage all", icon: <ChevronDown size={10} />, onClick: handleUnstageAll } : undefined}
        >
          {status.staged.map((f, i) => (
            <FileRow
              key={f.path}
              path={f.path}
              status={f.status}
              actionIcon={<Minus size={10} />}
              actionLabel="Unstage"
              expanded={expandedFile === f.path}
              isCursor={fileCursor === i}
              onAction={() => handleUnstageFile(f.path)}
              onToggleDiff={() => toggleFileDiff(f.path)}
              onOpenZen={() => openZen(f.path)}
              onCursor={() => setFileCursor(i)}
              sessionId={sessionId}
            />
          ))}
        </FileSection>

        <FileSection
          title="Changes"
          count={status.unstaged.length}
          action={status.unstaged.length > 0 ? { label: "Stage all", icon: <ChevronUp size={10} />, onClick: handleStageAll } : undefined}
        >
          {status.unstaged.map((f, i) => {
            const navIdx = status.staged.length + i;
            return (
              <FileRow
                key={f.path}
                path={f.path}
                status={f.status}
                actionIcon={<Plus size={10} />}
                actionLabel="Stage"
                expanded={expandedFile === f.path}
                isCursor={fileCursor === navIdx}
                onAction={() => handleStageFile(f.path)}
                onToggleDiff={() => toggleFileDiff(f.path)}
                onOpenZen={() => openZen(f.path)}
                onCursor={() => setFileCursor(navIdx)}
                sessionId={sessionId}
              />
            );
          })}
        </FileSection>

        {status.untracked.length > 0 && (
          <FileSection title="Untracked" count={status.untracked.length}>
            {status.untracked.map((path, i) => {
              const navIdx = status.staged.length + status.unstaged.length + i;
              return (
                <FileRow
                  key={path}
                  path={path}
                  status="Create"
                  actionIcon={<Plus size={10} />}
                  actionLabel="Stage"
                  expanded={expandedFile === path}
                  isCursor={fileCursor === navIdx}
                  onAction={() => handleStageFile(path)}
                  onToggleDiff={() => toggleFileDiff(path)}
                  onOpenZen={() => openZen(path)}
                  onCursor={() => setFileCursor(navIdx)}
                  sessionId={sessionId}
                />
              );
            })}
          </FileSection>
        )}

        {flatNav.length > 0 && (
          <div className="sticky bottom-0 flex items-center gap-1 border-t border-border/40 bg-card/95 px-3 py-1 text-[9px] text-muted-foreground/60">
            <Kbd keys="arrowup" /><Kbd keys="arrowdown" /> · <Kbd keys="j" /><Kbd keys="k" /> move · <Kbd keys="space" /> stage · <Kbd keys="enter" /> open
          </div>
        )}
      </div>

      <div className="shrink-0 border-t border-border/50 p-2">
        <Textarea
          value={commitMsg}
          onChange={(e) => setCommitMsg(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Commit message... (Ctrl+Enter = commit, Ctrl+Shift+Enter = ship)"
          className="mb-2 h-16 resize-none rounded border-border/50 bg-background font-mono text-[11px] leading-relaxed focus-visible:ring-1"
          spellCheck={false}
        />
        <div className="flex flex-wrap items-center gap-1.5">
          <button
            onClick={handleCommit}
            disabled={!canCommit}
            className={`flex h-6 flex-1 items-center justify-center gap-1.5 rounded text-[11px] font-medium transition-colors ${
              canCommit
                ? "bg-primary text-primary-foreground hover:bg-primary/90"
                : "bg-secondary text-muted-foreground cursor-not-allowed"
            }`}
          >
            {committing ? "Committing..." : `Commit (${status.staged.length})`}
            {canCommit && <Kbd keys="ctrl+enter" />}
          </button>
          {canPushOrShip && (
            <button
              onClick={handlePush}
              className="flex h-6 items-center gap-1.5 rounded border border-border/50 px-2 text-[10px] text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
            >
              <Upload size={10} /> Push <Kbd keys="ctrl+alt+p" />
            </button>
          )}
          {canPushOrShip && (
            <button
              onClick={() => handleShip(commitMsg.trim() || null)}
              disabled={creatingPr}
              className="flex h-6 items-center gap-1.5 rounded bg-green-500/15 px-2 text-[10px] font-medium text-green-400 hover:bg-green-500/25 transition-colors"
            >
              <Rocket size={10} /> Ship <Kbd keys="ctrl+shift+y" />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function FileSection({
  title,
  count,
  action,
  children,
}: {
  title: string;
  count: number;
  action?: { label: string; icon: React.ReactNode; onClick: () => void };
  children: React.ReactNode;
}) {
  return (
    <div className="border-b border-border/50">
      <div className="flex items-center justify-between px-3 py-1">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          {title} ({count})
        </span>
        {action && (
          <button
            onClick={action.onClick}
            className="flex items-center gap-0.5 text-[10px] text-muted-foreground hover:text-foreground transition-colors"
          >
            {action.icon}
            {action.label}
          </button>
        )}
      </div>
      {children}
    </div>
  );
}

function FileRow({
  path,
  status,
  actionIcon,
  actionLabel,
  expanded,
  isCursor,
  onAction,
  onToggleDiff,
  onOpenZen,
  onCursor,
  sessionId,
}: {
  path: string;
  status: string;
  actionIcon: React.ReactNode;
  actionLabel: string;
  expanded: boolean;
  isCursor?: boolean;
  onAction: () => void;
  onToggleDiff: () => void;
  onOpenZen: () => void;
  onCursor?: () => void;
  sessionId: string;
}) {
  const filename = path.split("/").pop() ?? path;
  const color = STATUS_COLORS[status] ?? "text-muted-foreground";
  const letter = STATUS_LETTERS[status] ?? "?";

  return (
    <div>
      <div
        ref={(el) => { if (el && isCursor) el.scrollIntoView({ block: "nearest" }); }}
        onMouseEnter={onCursor}
        className={`group flex items-center gap-1 px-3 py-0.5 transition-colors ${
          isCursor ? "bg-orange-500/15 border-l-2 border-l-orange-500" : "hover:bg-secondary/30 border-l-2 border-l-transparent"
        }`}
      >
        <button
          onClick={onToggleDiff}
          className="flex size-3 shrink-0 items-center justify-center text-muted-foreground/50"
        >
          <ChevronRight size={8} className={`transition-transform ${expanded ? "rotate-90" : ""}`} />
        </button>
        <span className={`shrink-0 font-mono text-[10px] font-bold w-3 ${color}`}>{letter}</span>
        <button
          onClick={onOpenZen}
          className="min-w-0 flex-1 truncate text-left text-[11px] text-foreground/80 hover:text-foreground transition-colors"
          title={path}
        >
          {filename}
        </button>
        <button
          onClick={onAction}
          className="flex size-4 shrink-0 items-center justify-center rounded opacity-0 transition-opacity group-hover:opacity-100 hover:bg-secondary text-muted-foreground hover:text-foreground"
          aria-label={actionLabel}
        >
          {actionIcon}
        </button>
      </div>
      {expanded && (
        <div className="mx-2 mb-1 max-h-64 overflow-hidden rounded border border-border/30">
          <DiffView filePath={path} sessionId={sessionId} onOpenZen={onOpenZen} />
        </div>
      )}
    </div>
  );
}
