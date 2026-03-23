import { useState, useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { focusZoneAtom, previousNonTerminalZoneAtom, triggerResumeSessionAtom, triggerNewSessionAtom, triggerAddWorkspaceAtom } from "@/stores/shortcuts";
import {
  workspacesAtom,
  activeSessionIdAtom,
  showCompletedAtom,
  sessionLaunchModeAtom,
  freshSessionsAtom,
  type Workspace,
  type Session,
} from "@/stores/workspace";
import { openTabAction } from "@/stores/rightPanel";
import { toastsAtom } from "@/stores/toast";
import { SessionRow } from "@/components/session/SessionRow";
import { ResumeModal } from "@/components/session/ResumeModal";
import { MergeModal } from "@/components/session/MergeModal";
import { CommitModal } from "@/components/session/CommitModal";
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
  onToggle: () => void;
}

export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  const setFocusZone = useSetAtom(focusZoneAtom);
  const setPreviousZone = useSetAtom(previousNonTerminalZoneAtom);

  function handleSidebarFocus() {
    setFocusZone("sidebar");
    setPreviousZone("sidebar");
  }

  return (
    <div className="flex h-full flex-col outline-none" tabIndex={-1} data-focus-zone="sidebar" onMouseDown={handleSidebarFocus}>
      {collapsed ? (
        <CollapsedSidebar onToggle={onToggle} />
      ) : (
        <div className="flex flex-1 flex-col gap-1.5">
          {/* Workspaces card */}
          <div className="flex flex-1 flex-col overflow-hidden rounded-lg bg-card">
            <div className="flex h-9 shrink-0 items-center border-b border-border/50 px-3">
              <span className="flex-1 text-[11px] font-medium text-foreground/80">Workspaces</span>
              <button
                onClick={onToggle}
                className="flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                aria-label="Collapse sidebar"
              >
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <polyline points="15 18 9 12 15 6" />
                </svg>
              </button>
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

const STATUS_DOT: Record<string, string> = {
  running: "bg-green-500",
  idle: "bg-muted-foreground",
  thinking: "bg-yellow-500",
  completed: "bg-muted-foreground/40",
};

function CollapsedSidebar({ onToggle }: { onToggle: () => void }) {
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
    <div className="flex h-full w-full flex-col items-center gap-0.5 bg-background py-1">
      <button
        onClick={onToggle}
        className="flex size-4 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors mb-0.5"
        aria-label="Expand sidebar"
      >
        <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="9 18 15 12 9 6" />
        </svg>
      </button>
      {sessionsWithWs.map(({ session: s, workspaceName }) => (
        <Tooltip key={s.id}>
          <TooltipTrigger
            onClick={() => setActiveSessionId(s.id)}
            className={`flex size-4 items-center justify-center rounded transition-colors ${
              s.id === activeSessionId ? "bg-secondary" : "hover:bg-secondary/50"
            }`}
          >
            <span className={`size-1.5 rounded-full ${STATUS_DOT[s.status] ?? "bg-muted-foreground"} ${s.status === "running" ? "animate-dot-pulse" : ""}`} />
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
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const [addingSessionFor, setAddingSessionFor] = useState<string | null>(null);
  const [newSessionName, setNewSessionName] = useState("");

  const [resumeModal, setResumeModal] = useState<{ session: Session } | null>(null);
  const [mergeModal, setMergeModal] = useState<{ session: Session; workspaceId: string } | null>(null);
  const [commitModal, setCommitModal] = useState<{ session: Session } | null>(null);
  const triggerResumeId = useAtomValue(triggerResumeSessionAtom);
  const setTriggerResume = useSetAtom(triggerResumeSessionAtom);
  const triggerNewSession = useAtomValue(triggerNewSessionAtom);
  const triggerAddWorkspace = useAtomValue(triggerAddWorkspaceAtom);

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
      setAddingSessionFor(workspaces[0].id);
    }
  }, [triggerNewSession]);

  useEffect(() => {
    if (triggerAddWorkspace > 0) handleAddWorkspace();
  }, [triggerAddWorkspace]);

  useEffect(() => {
    invoke<Workspace[]>("get_workspaces")
      .then((ws) => {
        setWorkspaces(ws);
        if (ws.length > 0) {
          setExpandedIds(new Set([ws[0].id]));
        }
      })
      .catch(() => {});
  }, []);

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

      {workspaces.map((ws) => {
        const isExpanded = expandedIds.has(ws.id);
        const filteredSessions = showCompleted
          ? ws.sessions
          : ws.sessions.filter((s) => s.status !== "completed");
        return (
          <div key={ws.id}>
            <button
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
                {filteredSessions.map((s) => (
                  <SessionRow
                    key={s.id}
                    session={s}
                    workspace={ws}
                    isActive={activeSessionId === s.id}
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
                    onCommit={() => {
                      invoke<{ dirty: boolean; commits_ahead: boolean }>("check_session_has_commits", { sessionId: s.id })
                        .then((status) => {
                          if (status.dirty) {
                            setCommitModal({ session: s });
                          } else {
                            addToast({ message: "Info", description: "Nothing to commit", type: "info" });
                          }
                        })
                        .catch(() => addToast({ message: "Error", description: "Failed to check status", type: "error" }));
                    }}
                    onMerge={() => {
                      invoke<{ dirty: boolean; commits_ahead: boolean }>("check_session_has_commits", { sessionId: s.id })
                        .then((status) => {
                          if (status.dirty) {
                            addToast({ message: "Info", description: "Commit your changes first", type: "info" });
                          } else if (status.commits_ahead) {
                            setMergeModal({ session: s, workspaceId: ws.id });
                          } else {
                            addToast({ message: "Info", description: "Nothing to merge", type: "info" });
                          }
                        })
                        .catch(() => addToast({ message: "Error", description: "Failed to check status", type: "error" }));
                    }}
                  />
                ))}

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
      })}

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
            invoke<Workspace[]>("get_workspaces")
              .then(setWorkspaces)
              .catch(() => {});
          }}
          onConflict={(targetBranch, detail) => {
            if (!mergeModal) return;
            const sid = mergeModal.session.id;
            const branch = mergeModal.session.worktree_branch ?? "this branch";
            setActiveSessionId(sid);
            setMergeModal(null);
            setTimeout(() => {
              terminalService.writeToSession(sid,
                `I need to merge ${branch} into ${targetBranch} but there are conflicts. Run these commands in this worktree to reproduce and resolve:\n1. git merge ${targetBranch}\n2. Resolve the conflicts in the affected files\n3. git add the resolved files\n4. git commit\n\nThe conflicting files from a prior attempt:\n${detail}\r`
              ).then(() => terminalService.focusActive()).catch(() => {});
            }, 500);
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
    </div>
  );
}
