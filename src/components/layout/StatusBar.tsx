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
import { openTabAction, expandRightPanelAtom } from "@/stores/rightPanel";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import { open as openShell } from "@tauri-apps/plugin-shell";
import { confirm as swalConfirm } from "@/lib/confirm";
import { focusZoneAtom } from "@/stores/shortcuts";
import * as terminalService from "@/components/terminal/terminalService";
import { Badge } from "@/components/ui/badge";
import { GitBranch, FolderOpen, Zap, ChevronUp, Gauge, Clock, Globe, CalendarRange, Pencil, TriangleAlert, Timer, History, X, Copy } from "lucide-react";
import { activeIncidentsAtom } from "@/stores/statusFeed";
import { notificationHistoryAtom, clearNotificationsAtom, notificationHistoryOpenAtom, type NotificationEntry } from "@/stores/notifications";
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

  // Status popovers (ports / notifications) are transient: opening the
  // activities drawer or expanding the right panel dismisses whichever is open.
  // The sibling case + click-away are handled inside each popover.
  const setPortsOpen = useSetAtom(portsPopoverOpenAtom);
  const setNotifOpen = useSetAtom(notificationHistoryOpenAtom);
  const activityDrawerOpen = useAtomValue(activityDrawerOpenAtom);
  const rightPanelExpand = useAtomValue(expandRightPanelAtom);
  useEffect(() => {
    setPortsOpen(false);
    setNotifOpen(false);
  }, [activityDrawerOpen, rightPanelExpand, setPortsOpen, setNotifOpen]);

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
                {/* Content-sized cap: short tool names like "Bash" sit flush
                    against the icon; long MCP names truncate (full name on hover)
                    so the model segment on the right never gets squeezed to wrap. */}
                <span className="min-w-0 max-w-28 truncate">{summary.lastAction}</span>
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
          <TooltipContent>{summary.lastAction ?? "Click to toggle the activity drawer"}</TooltipContent>
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
            <TooltipTrigger className="cursor-default whitespace-nowrap font-medium text-foreground">
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
  const setToasts = useSetAtom(toastsAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const setPortsOpen = useSetAtom(portsPopoverOpenAtom);
  const [open, setOpen] = useAtom(notificationHistoryOpenAtom);
  const [activeIdx, setActiveIdx] = useState(0);
  const popoverRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const now = Date.now();

  function closeAndFocusTerminal() {
    setOpen(false);
    setFocusZone("terminal");
    terminalService.focusActive();
  }

  function copyEntry(n: NotificationEntry) {
    // Single line "message → description" so it reads like the entry.
    const text = n.description ? `${n.message} → ${n.description}` : n.message;
    // terminal_clipboard_write is the Wayland-safe spawn_blocking writer used by
    // the ClickUp/Linear copy actions; the plugin's async write_text stalls.
    // skipHistory: confirm the copy without logging "Copied" into this very
    // popover (every toast otherwise mirrors into the history — toast.ts).
    void invoke("terminal_clipboard_write", { text })
      .then(() =>
        setToasts({ message: "Copied to clipboard", description: text, type: "success", skipHistory: true }),
      )
      .catch(() => setToasts({ message: "Copy failed", type: "error", skipHistory: true }));
  }

  useEffect(() => {
    if (open) setActiveIdx(0);
  }, [open]);

  // Notifications and ports are mutually exclusive — opening this closes the
  // sibling (covers the shortcut path; clicks are handled by the dismiss-on-
  // outside listener below).
  useEffect(() => {
    if (open) setPortsOpen(false);
  }, [open, setPortsOpen]);

  // Dismiss on any pointerdown outside the popover (incl. its trigger). Non-
  // consuming, so the same click can open the sibling popover or focus a panel.
  useEffect(() => {
    if (!open) return;
    function onDown(e: PointerEvent) {
      if (!containerRef.current?.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("pointerdown", onDown, true);
    return () => document.removeEventListener("pointerdown", onDown, true);
  }, [open, setOpen]);

  // Keep the keyboard cursor in view as it moves past the scroll fold (block:
  // "nearest" is a no-op for already-visible rows, so mouse-hover doesn't jump).
  useEffect(() => {
    if (!open) return;
    popoverRef.current
      ?.querySelector('[data-nav-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [activeIdx, open]);

  // Window-capture keyboard nav while open (mirrors the ports popover): ↑↓ move,
  // Enter/Space/C copy the selected entry, Shift+Backspace clears all + returns
  // focus to the terminal, Esc closes. stopImmediatePropagation keeps every
  // handled key from leaking to global shortcuts or the terminal.
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        closeAndFocusTerminal();
        return;
      }
      if (history.length === 0) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p + 1) % history.length);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p - 1 + history.length) % history.length);
      } else if (
        e.key === "Enter" ||
        e.key === " " ||
        ((e.key === "c" || e.key === "C") && !e.ctrlKey && !e.metaKey && !e.altKey)
      ) {
        e.preventDefault();
        e.stopImmediatePropagation();
        const n = history[activeIdx];
        if (n) copyEntry(n);
      } else if (e.key === "Backspace" && e.shiftKey) {
        e.preventDefault();
        e.stopImmediatePropagation();
        clearAll();
        closeAndFocusTerminal();
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, history, activeIdx]);

  // Clamp the highlight if the list shrank under the cursor (e.g. after clear).
  const safeIdx = Math.min(activeIdx, Math.max(0, history.length - 1));

  return (
    <div ref={containerRef} className="relative flex items-center">
      <Tooltip>
        <TooltipTrigger
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          className={`relative flex items-center gap-1 rounded px-1.5 py-0 text-[10px] transition-colors text-muted-foreground hover:bg-secondary hover:text-foreground ${open ? "bg-secondary text-foreground" : ""}`}
        >
          <History className="size-3 shrink-0" />
        </TooltipTrigger>
        <TooltipContent>Notification history (Ctrl+Alt+N)</TooltipContent>
      </Tooltip>
      {open && (
        <>
          <div
            ref={popoverRef}
            className="absolute bottom-full right-0 z-50 mb-1.5 max-h-72 w-72 overflow-y-auto rounded-md border border-border bg-card shadow-lg"
          >
            <div className="sticky top-0 flex items-center justify-between border-b border-border/50 bg-card px-2.5 py-1.5">
              <span className="text-[10px] font-medium text-foreground">Notification history</span>
              {history.length > 0 && (
                <button
                  onClick={() => {
                    clearAll();
                    closeAndFocusTerminal();
                  }}
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
              <>
                <div className="border-b border-border/40 px-2.5 py-1 text-[9px] text-muted-foreground/50">
                  ↑↓ move · Enter/C copy · ⇧⌫ clear all
                </div>
                {history.map((n, idx) => {
                  const selected = idx === safeIdx;
                  return (
                    <div
                      key={n.id}
                      data-nav-selected={selected ? "true" : undefined}
                      onMouseEnter={() => setActiveIdx(idx)}
                      // scroll-mt clears the sticky header so an item scrolled to
                      // the top (e.g. wrapping last→first) isn't hidden behind it.
                      className={`group/notif flex scroll-mt-9 items-start gap-2 border-b border-border/30 px-2.5 py-1.5 last:border-b-0 ${selected ? "bg-secondary" : ""}`}
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
                      <Tooltip>
                        <TooltipTrigger
                          onClick={() => copyEntry(n)}
                          className={`mt-0.5 flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground/40 transition-all hover:bg-secondary hover:text-foreground ${selected ? "opacity-100" : "opacity-0 group-hover/notif:opacity-100"}`}
                        >
                          <Copy className="size-3" />
                        </TooltipTrigger>
                        <TooltipContent>Copy</TooltipContent>
                      </Tooltip>
                    </div>
                  );
                })}
              </>
            )}
          </div>
        </>
      )}
    </div>
  );
}

/// One chip per provider with an active incident (status.claude.com /
/// status.openai.com, polled by the Rust feed). Hidden while everything is
/// operational. Click opens the provider's status page in the in-app browser
/// panel.
function IncidentChips() {
  const incidents = useAtomValue(activeIncidentsAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const newTab = useSetAtom(browserNewTabAction);
  const setMode = useSetAtom(browserSetModeAction);
  const openTab = useSetAtom(openTabAction);
  const expandPanel = useSetAtom(expandRightPanelAtom);

  if (incidents.length === 0) return null;

  async function openStatusPage(url: string, provider: string) {
    // status.openai.com sets frame-ancestors/CSP that the in-app iframe can't
    // satisfy (renders a black box), so OpenAI opens externally; status.claude.com
    // frames fine and stays in the panel.
    if (provider !== "claude") {
      void openShell(url).catch(() => {});
      return;
    }
    if (!sessionId) return;
    setMode({ sessionId, mode: "dock" });
    openTab({ tab: { id: `browser:${sessionId}`, type: "browser", label: "Browser" } });
    // Re-clicking after the right panel was hidden must re-open it — openTab only
    // (re)activates the tab; expanding the collapsed panel needs this signal.
    expandPanel((n) => n + 1);
    try {
      await newTab({ sessionId, url });
    } catch {
      /* status page URLs are hardcoded https — validate_url can't reject them */
    }
  }

  return (
    <div className="flex shrink-0 items-center gap-1">
      {incidents.map((s) => (
        <Tooltip key={s.provider}>
          <TooltipTrigger
            onClick={() => void openStatusPage(s.url, s.provider)}
            disabled={s.provider === "claude" && !sessionId}
            className={`flex h-5 shrink-0 items-center gap-1 rounded px-1.5 text-[10px] font-medium leading-none whitespace-nowrap transition-colors disabled:opacity-40 ${
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
  const setNotifOpen = useSetAtom(notificationHistoryOpenAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const popoverRef = useRef<HTMLDivElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [activeIdx, setActiveIdx] = useState(0);
  const [procInfo, setProcInfo] = useState<
    Record<number, { label: string; project: string | null; kind: string; pid: number | null } | null>
  >({});
  const pushToast = useSetAtom(toastsAtom);
  const hasPorts = ports.length > 0;

  // The popover must not outlive its content (last dev server dies while open).
  useEffect(() => {
    if (!hasPorts) setOpen(false);
  }, [hasPorts, setOpen]);

  function closeAndFocusTerminal() {
    setOpen(false);
    setFocusZone("terminal");
    terminalService.focusActive();
  }

  // Opened → reset the cursor to the first port so up/down nav has an anchor.
  useEffect(() => {
    if (open) setActiveIdx(0);
  }, [open]);

  // Ports and notifications are mutually exclusive — opening this closes the
  // sibling (covers the shortcut path; clicks are handled by the dismiss-on-
  // outside listener below).
  useEffect(() => {
    if (open) setNotifOpen(false);
  }, [open, setNotifOpen]);

  // Dismiss on any pointerdown outside the popover (incl. its trigger). Non-
  // consuming, so the same click can open the sibling popover or focus a panel.
  useEffect(() => {
    if (!open) return;
    function onDown(e: PointerEvent) {
      if (!containerRef.current?.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("pointerdown", onDown, true);
    return () => document.removeEventListener("pointerdown", onDown, true);
  }, [open, setOpen]);

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

  // Killing a port is reversible (it just restarts) but still a deliberate
  // destructive-ish act — confirm with the project `confirm()` first.
  async function confirmKillPort(port: number) {
    const info = procInfo[port];
    const isDocker = info?.kind === "docker";
    if (!info || (info.pid == null && !isDocker)) {
      pushToast({
        message: "Can't free this port",
        description: "No owning process resolved (it may be a system/daemon socket).",
        type: "info",
      });
      return;
    }
    // Close the popover BEFORE the confirm so its window-capture keydown
    // listener detaches — otherwise Enter/Space on the confirm is swallowed by
    // the ports handler and re-fires the confirm.
    setOpen(false);
    const ok = await swalConfirm({
      title: `Free port :${port}?`,
      body: isDocker
        ? `Stops the Docker container ${info.label}.`
        : `Sends SIGTERM to ${info.label} (pid ${info.pid}).`,
      confirmLabel: isDocker ? "Stop container" : "Kill",
      destructive: true,
    });
    if (ok) await killPort(port);
    setFocusZone("terminal");
    terminalService.focusActive();
  }

  // Keyboard nav (window-capture so it works without focusing a row, mirroring
  // the ClickUp StatusPicker): up/down move the cursor, Enter/Space free the
  // port (confirm-gated), Escape closes and returns focus to the terminal.
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (ports.length === 0) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p + 1) % ports.length);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p - 1 + ports.length) % ports.length);
      } else if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        e.stopImmediatePropagation();
        const port = ports[activeIdx];
        if (port != null) void confirmKillPort(port);
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        closeAndFocusTerminal();
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, ports, activeIdx]);

  // Scroll the keyboard cursor into view as it moves past the fold (mirrors the
  // notifications popover). scroll-mt on rows + a sticky hints header keep the
  // top hints visible when wrapping last→first.
  useEffect(() => {
    if (!open) return;
    popoverRef.current
      ?.querySelector('[data-nav-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [activeIdx, open]);

  return (
    <div ref={containerRef} className="relative flex items-center">
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
          <div
            ref={popoverRef}
            className="absolute bottom-full left-1/2 z-50 mb-1.5 max-h-56 w-64 -translate-x-1/2 overflow-y-auto rounded-md border border-border bg-card py-1 shadow-lg"
          >
            <div className="sticky top-0 z-10 border-b border-border/40 bg-card px-2 pb-1 pt-0.5 text-[9px] text-muted-foreground/50">
              ↑↓ move · Enter free port · click opens
            </div>
            {ports.map((port, idx) => {
              const info = procInfo[port];
              const selected = idx === activeIdx;
              return (
                <div
                  key={port}
                  data-nav-selected={selected ? "true" : undefined}
                  onMouseEnter={() => setActiveIdx(idx)}
                  className={`flex scroll-mt-7 items-center gap-1 px-2 py-1 text-[10px] transition-colors ${selected ? "bg-secondary" : ""}`}
                >
                  <button
                    onClick={() => openPort(port)}
                    disabled={!sessionId}
                    className={`flex min-w-0 flex-1 items-center gap-2 text-left font-mono transition-colors disabled:opacity-40 ${selected ? "text-foreground" : "text-muted-foreground"}`}
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
                  {(info?.pid != null || info?.kind === "docker") && (
                    <Tooltip>
                      <TooltipTrigger
                        onClick={() => void confirmKillPort(port)}
                        className={`flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground/40 transition-all hover:bg-red-500/15 hover:text-red-400 ${selected ? "opacity-100" : "opacity-0"}`}
                      >
                        <X className="size-3" />
                      </TooltipTrigger>
                      <TooltipContent>
                        {info?.kind === "docker" ? "Stop this container" : "Free this port (SIGTERM)"}
                      </TooltipContent>
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
