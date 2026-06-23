import { useEffect, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { activeSessionIdAtom, activeModeAtom, activeCwdAtom, activeAgentStatusAtom } from "@/stores/workspace";
import { activeGitInfoAtom, refreshGitInfoAtom, renameBranchSignalAtom } from "@/stores/git";
import { loadSessionFilesAtom } from "@/stores/files";
import { loadPinnedNotesAtom } from "@/stores/pinnedNotes";
import { activitySummaryAtom, activityDrawerOpenAtom } from "@/stores/activity";
import { activeAgentMetadataAtom } from "@/stores/agent";
import {
  browserNewTabAction,
  browserSetModeAction,
  localhostPortsAtom,
  portsPopoverOpenAtom,
} from "@/stores/browser";
import { openTabAction } from "@/stores/rightPanel";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import { open as openShell } from "@tauri-apps/plugin-shell";
import { Badge } from "@/components/ui/badge";
import { GitBranch, FolderOpen, Zap, ChevronUp, Gauge, Clock, Globe, CalendarRange, Pencil, TriangleAlert, Timer, History, X } from "lucide-react";
import { activeIncidentsAtom } from "@/stores/statusFeed";
import { notificationHistoryAtom, clearNotificationsAtom } from "@/stores/notifications";
import {
  Tooltip,
  TooltipProvider,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

function formatElapsed(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  if (mins >= 60) {
    const hrs = Math.floor(mins / 60);
    return `${hrs}h ${mins % 60}m`;
  }
  return `${mins}m ${secs}s`;
}

function formatDurationFromStart(startedAt: number | null, now: number): string {
  if (startedAt == null) return "--";
  const seconds = Math.max(0, Math.floor(now / 1000) - startedAt);
  return formatElapsed(seconds);
}

function rateLimitColor(pct: number | null): string {
  if (pct == null) return "text-muted-foreground";
  if (pct >= 90) return "text-red-500";
  if (pct >= 70) return "text-yellow-500";
  return "text-muted-foreground";
}

/// Text progress bar for tooltips — the inline bars moved into hover to keep
/// the bar chrome quiet (user request 2026-06-06).
function asciiBar(pct: number, width = 12): string {
  const filled = Math.max(0, Math.min(width, Math.round((pct / 100) * width)));
  return "█".repeat(filled) + "░".repeat(width - filled);
}

function formatTokens(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 100_000 ? 0 : 1)}K`;
  return `${Math.round(n)}`;
}

function modeDotColor(mode: string): string {
  if (mode === "idle") return "bg-muted-foreground";
  if (mode === "active") return "bg-sky-400";
  return "bg-green-500";
}

export function StatusBar() {
  const sessionId = useAtomValue(activeSessionIdAtom);
  const mode = useAtomValue(activeModeAtom);
  const gitInfo = useAtomValue(activeGitInfoAtom);
  const refreshGit = useSetAtom(refreshGitInfoAtom);
  const setRenameBranchSignal = useSetAtom(renameBranchSignalAtom);
  const loadFiles = useSetAtom(loadSessionFilesAtom);
  const loadPinned = useSetAtom(loadPinnedNotesAtom);
  const cwd = useAtomValue(activeCwdAtom);
  const summary = useAtomValue(activitySummaryAtom);
  const setDrawerOpen = useSetAtom(activityDrawerOpenAtom);
  const sl = useAtomValue(activeAgentStatusAtom);
  const agentMeta = useAtomValue(activeAgentMetadataAtom);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (sessionId) {
      refreshGit(sessionId);
      loadFiles(sessionId);
      loadPinned(sessionId);
    }
  }, [sessionId]);

  // Tick once per second so the "duration" cell counts up between agent
  // status pushes. The atom only updates when the agent emits a snapshot,
  // which can be infrequent for non-CC agents.
  useEffect(() => {
    if (sl.session_started_at == null) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [sl.session_started_at]);

  const dotColor = modeDotColor(mode);
  const ctxPct = sl.context_used_pct != null ? Math.round(sl.context_used_pct) : null;

  return (
    // delay={0}: status-bar hovers respond instantly, same as sidebar rows.
    <TooltipProvider delay={0}>
    <footer
      // 1fr/auto/1fr keeps the center cluster viewport-centered while each zone
      // stays confined to its track (no overlap when one side grows).
      className="grid h-7 grid-cols-[1fr_auto_1fr] items-center gap-2 bg-card px-3 text-[11px] leading-none"
      role="status"
    >
      {/* Left: git info + cwd + mode */}
      <div className="flex min-w-0 items-center gap-2 text-muted-foreground">
        {agentMeta && (
          <Tooltip>
            <TooltipTrigger className="cursor-default">
              <Badge variant="outline" className="h-4 px-1.5 text-[9px] font-medium uppercase tracking-wider">
                {agentMeta.display_name}
              </Badge>
            </TooltipTrigger>
            <TooltipContent>Active agent: {agentMeta.id}</TooltipContent>
          </Tooltip>
        )}
        {gitInfo && (
          <div className="group flex min-w-0 items-center gap-1">
            <GitBranch className="size-3 shrink-0" />
            <Tooltip>
              <TooltipTrigger className="cursor-default max-w-40 truncate">
                {gitInfo.branch}
              </TooltipTrigger>
              <TooltipContent>{gitInfo.branch}</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger
                render={
                  <button
                    type="button"
                    aria-label="Rename branch"
                    onClick={() => setRenameBranchSignal((p) => p + 1)}
                    className="hidden size-4 shrink-0 items-center justify-center rounded text-muted-foreground/70 hover:bg-secondary hover:text-foreground transition-colors group-hover:flex"
                  />
                }
              >
                <Pencil className="size-2.5" />
              </TooltipTrigger>
              <TooltipContent side="top" className="text-[10px]">Rename branch (Ctrl+Alt+R)</TooltipContent>
            </Tooltip>
            {gitInfo.dirty && (
              <span className="inline-block size-1.5 shrink-0 rounded-full bg-orange-500" aria-label="Uncommitted changes" />
            )}
            {gitInfo.ahead > 0 && (
              <span className="text-muted-foreground/70">+{gitInfo.ahead}</span>
            )}
          </div>
        )}
        {cwd && (
          <div className="flex min-w-0 items-center gap-1">
            <FolderOpen className="size-3 shrink-0" />
            <Tooltip>
              <TooltipTrigger className="cursor-default max-w-32 truncate">
                {cwd.split("/").pop() ?? cwd}
              </TooltipTrigger>
              <TooltipContent>{cwd}</TooltipContent>
            </Tooltip>
          </div>
        )}
        <Tooltip>
          <TooltipTrigger>
            <Badge
              variant="secondary"
              className="h-4 gap-1 px-1.5 text-[11px] leading-none"
            >
              <span
                className={`inline-block size-1.5 shrink-0 rounded-full ${dotColor}`}
                aria-hidden="true"
              />
              {mode}
            </Badge>
          </TooltipTrigger>
          <TooltipContent>Session mode: {mode}</TooltipContent>
        </Tooltip>
      </div>

      {/* Center: activity summary + localhost ports */}
      <div className="flex min-w-0 items-center justify-center gap-3">
        <Tooltip>
          <TooltipTrigger
            onClick={() => setDrawerOpen((prev) => !prev)}
            className="flex min-w-0 items-center gap-1.5 rounded px-2 py-0.5 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            {summary.lastAction ? (
              <>
                <Zap className="size-3 shrink-0 text-primary" />
                <span className="w-44 shrink-0 truncate">{summary.lastAction}</span>
                <span className="shrink-0 text-muted-foreground/60">│</span>
                <span className="shrink-0 whitespace-nowrap tabular-nums">
                  <span className="inline-block min-w-[1.25rem] text-right">{summary.actionCount}</span> actions
                </span>
                <span className="shrink-0 text-muted-foreground/60">│</span>
                <span className="inline-block min-w-[3.25rem] shrink-0 text-right tabular-nums">{formatElapsed(summary.elapsedSeconds)}</span>
              </>
            ) : (
              <span className="shrink-0">No activity</span>
            )}
            <ChevronUp className="ml-1 size-3 shrink-0" />
          </TooltipTrigger>
          <TooltipContent>Click to toggle the activity drawer</TooltipContent>
        </Tooltip>

        <LocalhostPortChips />
        <IncidentChips />
        <NotificationHistory />
      </div>

      {/* Right: context %, rate limits, model, duration. Progress bars live
          in the tooltips (ASCII) — inline only the colored percentages. */}
      <div className="flex min-w-0 items-center justify-end gap-2.5 text-muted-foreground">
        {ctxPct != null && (
          <Tooltip>
            <TooltipTrigger className="flex cursor-default items-center gap-1">
              <Gauge className="size-3 shrink-0" />
              <span className={`inline-block min-w-[2.25rem] text-right tabular-nums ${rateLimitColor(ctxPct)}`}>{ctxPct}%</span>
            </TooltipTrigger>
            <TooltipContent className="flex-col items-start gap-0.5">
              <span className="font-mono text-[10px]">{asciiBar(ctxPct)} {ctxPct}%</span>
              <span>
                Context window
                {sl.context_window_size != null && (
                  <>
                    : {formatTokens((sl.context_window_size * ctxPct) / 100)} /{" "}
                    {formatTokens(sl.context_window_size)} tokens
                  </>
                )}
              </span>
            </TooltipContent>
          </Tooltip>
        )}

        {sl.rate_5h_pct != null && (
          <Tooltip>
            <TooltipTrigger className="flex cursor-default items-center gap-1">
              <Timer className="size-3 shrink-0" />
              <span className={`inline-block min-w-[2.25rem] text-right tabular-nums ${rateLimitColor(sl.rate_5h_pct)}`}>{Math.round(sl.rate_5h_pct)}%</span>
              {sl.rate_5h_resets_at && (
                <span className="tabular-nums text-muted-foreground/60">{new Date(sl.rate_5h_resets_at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false })}</span>
              )}
            </TooltipTrigger>
            <TooltipContent className="flex-col items-start gap-0.5">
              <span className="font-mono text-[10px]">{asciiBar(sl.rate_5h_pct)} {sl.rate_5h_pct.toFixed(1)}%</span>
              <span>
                5-hour rate limit
                {sl.rate_5h_resets_at && (
                  <> — resets {new Date(sl.rate_5h_resets_at * 1000).toLocaleTimeString([], { hour12: false })}</>
                )}
              </span>
            </TooltipContent>
          </Tooltip>
        )}

        {sl.model_name && (
          <Tooltip>
            <TooltipTrigger className="cursor-default font-medium text-foreground">
              {sl.model_name}
            </TooltipTrigger>
            <TooltipContent>{sl.model_id ?? sl.model_name}</TooltipContent>
          </Tooltip>
        )}

        {sl.effort_level && (
          <Tooltip>
            <TooltipTrigger className="cursor-default text-muted-foreground/80">
              {sl.effort_level}
            </TooltipTrigger>
            <TooltipContent>Effort level</TooltipContent>
          </Tooltip>
        )}

        {sl.session_started_at != null && (
          <Tooltip>
            <TooltipTrigger className="flex cursor-default items-center gap-0.5">
              <Clock className="size-3 shrink-0" />
              <span className="inline-block min-w-[3rem] text-right tabular-nums">
                {formatDurationFromStart(sl.session_started_at, now)}
              </span>
            </TooltipTrigger>
            <TooltipContent>Session duration</TooltipContent>
          </Tooltip>
        )}

        {sl.rate_7d_pct != null && (
          <Tooltip>
            <TooltipTrigger className="flex cursor-default items-center gap-1">
              <CalendarRange className="size-3 shrink-0" />
              <span className={`inline-block min-w-[2.25rem] text-right tabular-nums ${rateLimitColor(sl.rate_7d_pct)}`}>{Math.round(sl.rate_7d_pct)}%</span>
            </TooltipTrigger>
            <TooltipContent className="flex-col items-start gap-0.5">
              <span className="font-mono text-[10px]">{asciiBar(sl.rate_7d_pct)} {sl.rate_7d_pct.toFixed(1)}%</span>
              <span>
                Weekly rate limit
                {sl.rate_7d_resets_at && (
                  <> — resets {new Date(sl.rate_7d_resets_at * 1000).toLocaleString([], { hour12: false })}</>
                )}
              </span>
            </TooltipContent>
          </Tooltip>
        )}
      </div>
    </footer>
    </TooltipProvider>
  );
}

function formatRelativeTime(ts: number, now: number): string {
  const s = Math.max(0, Math.floor((now - ts) / 1000));
  if (s < 10) return "now";
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  return new Date(ts).toLocaleDateString([], { month: "short", day: "numeric" });
}

/// History chip + upward popover: a passive log of every toast, consulted on
/// demand. No unread tracking — toasts are ephemeral status messages.
function NotificationHistory() {
  const history = useAtomValue(notificationHistoryAtom);
  const clearAll = useSetAtom(clearNotificationsAtom);
  const [open, setOpen] = useState(false);

  const now = Date.now();

  return (
    <div className="relative flex items-center">
      <Tooltip>
        <TooltipTrigger
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          className={`relative flex items-center gap-1 rounded px-1.5 py-0 text-[10px] transition-colors text-muted-foreground hover:bg-secondary hover:text-foreground ${open ? "bg-secondary text-foreground" : ""}`}
        >
          <History className="size-3 shrink-0" />
        </TooltipTrigger>
        <TooltipContent>Notification history</TooltipContent>
      </Tooltip>
      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div className="absolute bottom-full right-0 z-50 mb-1.5 max-h-72 w-72 overflow-y-auto rounded-md border border-border bg-card shadow-lg">
            <div className="sticky top-0 flex items-center justify-between border-b border-border/50 bg-card px-2.5 py-1.5">
              <span className="text-[10px] font-medium text-foreground">Notification history</span>
              {history.length > 0 && (
                <button
                  onClick={() => clearAll()}
                  className="text-[9px] text-muted-foreground transition-colors hover:text-foreground"
                >
                  Clear all
                </button>
              )}
            </div>
            {history.length === 0 ? (
              <div className="px-2.5 py-5 text-center text-[10px] text-muted-foreground/60">
                No notifications yet
              </div>
            ) : (
              history.map((n) => (
                <div
                  key={n.id}
                  className="flex items-start gap-2 border-b border-border/30 px-2.5 py-1.5 last:border-b-0"
                >
                  <span
                    className={`mt-1 inline-block size-1.5 shrink-0 rounded-full ${
                      n.type === "error"
                        ? "bg-red-500"
                        : n.type === "success"
                          ? "bg-green-500"
                          : "bg-sky-400"
                    }`}
                    aria-hidden="true"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-2">
                      <span className="truncate text-[10px] font-medium text-foreground">{n.message}</span>
                      <span className="shrink-0 text-[9px] tabular-nums text-muted-foreground/50">
                        {formatRelativeTime(n.ts, now)}
                      </span>
                    </div>
                    {n.description && (
                      <p className="mt-0.5 line-clamp-2 text-[9px] text-muted-foreground">{n.description}</p>
                    )}
                  </div>
                </div>
              ))
            )}
          </div>
        </>
      )}
    </div>
  );
}

/// One chip per provider with an active incident (status.claude.com /
/// status.openai.com, polled by the Rust feed). Hidden while everything is
/// operational. Status pages set frame-ancestors/CSP that the in-app iframe
/// can't satisfy (renders a black box), so the chip opens them externally.
function IncidentChips() {
  const incidents = useAtomValue(activeIncidentsAtom);

  if (incidents.length === 0) return null;

  return (
    <div className="flex shrink-0 items-center gap-1">
      {incidents.map((s) => (
        <Tooltip key={s.provider}>
          <TooltipTrigger
            onClick={() => void openShell(s.url).catch(() => {})}
            className={`flex h-5 shrink-0 items-center gap-1 rounded px-1.5 text-[10px] font-medium leading-none whitespace-nowrap transition-colors ${
              s.indicator === "minor"
                ? "bg-yellow-500/15 text-yellow-400 hover:bg-yellow-500/25"
                : "bg-red-500/15 text-red-400 hover:bg-red-500/25"
            }`}
          >
            <TriangleAlert className="size-3 shrink-0" />
            {s.provider === "claude" ? "Claude" : "OpenAI"}
          </TooltipTrigger>
          <TooltipContent>
            {s.description} — click to open {s.url}
          </TooltipContent>
        </Tooltip>
      ))}
    </div>
  );
}

/// Single "ports" chip for the dev servers detected by the Rust scanner.
/// Dimmed and inert while no ports are listening; with ports it opens an
/// upward popover listing them — click a port to open it in the browser
/// panel. Replaces the old horizontal scroll strip (one chip per port),
/// which pushed status icons around on busy projects.
function LocalhostPortChips() {
  const ports = useAtomValue(localhostPortsAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const newTab = useSetAtom(browserNewTabAction);
  const setMode = useSetAtom(browserSetModeAction);
  const openTab = useSetAtom(openTabAction);
  const [open, setOpen] = useAtom(portsPopoverOpenAtom);
  const popoverRef = useRef<HTMLDivElement | null>(null);
  const [procInfo, setProcInfo] = useState<
    Record<number, { label: string; project: string | null; kind: string; pid: number | null } | null>
  >({});
  const pushToast = useSetAtom(toastsAtom);
  const hasPorts = ports.length > 0;

  // The popover must not outlive its content (last dev server dies while open).
  useEffect(() => {
    if (!hasPorts) setOpen(false);
  }, [hasPorts, setOpen]);

  // Opened (often via the global shortcut) → focus the first row so the list is
  // keyboard-navigable (Tab between ports, Enter to open, the kill button is
  // revealed on focus). rAF lets the popover mount first.
  useEffect(() => {
    if (!open) return;
    const id = requestAnimationFrame(() => {
      popoverRef.current?.querySelector<HTMLElement>("button")?.focus({ preventScroll: true });
    });
    return () => cancelAnimationFrame(id);
  }, [open]);

  // Resolve the owning process per port when the popover opens — a cheap /proc
  // walk, on-demand only so the 3s scanner stays lightweight.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    Promise.all(
      ports.map((port) =>
        invoke<{ label: string; project: string | null; kind: string; pid: number | null } | null>(
          "port_process_info",
          { port },
        )
          .then((info) => [port, info] as const)
          .catch(() => [port, null] as const),
      ),
    ).then((entries) => {
      if (!cancelled) setProcInfo(Object.fromEntries(entries));
    });
    return () => {
      cancelled = true;
    };
  }, [open, ports]);

  async function openPort(port: number) {
    setOpen(false);
    if (!sessionId) return;
    setMode({ sessionId, mode: "dock" });
    openTab({
      tab: { id: `browser:${sessionId}`, type: "browser", label: "Browser" },
    });
    try {
      await newTab({ sessionId, url: `http://localhost:${port}` });
    } catch {
      /* validate_url fails on a sane localhost URL only if the user fed us
         garbage — ignore here. */
    }
  }

  async function killPort(port: number) {
    try {
      await invoke("kill_port", { port });
      pushToast({ message: "Port freed", description: `Sent SIGTERM to the process on :${port}`, type: "success" });
    } catch (e) {
      pushToast({ message: "Couldn't free port", description: String(e), type: "error" });
    }
  }

  return (
    <div className="relative flex items-center">
      <Tooltip>
        <TooltipTrigger
          onClick={() => hasPorts && setOpen((v) => !v)}
          disabled={!hasPorts}
          aria-expanded={open}
          className={`flex items-center gap-1 rounded px-1.5 py-0 text-[10px] transition-colors ${
            hasPorts
              ? "text-muted-foreground hover:bg-secondary hover:text-foreground"
              : "text-muted-foreground/40 cursor-default"
          } ${open ? "bg-secondary text-foreground" : ""}`}
        >
          <Globe className="size-3 shrink-0" />
          ports
          {hasPorts && (
            <span className="flex h-3.5 items-center justify-center rounded-full bg-primary/15 px-1 text-[9px] font-medium tabular-nums text-primary">
              {ports.length}
            </span>
          )}
        </TooltipTrigger>
        <TooltipContent>
          {hasPorts ? "Listening dev servers — click to list" : "No dev servers detected"}
        </TooltipContent>
      </Tooltip>
      {open && (
        <>
          {/* Outside-click catcher — same pattern as the TopBar/TabBar dropdowns. */}
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div
            ref={popoverRef}
            onKeyDown={(e) => { if (e.key === "Escape") setOpen(false); }}
            className="absolute bottom-full left-1/2 z-50 mb-1.5 max-h-56 w-64 -translate-x-1/2 overflow-y-auto rounded-md border border-border bg-card py-1 shadow-lg"
          >
            {ports.map((port) => {
              const info = procInfo[port];
              return (
                <div key={port} className="group flex items-center gap-1 px-2 py-1 text-[10px] transition-colors hover:bg-secondary">
                  <button
                    onClick={() => openPort(port)}
                    disabled={!sessionId}
                    className="flex min-w-0 flex-1 items-center gap-2 text-left font-mono text-muted-foreground transition-colors group-hover:text-foreground disabled:opacity-40"
                  >
                    <span className="inline-block size-1.5 shrink-0 rounded-full bg-green-500" aria-hidden="true" />
                    <span className="shrink-0">localhost:{port}</span>
                    {info && (
                      <span className="min-w-0 truncate text-muted-foreground/45">
                        {info.label}
                        {info.project ? ` · ${info.project}` : ""}
                        {info.kind === "docker" ? " · docker" : ""}
                      </span>
                    )}
                  </button>
                  {info?.pid != null && (
                    <Tooltip>
                      <TooltipTrigger
                        onClick={() => killPort(port)}
                        className="flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground/40 opacity-0 transition-all hover:bg-red-500/15 hover:text-red-400 focus:opacity-100 group-hover:opacity-100 group-focus-within:opacity-100"
                      >
                        <X className="size-3" />
                      </TooltipTrigger>
                      <TooltipContent>Kill the process on this port (SIGTERM)</TooltipContent>
                    </Tooltip>
                  )}
                </div>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
