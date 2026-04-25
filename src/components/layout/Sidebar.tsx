import { useState, useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { focusZoneAtom, previousNonTerminalZoneAtom, triggerResumeSessionAtom, triggerNewSessionAtom, triggerAddWorkspaceAtom, triggerCommitAtom, triggerMergeAtom, triggerJumpToProjectAtom, sidebarSelectedIdxAtom, focusedWorkspaceIdAtom } from "@/stores/shortcuts";
import {
  workspacesAtom,
  activeSessionIdAtom,
  showCompletedAtom,
  sessionLaunchModeAtom,
  freshSessionsAtom,
  activeSessionAtom,
  type Workspace,
  type Session,
} from "@/stores/workspace";
import { openTabAction } from "@/stores/rightPanel";
import { toastsAtom } from "@/stores/toast";
import { refreshConflictedFilesAtom } from "@/stores/git";
import { openConflictsTabAction } from "@/stores/conflict";
import { SessionRow } from "@/components/session/SessionRow";
import { SessionIndicator } from "@/components/session/SessionIndicator";
import { ResumeModal } from "@/components/session/ResumeModal";
import { MergeModal } from "@/components/session/MergeModal";
import { CommitModal } from "@/components/session/CommitModal";
import { ProjectPickerModal } from "@/components/session/ProjectPickerModal";
import Swal from "sweetalert2";
import { invoke } from "@/lib/tauri";
import { open } from "@tauri-apps/plugin-dialog";
import * as terminalService from "@/components/terminal/terminalService";
import { TasksIsland } from "@/components/tasks/TasksIsland";
import { Eye, EyeOff } from "lucide-react";
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

  function handleSidebarFocus() {
    setFocusZone("sidebar");
    setPreviousZone("sidebar");
    if (sidebarSelectedIdx < 0) setSidebarSelectedIdx(0);
  }

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
        <div className="flex flex-1 flex-col gap-1">
          {/* Workspaces card */}
          <div className="flex flex-1 flex-col overflow-hidden rounded bg-card">
            <div className="flex h-9 shrink-0 items-center px-3">
              <span className="flex-1 text-[11px] font-medium text-foreground/80">Workspaces</span>
            </div>

            <div className="flex-1 overflow-y-auto">
              <WorkspacesView />
            </div>
          </div>

          {/* Tasks island — separate card */}
          <TasksIsland />
        </div>
      )}

      {/* Always render WorkspacesView hidden when collapsed so modals (ResumeModal) still work */}
      {collapsed && (
        <div className="hidden">
          <WorkspacesView />
        </div>
      )}
    </div>
  );
}

function CollapsedSidebar() {
  const workspaces = useAtomValue(workspacesAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const setActiveSessionId = useSetAtom(activeSessionIdAtom);

  const sessionsWithWs = workspaces.flatMap((w) =>
    w.sessions
      .filter((s) => s.status !== "completed")
      .map((s) => ({ session: s, workspaceName: w.name }))
  );

  return (
    <TooltipProvider delay={0}>
    <div className="flex h-full w-full flex-col items-center gap-0.5 bg-card py-1">
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
              <p className="text-[11px] font-medium">{s.name}</p>
            </div>
          </TooltipContent>
        </Tooltip>
      ))}
    </div>
    </TooltipProvider>
  );
}

function WorkspacesView() {
  const workspaces = useAtomValue(workspacesAtom);
  const setWorkspaces = useSetAtom(workspacesAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const setActiveSessionId = useSetAtom(activeSessionIdAtom);
  const showCompleted = useAtomValue(showCompletedAtom);
  const setShowCompleted = useSetAtom(showCompletedAtom);
  const setLaunchMode = useSetAtom(sessionLaunchModeAtom);
  const freshSessions = useAtomValue(freshSessionsAtom);
  const setFreshSessions = useSetAtom(freshSessionsAtom);
  const openTab = useSetAtom(openTabAction);
  const addToast = useSetAtom(toastsAtom);
  const refreshConflicts = useSetAtom(refreshConflictedFilesAtom);
  const openConflictsTab = useSetAtom(openConflictsTabAction);
  const triggerCommitSignal = useAtomValue(triggerCommitAtom);
  const triggerMergeSignal = useAtomValue(triggerMergeAtom);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const [addingSessionFor, setAddingSessionFor] = useState<string | null>(null);
  const [newSessionName, setNewSessionName] = useState("");

  const [resumeModal, setResumeModal] = useState<{ session: Session } | null>(null);
  const [mergeModal, setMergeModal] = useState<{ session: Session; workspaceId: string } | null>(null);
  const [commitModal, setCommitModal] = useState<{ session: Session } | null>(null);
  const [projectPickerOpen, setProjectPickerOpen] = useState(false);
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
        setResumeModal({ session });
        setTriggerResume(null);
        return;
      }
    }
    setTriggerResume(null);
  }, [triggerResumeId]);

  useEffect(() => {
    if (triggerNewSession > 0 && workspaces.length > 0) {
      if (workspaces.length === 1) {
        setAddingSessionFor(workspaces[0].id);
      } else {
        setProjectPickerOpen(true);
      }
    }
  }, [triggerNewSession]);

  useEffect(() => {
    if (triggerAddWorkspace > 0) handleAddWorkspace();
  }, [triggerAddWorkspace]);

  useEffect(() => {
    if (triggerCommitSignal === 0 || !activeSessionId) return;
    const session = workspaces.flatMap((w) => w.sessions).find((s) => s.id === activeSessionId);
    if (!session) return;
    invoke<{ dirty: boolean; commits_ahead: boolean }>("check_session_has_commits", { sessionId: activeSessionId })
      .then((status) => {
        if (status.dirty) setCommitModal({ session });
        else addToast({ message: "Commit", description: "Nothing to commit", type: "info" });
      })
      .catch(() => {});
  }, [triggerCommitSignal]);

  useEffect(() => {
    if (triggerMergeSignal === 0 || !activeSessionId) return;
    const ws = workspaces.find((w) => w.sessions.some((s) => s.id === activeSessionId));
    const session = ws?.sessions.find((s) => s.id === activeSessionId);
    if (!ws || !session) return;
    invoke<{ dirty: boolean; commits_ahead: boolean }>("check_session_has_commits", { sessionId: activeSessionId })
      .then((status) => {
        if (status.dirty) addToast({ message: "Merge", description: "Commit your changes first", type: "info" });
        else if (status.commits_ahead) setMergeModal({ session, workspaceId: ws.id });
        else addToast({ message: "Merge", description: "Nothing to merge", type: "info" });
      })
      .catch(() => {});
  }, [triggerMergeSignal]);

  const activeSession = useAtomValue(activeSessionAtom);
  void activeSession;

  useEffect(() => {
    invoke<Workspace[]>("get_workspaces")
      .then((ws) => {
        setWorkspaces(ws);
        if (ws.length > 0) {
          setExpandedIds(new Set([ws[0].id]));
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
      .then((ws) => {
        setWorkspaces((prev) => [...prev, ws]);
        setExpandedIds((prev) => new Set([...prev, ws.id]));
      })
      .catch(() => {});
  }

  function handleSessionClick(session: Session) {
    if (session.status === "completed") {
      openTab({ tab: { id: `transcript-${session.id}`, type: "transcript", label: `Transcript: ${session.name}`, data: { sessionId: session.id } }, isPinned: true });
      return;
    }
    if (!freshSessions.has(session.id) && !terminalService.hasTerminal(session.id)) {
      setResumeModal({ session });
      return;
    }
    setActiveSessionId(session.id);
  }

  function handleResume(mode: "continue" | "resume_pick") {
    if (!resumeModal) return;
    setLaunchMode((prev) => ({ ...prev, [resumeModal.session.id]: mode }));
    setActiveSessionId(resumeModal.session.id);
    setResumeModal(null);
  }

  function handleCommitConfirm(lang: string) {
    if (!commitModal) return;
    const sid = commitModal.session.id;
    setActiveSessionId(sid);
    setCommitModal(null);
    terminalService.writeToSession(sid, `/commit ${lang}\r`)
      .then(() => terminalService.focusActive())
      .catch(() => {});
  }

  async function handleCreateSession(workspaceId: string) {
    if (!newSessionName.trim()) return;
    try {
      const session = await invoke<Session>("create_session", {
        workspaceId,
        name: newSessionName.trim(),
      });

      setWorkspaces((prev) =>
        prev.map((w) =>
          w.id === workspaceId
            ? { ...w, sessions: [...w.sessions, session] }
            : w,
        ),
      );
      setFreshSessions((prev: Set<string>) => new Set([...prev, session.id]));
      setActiveSessionId(session.id);
      setAddingSessionFor(null);
      setNewSessionName("");
    } catch {
      // handled by toast in future
    }
  }

  return (
    <div className="py-1">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-1.5">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Workspaces
        </span>
        <div className="flex items-center gap-1">
          <Tooltip>
            <TooltipTrigger
              render={
                <div
                  role="button"
                  onClick={() => setShowCompleted(!showCompleted)}
                  className="flex size-5 items-center justify-center rounded text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
                  aria-label={showCompleted ? "Hide completed" : "Show completed"}
                />
              }
            >
              {showCompleted ? <Eye className="size-3" /> : <EyeOff className="size-3" />}
            </TooltipTrigger>
            <TooltipContent side="top">{showCompleted ? "Hide completed" : "Show completed"}</TooltipContent>
          </Tooltip>
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
          const filteredSessions = showCompleted
            ? ws.sessions
            : ws.sessions.filter((s) => s.status !== "completed");
          const isShortcutWorkspace = ws.id === shortcutWsId;
          const nonCompletedSessions = ws.sessions.filter((s) => s.status !== "completed");
        return (
          <div key={ws.id}>
            <button
              data-nav-item
              data-workspace-id={ws.id}
              data-nav-expanded={isExpanded ? "true" : "false"}
              onClick={() => toggleWorkspace(ws.id)}
              className="flex w-full items-center gap-1.5 px-3 py-1 text-left hover:bg-secondary/40 transition-colors"
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
            </button>

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
                    onRename={(newName) => {
                      invoke("rename_session", { sessionId: s.id, name: newName })
                        .then(() => {
                          if (terminalService.hasTerminal(s.id)) {
                            terminalService.writeToSession(s.id, `/rename ${newName}\r`).catch(() => {});
                          }
                          setWorkspaces((prev) =>
                            prev.map((w) => ({
                              ...w,
                              sessions: w.sessions.map((sess) =>
                                sess.id === s.id ? { ...sess, name: newName } : sess,
                              ),
                            })),
                          );
                        })
                        .catch(() => {});
                    }}
                    onDelete={() => {
                      if (!window.confirm(`Delete session "${s.name}"?`)) return;
                      terminalService.destroy(s.id);
                      invoke("delete_session", { sessionId: s.id })
                        .then(() => {
                          setWorkspaces((prev) =>
                            prev.map((w) => ({
                              ...w,
                              sessions: w.sessions.filter((sess) => sess.id !== s.id),
                            })),
                          );
                          if (activeSessionId === s.id) setActiveSessionId(null);
                        })
                        .catch(() => {});
                    }}
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

      {resumeModal && (
        <ResumeModal
          open={true}
          onOpenChange={(o) => { if (!o) setResumeModal(null); }}
          sessionName={resumeModal.session.name}
          onSelect={handleResume}
        />
      )}
      {mergeModal && (
        <MergeModal
          open={true}
          onOpenChange={(o) => { if (!o) setMergeModal(null); }}
          session={mergeModal.session}
          workspaceId={mergeModal.workspaceId}
          onMerged={() => {
            const merged = mergeModal?.session;
            Swal.fire({
              title: "Merge complete",
              text: merged ? `Delete the worktree for "${merged.name}"?` : "Delete the worktree?",
              icon: "question",
              showCancelButton: true,
              confirmButtonText: "Delete worktree",
              cancelButtonText: "Keep worktree",
              background: "#171717",
              color: "#ededef",
              confirmButtonColor: "#ef4444",
              cancelButtonColor: "#3f3f46",
            }).then((result) => {
              if (result.isConfirmed && merged) {
                invoke("cleanup_merged_session", { sessionId: merged.id })
                  .catch(() => {})
                  .finally(() => {
                    invoke<Workspace[]>("get_workspaces").then(setWorkspaces).catch(() => {});
                  });
              } else {
                invoke<Workspace[]>("get_workspaces").then(setWorkspaces).catch(() => {});
              }
            });
          }}
          onConflict={(targetBranch) => {
            if (!mergeModal) return;
            const sid = mergeModal.session.id;
            setActiveSessionId(sid);
            setMergeModal(null);
            invoke<string[]>("pull_target_into_session", { sessionId: sid, target: targetBranch })
              .then((files) => {
                if (files.length === 0) {
                  addToast({ message: "Merge", description: "No conflicts after pull — ready to commit", type: "info" });
                  return;
                }
                refreshConflicts(sid);
                openConflictsTab({ sessionId: sid, path: files[0] });
                addToast({ message: "Conflicts", description: `${files.length} file(s) — resolve via the Conflicts tab`, type: "info" });
              })
              .catch((e: unknown) => addToast({ message: "Pull failed", description: String(e), type: "error" }));
          }}
        />
      )}
      {commitModal && (
        <CommitModal
          open={true}
          onOpenChange={(o) => { if (!o) setCommitModal(null); }}
          onConfirm={handleCommitConfirm}
        />
      )}
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
    </div>
  );
}
