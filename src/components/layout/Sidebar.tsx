import { useState, useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { configAtom } from "@/stores/config";
import { useFocusPulse } from "@/hooks/useFocusPulse";
import { confirm as swalConfirm } from "@/lib/swal";
import { Pencil, Trash2 } from "lucide-react";
import { TextInputDialog } from "@/components/ui/TextInputDialog";
import { deleteSessionWithGraceAction, deleteWorkspaceWithGraceAction } from "@/stores/pendingDeletes";
import { focusZoneAtom, previousNonTerminalZoneAtom, triggerResumeSessionAtom, triggerNewSessionAtom, triggerAddWorkspaceAtom, triggerMergeAtom, triggerJumpToProjectAtom, sidebarSelectedIdxAtom, focusedWorkspaceIdAtom } from "@/stores/shortcuts";
import {
  workspacesAtom,
  activeSessionIdAtom,
  sessionLaunchModeAtom,
  freshSessionsAtom,
  activeSessionAtom,
  expandedWorkspaceIdsAtom,
  type Workspace,
  type Session,
  type LaunchOptions,
  type EnvShellDef,
} from "@/stores/workspace";
import { spawnEnvShells } from "@/stores/quake";
import { openTabAction } from "@/stores/rightPanel";
import { appStore } from "@/stores/jotaiStore";
import { toastsAtom } from "@/stores/toast";
import { bootstrapPromptAtom } from "@/stores/obsidian";
import { SessionRow } from "@/components/session/SessionRow";
import { SessionIndicator } from "@/components/session/SessionIndicator";
import { ProjectPickerModal } from "@/components/session/ProjectPickerModal";
import { AgentPickerModal } from "@/components/session/AgentPickerModal";
import type { AvailableAgent } from "@/lib/types";
import { invoke } from "@/lib/tauri";
import { open } from "@tauri-apps/plugin-dialog";
import * as terminalService from "@/components/terminal/terminalService";
import { TasksIsland } from "@/components/tasks/TasksIsland";
import { NergalLogo, NergalN } from "@/components/layout/NergalLogo";
import {
  Tooltip,
  TooltipProvider,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

interface SidebarProps {
  collapsed: boolean;
}

export function Sidebar({ collapsed }: SidebarProps) {
  const setFocusZone = useSetAtom(focusZoneAtom);
  const setPreviousZone = useSetAtom(previousNonTerminalZoneAtom);
  const sidebarSelectedIdx = useAtomValue(sidebarSelectedIdxAtom);
  const setSidebarSelectedIdx = useSetAtom(sidebarSelectedIdxAtom);
  const focusZone = useAtomValue(focusZoneAtom);
  const isActive = focusZone === "sidebar";
  const config = useAtomValue(configAtom);
  const isPulsing = useFocusPulse(isActive);
  const showAccent = config.panel_focus_pulse ? isPulsing : isActive;
  const borderClass = showAccent ? "border-primary" : "border-border";
  const dotGridClass = config.sidebar_dot_grid ? "cluihud-dot-grid" : "";

  function handleSidebarFocus() {
    setFocusZone("sidebar");
    setPreviousZone("sidebar");
    if (sidebarSelectedIdx < 0) setSidebarSelectedIdx(0);
  }

  // Hover-driven shortcuts need a global listener: the user keeps focus in
  // the terminal while hovering a session row, so an onKeyDown bound to the
  // sidebar zone never fires.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      if (e.key !== "d" && e.key !== "D" && e.key !== "r" && e.key !== "R") return;
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)) return;
      const mouseHovered = document.querySelectorAll<HTMLElement>(
        "[data-focus-zone='sidebar'] [data-nav-item]:hover",
      );
      // The selected-row fallback is only valid while the sidebar is the
      // active zone — `data-nav-selected` persists after the user moves on,
      // and a stale match here would steal "d"/"r" from other panels (a
      // stash drop used to open the delete-session modal, BUG-03 v0.2.0).
      const row =
        mouseHovered[mouseHovered.length - 1] ??
        (appStore.get(focusZoneAtom) === "sidebar"
          ? document.querySelector<HTMLElement>(
              "[data-focus-zone='sidebar'] [data-nav-item][data-nav-selected='true']",
            )
          : null);
      if (!row) return;
      const isDelete = e.key === "d" || e.key === "D";
      const action = isDelete
        ? row.querySelector<HTMLElement>('[aria-label="Delete"]') ??
          row.querySelector<HTMLElement>('[aria-label="Remove workspace"]')
        : row.querySelector<HTMLElement>('[aria-label="Rename"]');
      if (!action) return;
      e.preventDefault();
      e.stopPropagation();
      action.click();
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  function updateSidebarSelection(container: HTMLElement, idx: number) {
    const items = Array.from(container.querySelectorAll<HTMLElement>("[data-nav-item]"));
    for (const item of items) item.removeAttribute("data-nav-selected");
    if (items[idx]) {
      items[idx].setAttribute("data-nav-selected", "true");
      items[idx].scrollIntoView({ block: "nearest" });
    }
    setSidebarSelectedIdx(idx);
  }

  function handleSidebarKeyDown(e: React.KeyboardEvent) {
    if (e.ctrlKey || e.altKey || e.shiftKey) return;
    const target = e.target as HTMLElement | null;
    if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
    // Radix portals out of the DOM tree but React events still bubble through
    // the React tree to this handler; without this guard, Enter inside a modal
    // would click the hovered sidebar row (BUG-16).
    if (target?.closest('[role="dialog"]')) return;

    const container = e.currentTarget as HTMLElement;

    const items = Array.from(container.querySelectorAll<HTMLElement>("[data-nav-item]"));
    if (items.length === 0) return;

    const idx = sidebarSelectedIdx;
    const selectedItem = idx >= 0 && idx < items.length ? items[idx] : null;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      updateSidebarSelection(container, idx === -1 ? 0 : Math.min(idx + 1, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      updateSidebarSelection(container, idx === -1 ? items.length - 1 : Math.max(idx - 1, 0));
    } else if (e.key === "ArrowRight" && selectedItem?.dataset.navExpanded === "false") {
      e.preventDefault();
      selectedItem.click();
    } else if (e.key === "ArrowLeft" && selectedItem?.dataset.navExpanded === "true") {
      e.preventDefault();
      selectedItem.click();
    } else if (e.key === "Enter" && selectedItem) {
      e.preventDefault();
      selectedItem.click();
    }
  }

  return (
    <div className="flex h-full flex-col outline-none" tabIndex={-1} data-focus-zone="sidebar" onMouseDown={handleSidebarFocus} onKeyDown={handleSidebarKeyDown}>
      {collapsed ? (
        <CollapsedSidebar />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-1">
          {/* `min-h-0` lets the workspaces card shrink below its content
              height — without it, a sidebar with many sessions grows past
              the viewport and pushes TasksIsland off-screen instead of
              activating the inner overflow-y-auto. */}
          <div className={`flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border-2 ${borderClass} bg-card ${dotGridClass} cluihud-panel-focus`}>
            <div className="flex h-20 shrink-0 items-center px-3 pt-2">
              <NergalLogo />
            </div>

            <div className="flex-1 overflow-y-auto">
              <WorkspacesView />
            </div>
          </div>

          <TasksIsland />
        </div>
      )}

      {/* Always render WorkspacesView hidden when collapsed so the AgentPickerModal still mounts */}
      {collapsed && (
        <div className="hidden">
          <WorkspacesView />
        </div>
      )}
    </div>
  );
}

// Maps the cluihud "rename" action to the per-agent slash command sent into
// the live PTY. Returning `null` means cluihud should only persist the new
// name in its own DB without poking the agent's TUI (used for OpenCode,
// which only supports rename via a Ctrl+R modal that's not safe to drive
// by injecting keystrokes mid-session).
function renameCommandFor(agentId: string): string | null {
  switch (agentId) {
    case "claude-code":
    case "codex":
      return "/rename";
    case "pi":
      return "/name";
    case "opencode":
    default:
      return null;
  }
}

/// Rename + delete shared by the expanded SessionRow callbacks and the
/// collapsed-sidebar hover popover, so both surfaces stay in lockstep.
function useSessionActions() {
  const setWorkspaces = useSetAtom(workspacesAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const deleteWithGrace = useSetAtom(deleteSessionWithGraceAction);

  function renameSession(session: Session, newName: string) {
    invoke("rename_session", { sessionId: session.id, name: newName })
      .then(() => {
        if (terminalService.hasTerminal(session.id)) {
          const cmd = renameCommandFor(session.agent_id ?? "claude-code");
          if (cmd) {
            terminalService.writeToSession(session.id, `${cmd} ${newName}\r`).catch(() => {});
          }
        }
        setWorkspaces((prev) =>
          prev.map((w) => ({
            ...w,
            sessions: w.sessions.map((sess) =>
              sess.id === session.id ? { ...sess, name: newName } : sess,
            ),
          })),
        );
        setFocusZone("terminal");
        requestAnimationFrame(() => terminalService.focusActive());
      })
      .catch(() => {});
  }

  async function deleteSession(session: Session) {
    const ok = await swalConfirm({
      title: "Delete session?",
      body: `<strong>${session.name}</strong> will be removed and its terminal closed.`,
      confirmLabel: "Delete",
      cancelLabel: "Cancel",
      kind: "warning",
      destructive: true,
    });
    if (!ok) return;
    deleteWithGrace(session);
  }

  return { renameSession, deleteSession };
}

function CollapsedSidebar() {
  const workspaces = useAtomValue(workspacesAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const setActiveSessionId = useSetAtom(activeSessionIdAtom);
  const focusZone = useAtomValue(focusZoneAtom);
  const isActive = focusZone === "sidebar";
  const config = useAtomValue(configAtom);
  const isPulsing = useFocusPulse(isActive);
  const showAccent = config.panel_focus_pulse ? isPulsing : isActive;
  const borderClass = showAccent ? "border-primary" : "border-border";
  const dotGridClass = config.sidebar_dot_grid ? "cluihud-dot-grid" : "";
  const { renameSession, deleteSession } = useSessionActions();
  const [renameTarget, setRenameTarget] = useState<Session | null>(null);

  const sessionsWithWs = workspaces.flatMap((w) =>
    w.sessions
      .filter((s) => s.status !== "completed")
      .map((s) => ({ session: s, workspaceName: w.name }))
  );

  return (
    <TooltipProvider delay={0}>
    <div className={`flex h-full w-full flex-col items-center gap-0.5 rounded-lg border-2 ${borderClass} bg-card ${dotGridClass} py-1 cluihud-panel-focus`}>
      <div className="mb-2 mt-1 flex shrink-0 items-center justify-center px-1.5">
        <NergalN size={20} />
      </div>
      {sessionsWithWs.map(({ session: s, workspaceName }) => (
        <Tooltip key={s.id}>
          <TooltipTrigger
            onClick={() => setActiveSessionId(s.id)}
            className={`flex size-4 items-center justify-center rounded transition-colors ${
              s.id === activeSessionId ? "bg-secondary" : "hover:bg-secondary/50"
            }`}
          >
            <SessionIndicator sessionId={s.id} sessionStatus={s.status} size="md" />
          </TooltipTrigger>
          <TooltipContent side="right" className="p-0" sideOffset={4}>
            <div className="px-2.5 py-1.5">
              <p className="text-[9px] text-muted-foreground">{workspaceName}</p>
              <div className="flex items-center gap-1.5">
                <p className="text-[11px] font-medium">{s.name}</p>
                <button
                  type="button"
                  aria-label="Rename"
                  onClick={() => setRenameTarget(s)}
                  className="flex size-4 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                >
                  <Pencil className="size-2.5" />
                </button>
                <button
                  type="button"
                  aria-label="Delete"
                  onClick={() => void deleteSession(s)}
                  className="flex size-4 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-red-400 transition-colors"
                >
                  <Trash2 className="size-2.5" />
                </button>
              </div>
            </div>
          </TooltipContent>
        </Tooltip>
      ))}
    </div>
    <TextInputDialog
      open={renameTarget !== null}
      onOpenChange={(o) => { if (!o) setRenameTarget(null); }}
      title="Rename session"
      initialValue={renameTarget?.name ?? ""}
      confirmLabel="Rename"
      onSubmit={(name) => {
        if (renameTarget && name !== renameTarget.name) renameSession(renameTarget, name);
      }}
    />
    </TooltipProvider>
  );
}

// Persists across mount cycles — a `useRef` inside the component resets
// when WorkspacesView unmounts (sidebar collapses), which replays the
// last Ctrl+N trigger as soon as the sidebar re-expands.
let lastNewSessionConsumed = 0;

function WorkspacesView() {
  const { renameSession, deleteSession } = useSessionActions();
  const deleteWorkspaceWithGrace = useSetAtom(deleteWorkspaceWithGraceAction);
  const workspaces = useAtomValue(workspacesAtom);
  const setWorkspaces = useSetAtom(workspacesAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const setActiveSessionId = useSetAtom(activeSessionIdAtom);
  const setLaunchMode = useSetAtom(sessionLaunchModeAtom);
  const freshSessions = useAtomValue(freshSessionsAtom);
  const setFreshSessions = useSetAtom(freshSessionsAtom);
  const expandedFromAtom = useAtomValue(expandedWorkspaceIdsAtom);
  const setExpandedAtom = useSetAtom(expandedWorkspaceIdsAtom);
  const openTab = useSetAtom(openTabAction);
  const addToast = useSetAtom(toastsAtom);
  const setBootstrapPrompt = useSetAtom(bootstrapPromptAtom);
  const triggerMergeSignal = useAtomValue(triggerMergeAtom);
  const expandedIds = expandedFromAtom ?? new Set<string>();
  const setExpandedIds = (updater: Set<string> | ((prev: Set<string>) => Set<string>)) => {
    setExpandedAtom((prev) => {
      const current = prev ?? new Set<string>();
      return typeof updater === "function" ? updater(current) : updater;
    });
  };
  const [addingSessionFor, setAddingSessionFor] = useState<string | null>(null);
  const [newSessionName, setNewSessionName] = useState("");

  const [projectPickerOpen, setProjectPickerOpen] = useState(false);
  const [agentPicker, setAgentPicker] = useState<
    | { workspaceId: string; sessionName: string; agents: AvailableAgent[]; defaultId: string | null }
    | null
  >(null);
  const triggerResumeId = useAtomValue(triggerResumeSessionAtom);
  const setTriggerResume = useSetAtom(triggerResumeSessionAtom);
  const triggerNewSession = useAtomValue(triggerNewSessionAtom);
  const triggerAddWorkspace = useAtomValue(triggerAddWorkspaceAtom);
  const jumpToProject = useAtomValue(triggerJumpToProjectAtom);
  const setFocusZoneDirect = useSetAtom(focusZoneAtom);
  const setSidebarIdxDirect = useSetAtom(sidebarSelectedIdxAtom);
  const focusedWorkspaceId = useAtomValue(focusedWorkspaceIdAtom);
  const setFocusedWorkspaceId = useSetAtom(focusedWorkspaceIdAtom);
  const focusZone = useAtomValue(focusZoneAtom);

  // Numbers follow focus: clear the focused-workspace override as soon as the
  // user leaves the sidebar without picking anything.
  useEffect(() => {
    if (focusZone !== "sidebar" && focusedWorkspaceId) {
      setFocusedWorkspaceId(null);
    }
  }, [focusZone, focusedWorkspaceId, setFocusedWorkspaceId]);

  useEffect(() => {
    if (!triggerResumeId) return;
    for (const ws of workspaces) {
      const session = ws.sessions.find((s) => s.id === triggerResumeId);
      if (session) {
        setLaunchMode((prev) => ({ ...prev, [session.id]: "continue" }));
        setActiveSessionId(session.id);
        setTriggerResume(null);
        return;
      }
    }
    setTriggerResume(null);
  }, [triggerResumeId]);

  useEffect(() => {
    if (triggerNewSession === 0 || workspaces.length === 0) return;
    if (triggerNewSession <= lastNewSessionConsumed) return;
    lastNewSessionConsumed = triggerNewSession;
    if (workspaces.length === 1) {
      setAddingSessionFor(workspaces[0].id);
    } else {
      setProjectPickerOpen(true);
    }
  }, [triggerNewSession, workspaces.length]);

  useEffect(() => {
    if (triggerAddWorkspace > 0) handleAddWorkspace();
  }, [triggerAddWorkspace]);

  // Merge entry point moved to GitPanel — it owns both the visible button
  // and the Ctrl+Shift+M shortcut handler.
  void triggerMergeSignal;

  const activeSession = useAtomValue(activeSessionAtom);
  void activeSession;

  useEffect(() => {
    invoke<Workspace[]>("get_workspaces")
      .then((ws) => {
        setWorkspaces(ws);
        // Initialize only on first hydration — a subsequent re-mount (e.g.
        // sidebar re-expand) keeps whatever the user toggled.
        if (ws.length > 0 && expandedFromAtom === null) {
          setExpandedAtom(new Set([ws[0].id]));
        }
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (jumpToProject.tick === 0) return;
    const ws = workspaces[jumpToProject.index];
    if (!ws) return;
    setExpandedIds((prev) => {
      const next = new Set(prev);
      next.add(ws.id);
      return next;
    });
    setFocusZoneDirect("sidebar");
    // Move shortcut numbers onto this workspace until the user picks a session
    // or leaves the sidebar (handled by the focus-zone effect above).
    setFocusedWorkspaceId(ws.id);
    requestAnimationFrame(() => {
      const zone = document.querySelector("[data-focus-zone='sidebar']") as HTMLElement | null;
      const el = document.querySelector(`[data-workspace-id="${ws.id}"]`) as HTMLElement | null;
      if (!zone || !el) return;
      const items = Array.from(zone.querySelectorAll<HTMLElement>("[data-nav-item]"));
      for (const item of items) item.removeAttribute("data-nav-selected");
      el.setAttribute("data-nav-selected", "true");
      el.scrollIntoView({ block: "nearest" });
      setSidebarIdxDirect(items.indexOf(el));
      // Focus the sidebar zone (not the button) so arrow keys are handled by handleSidebarKeyDown
      zone.focus();
    });
  }, [jumpToProject.tick]);

  function toggleWorkspace(id: string) {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function handleAddWorkspace() {
    const selected = await open({ directory: true, title: "Select workspace folder" });
    if (!selected) return;
    invoke<Workspace>("create_workspace", { repoPath: selected })
      .then(async (ws) => {
        setWorkspaces((prev) => [...prev, ws]);
        setExpandedIds((prev) => new Set([...prev, ws.id]));
        try {
          const probe = await invoke<{
            vault_root: string;
            expected_path: string;
            inherited: boolean;
          } | null>("obsidian_pre_bootstrap", { workspaceId: ws.id });
          if (probe) {
            setBootstrapPrompt({
              workspaceId: ws.id,
              workspaceName: ws.name,
              expectedPath: probe.expected_path,
              inheritedVault: probe.inherited,
            });
          }
        } catch (err) {
          console.warn("[sidebar] obsidian_pre_bootstrap failed:", err);
        }
      })
      .catch((err) => {
        // The backend returns plain strings (e.g. "Not a git repository").
        // Surfacing the error keeps fresh users from staring at a no-op (the
        // dialog closed, the workspace didn't appear, and previously the catch
        // swallowed the reason).
        const message = typeof err === "string" ? err : (err?.message ?? "Failed to add workspace");
        const description = message === "Not a git repository"
          ? `${selected} is not a git repository. Run "git init" inside it first or pick a different folder.`
          : undefined;
        addToast({ type: "error", message: "Could not add workspace", description: description ?? message });
      });
  }

  async function handleDeleteWorkspace(ws: Workspace) {
    const sessionCount = ws.sessions.length;
    const sessionsLine = sessionCount > 0
      ? `<br /><span class="text-muted-foreground text-[11px]">${sessionCount} session${sessionCount === 1 ? "" : "s"} and their worktrees will be removed.</span>`
      : "";
    const ok = await swalConfirm({
      title: "Remove workspace?",
      body: `<strong>${ws.name}</strong> will be removed from the sidebar.${sessionsLine}`,
      confirmLabel: "Remove",
      cancelLabel: "Cancel",
      kind: "warning",
      destructive: true,
    });
    if (!ok) return;
    setExpandedIds((prev) => {
      const next = new Set(prev);
      next.delete(ws.id);
      return next;
    });
    deleteWorkspaceWithGrace(ws);
  }

  function handleSessionClick(session: Session) {
    if (session.status === "completed") {
      openTab({ tab: { id: `transcript-${session.id}`, type: "transcript", label: `Transcript: ${session.name}`, data: { sessionId: session.id } }, isPinned: true });
      return;
    }
    // Reopening an idle session always resumes that session's own
    // conversation. The backend (pty.rs) swaps the "continue" sentinel for
    // the stored agent_internal_session_id when one is on the session row,
    // so this resolves to `claude --resume <uuid>` / `pi --session <uuid>` /
    // `codex resume <id>` / `opencode --session <id>`. When no UUID is
    // persisted (very short sessions, crash before capture) the adapters
    // fall back to their `--continue` equivalents.
    if (!freshSessions.has(session.id) && !terminalService.hasTerminal(session.id)) {
      setLaunchMode((prev) => ({ ...prev, [session.id]: "continue" }));
    }
    setActiveSessionId(session.id);
    setFocusZoneDirect("terminal");
    requestAnimationFrame(() => terminalService.focusActive());
  }

  async function spawnSession(
    workspaceId: string,
    sessionName: string,
    agentId: string | null,
    launchOptions: LaunchOptions | null,
    envShells: EnvShellDef[] = [],
  ) {
    try {
      const session = await invoke<Session>("create_session", {
        workspaceId,
        name: sessionName,
        agentId,
        launchOptions,
        envShells,
      });

      // Auto-run on creation (the re-open path pre-fills instead). Seeding
      // here, before activation, lets TerminalManager's prefill pass skip
      // this session.
      if (session.env_shells?.length) {
        const ws = workspaces.find((w) => w.id === workspaceId);
        const cwd = session.worktree_path ?? ws?.repo_path;
        if (cwd) spawnEnvShells(session.id, cwd, session.env_shells, true);
      }

      setWorkspaces((prev) =>
        prev.map((w) =>
          w.id === workspaceId
            ? { ...w, sessions: [...w.sessions, session] }
            : w,
        ),
      );
      // Keep the workspace expanded so the new row stays visible and the
      // numeric switch-session shortcuts (Ctrl+1..9) keep targeting it.
      setExpandedIds((prev) => {
        if (prev.has(workspaceId)) return prev;
        const next = new Set(prev);
        next.add(workspaceId);
        return next;
      });
      setFreshSessions((prev: Set<string>) => new Set([...prev, session.id]));
      setActiveSessionId(session.id);
      setAddingSessionFor(null);
      setNewSessionName("");
      setFocusZoneDirect("terminal");
      requestAnimationFrame(() => terminalService.focusActive());
    } catch {
    }
  }

  async function handleCreateSession(workspaceId: string) {
    const sessionName = newSessionName.trim();
    if (!sessionName) return;

    // The picker now opens even with a single installed agent: it's also the
    // launch-options surface (permission mode, skip, startup command), so
    // skipping it would make those unreachable. Enter passes straight
    // through, so the single-agent cost is one keypress.
    let agents: AvailableAgent[] = [];
    try {
      agents = await invoke<AvailableAgent[]>("list_available_agents");
    } catch {
      agents = [];
    }
    const installed = agents.filter((a) => a.installed);
    if (installed.length === 0) {
      // Nothing detected — fall back to the backend's CC default rather than
      // showing an empty modal.
      await spawnSession(workspaceId, sessionName, null, null);
      return;
    }

    const ws = workspaces.find((w) => w.id === workspaceId);
    let defaultId: string | null = null;
    try {
      if (ws?.repo_path) {
        defaultId = await invoke<string>("resolve_default_agent", {
          projectPath: ws.repo_path,
        });
      }
    } catch {
      defaultId = null;
    }

    setAgentPicker({
      workspaceId,
      sessionName,
      agents: installed,
      defaultId,
    });
  }

  return (
    <div className="py-1">
      {/* Sticky inside the workspaces scroll container so the title + add
          button stay visible when the user has enough sessions to overflow. */}
      <div className="sticky top-0 z-10 flex items-center justify-between bg-card px-3 py-1.5">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Workspaces
        </span>
        <div className="flex items-center gap-1">
          <button
            onClick={handleAddWorkspace}
            className="flex size-5 items-center justify-center rounded text-muted-foreground hover:text-foreground transition-colors"
            aria-label="Add workspace"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 5v14" /><path d="M5 12h14" />
            </svg>
          </button>
        </div>
      </div>

      {(() => {
        // Numbers follow the sidebar-focused workspace first, then fall back
        // to the workspace owning the active session, then the first populated.
        const activeWs = workspaces.find((w) => w.sessions.some((s) => s.id === activeSessionId))
          ?? workspaces.find((w) => w.sessions.length > 0);
        const shortcutWsId = focusZone === "sidebar" && focusedWorkspaceId
          ? focusedWorkspaceId
          : activeWs?.id;
        return workspaces.map((ws) => {
          const isExpanded = expandedIds.has(ws.id);
          const filteredSessions = ws.sessions.filter((s) => s.status !== "completed");
          const isShortcutWorkspace = ws.id === shortcutWsId;
          const nonCompletedSessions = ws.sessions.filter((s) => s.status !== "completed");
        return (
          <div key={ws.id}>
            <div
              role="button"
              tabIndex={0}
              data-nav-item
              data-workspace-id={ws.id}
              data-nav-expanded={isExpanded ? "true" : "false"}
              onClick={() => toggleWorkspace(ws.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  toggleWorkspace(ws.id);
                }
              }}
              className="group flex w-full items-center gap-1.5 px-3 py-1 text-left hover:bg-secondary/40 transition-colors cursor-pointer"
            >
              <svg
                width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"
                className={`shrink-0 text-muted-foreground transition-transform ${isExpanded ? "rotate-90" : ""}`}
              >
                <polyline points="9 18 15 12 9 6" />
              </svg>
              <span className="flex-1 truncate text-[11px] font-medium text-foreground/90">
                {ws.name}
              </span>
              {!ws.is_git && (
                <Tooltip>
                  <TooltipTrigger
                    render={
                      <span className="shrink-0 rounded bg-amber-500/15 px-1 py-0.5 text-[8px] font-medium uppercase tracking-wider text-amber-400" />
                    }
                  >
                    non-git
                  </TooltipTrigger>
                  <TooltipContent side="top" className="max-w-56 text-[10px]">
                    No git repo here: sessions share this directory (no worktrees) and Git/Ship/Merge are disabled. Use "Init git" in the Git panel to enable them.
                  </TooltipContent>
                </Tooltip>
              )}
              <Tooltip>
                <TooltipTrigger
                  render={
                    <span
                      role="button"
                      tabIndex={0}
                      aria-label="Remove workspace"
                      onClick={(e) => { e.stopPropagation(); handleDeleteWorkspace(ws); }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          e.stopPropagation();
                          handleDeleteWorkspace(ws);
                        }
                      }}
                      className="hidden size-4 shrink-0 items-center justify-center rounded text-muted-foreground/70 hover:bg-secondary hover:text-foreground transition-colors group-hover:flex"
                    >
                      <svg
                        width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                        strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"
                      >
                        <path d="M18 6 6 18" /><path d="m6 6 12 12" />
                      </svg>
                    </span>
                  }
                />
                <TooltipContent side="top" className="text-[10px]">Remove workspace</TooltipContent>
              </Tooltip>
            </div>

            {isExpanded && (
              <>
                {filteredSessions.map((s) => {
                  const shortcutIdx = isShortcutWorkspace ? nonCompletedSessions.indexOf(s) : -1;
                  const shortcutNumber = shortcutIdx >= 0 && shortcutIdx < 9 ? shortcutIdx + 1 : undefined;
                  return (
                  <SessionRow
                    key={s.id}
                    session={s}
                    workspace={ws}
                    isActive={activeSessionId === s.id}
                    shortcutNumber={shortcutNumber}
                    onSelect={() => handleSessionClick(s)}
                    onRename={(newName) => renameSession(s, newName)}
                    onDelete={() => void deleteSession(s)}
                  />
                  );
                })}

                {addingSessionFor === ws.id ? (
                  <div className="flex items-center gap-1 pl-7 pr-3 py-1">
                    <input
                      type="text"
                      value={newSessionName}
                      onChange={(e) => setNewSessionName(e.target.value.replace(/[^a-zA-Z0-9áéíóúñÁÉÍÓÚÑ\s]/g, ""))}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") handleCreateSession(ws.id);
                        if (e.key === "Escape") { setAddingSessionFor(null); setNewSessionName(""); }
                      }}
                      placeholder="Session name..."
                      className="flex-1 h-5 bg-transparent border-b border-border text-[11px] text-foreground outline-none placeholder:text-muted-foreground/50"
                      autoFocus
                    />
                  </div>
                ) : (
                  <button
                    onClick={() => setAddingSessionFor(ws.id)}
                    className="flex w-full items-center gap-1.5 pl-7 pr-3 py-1 text-left text-muted-foreground/50 hover:text-muted-foreground transition-colors"
                  >
                    <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M12 5v14" /><path d="M5 12h14" />
                    </svg>
                    <span className="text-[10px]">New session</span>
                  </button>
                )}
              </>
            )}
          </div>
        );
        });
      })()}

      {workspaces.length === 0 && (
        <div className="flex items-center justify-center py-8">
          <span className="text-[11px] text-muted-foreground">No workspaces</span>
        </div>
      )}

      {/* MergeModal is now hosted in GitPanel (single entry point). The
          Ctrl+Shift+M shortcut still triggers it via triggerMergeAtom which
          GitPanel listens to. */}
      <ProjectPickerModal
        open={projectPickerOpen}
        onOpenChange={setProjectPickerOpen}
        workspaces={workspaces}
        preselectedId={activeSession?.workspace_id ?? null}
        onPick={(wsId) => {
          setExpandedIds((prev) => { const next = new Set(prev); next.add(wsId); return next; });
          setAddingSessionFor(wsId);
          // Dialog unmount steals focus — re-focus the new-session input after the next paint.
          setTimeout(() => {
            const input = document.querySelector(`input[placeholder="Session name..."]`) as HTMLInputElement | null;
            input?.focus();
          }, 50);
        }}
      />
      {agentPicker && (
        <AgentPickerModal
          open={true}
          onOpenChange={(o) => { if (!o) setAgentPicker(null); }}
          agents={agentPicker.agents}
          sessionName={agentPicker.sessionName}
          workspaceId={agentPicker.workspaceId}
          preselectedId={agentPicker.defaultId}
          onPick={(agentId, launchOptions, envShells) => {
            const { workspaceId, sessionName } = agentPicker;
            setAgentPicker(null);
            void spawnSession(workspaceId, sessionName, agentId, launchOptions, envShells);
          }}
        />
      )}
    </div>
  );
}
