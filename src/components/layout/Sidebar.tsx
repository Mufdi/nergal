import { useState, useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  workspacesAtom,
  activeSessionIdAtom,
  type Workspace,
  type Session,
} from "@/stores/workspace";
import { TaskPanel } from "@/components/tasks/TaskPanel";
import { invoke } from "@/lib/tauri";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

type SidebarTab = "workspaces" | "tasks" | "git";

interface SidebarProps {
  collapsed: boolean;
  onToggle: () => void;
}

export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  const [activeTab, setActiveTab] = useState<SidebarTab>("workspaces");

  if (collapsed) {
    return (
      <div className="flex h-full w-full flex-col items-center gap-1 bg-background py-2">
        <button
          onClick={onToggle}
          className="flex size-7 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
          aria-label="Expand sidebar"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="9 18 15 12 9 6" />
          </svg>
        </button>

        {(["workspaces", "tasks", "git"] as const).map((tab) => (
          <Tooltip key={tab}>
            <TooltipTrigger
              render={
                <button
                  className={`flex size-7 items-center justify-center rounded transition-colors ${
                    activeTab === tab
                      ? "text-foreground bg-secondary"
                      : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                  }`}
                  aria-label={tab}
                />
              }
            >
              <TabIcon tab={tab} />
            </TooltipTrigger>
            <TooltipContent side="right" className="capitalize">{tab}</TooltipContent>
          </Tooltip>
        ))}
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-lg bg-card">
      <div className="flex h-9 shrink-0 items-center border-b border-border/50">
        <div className="flex flex-1 items-stretch h-full">
          {(["workspaces", "tasks", "git"] as const).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`flex-1 text-[11px] font-medium capitalize transition-colors ${
                activeTab === tab
                  ? "text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {tab}
            </button>
          ))}
        </div>
        <button
          onClick={onToggle}
          className="flex size-6 shrink-0 mr-1.5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
          aria-label="Collapse sidebar"
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="15 18 9 12 15 6" />
          </svg>
        </button>
      </div>

      <div className="flex-1 overflow-y-auto">
        {activeTab === "workspaces" && <WorkspacesView />}
        {activeTab === "tasks" && <TaskPanel />}
        {activeTab === "git" && (
          <div className="flex items-center justify-center py-8">
            <span className="text-[11px] text-muted-foreground">Coming soon</span>
          </div>
        )}
      </div>
    </div>
  );
}

const STATUS_DOT_COLORS: Record<Session["status"], string> = {
  idle: "bg-muted-foreground/60",
  running: "bg-green-500",
  needs_attention: "bg-orange-500",
  completed: "bg-muted-foreground/30",
};

function WorkspacesView() {
  const workspaces = useAtomValue(workspacesAtom);
  const setWorkspaces = useSetAtom(workspacesAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const setActiveSessionId = useSetAtom(activeSessionIdAtom);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const [addingSessionFor, setAddingSessionFor] = useState<string | null>(null);
  const [newSessionName, setNewSessionName] = useState("");

  // Load workspaces on mount
  useEffect(() => {
    invoke<Workspace[]>("get_workspaces")
      .then((ws) => {
        setWorkspaces(ws);
        // Auto-expand first workspace
        if (ws.length > 0) {
          setExpandedIds(new Set([ws[0].id]));
        }
        // Auto-select first session if none active
        if (!activeSessionId && ws.length > 0 && ws[0].sessions.length > 0) {
          setActiveSessionId(ws[0].sessions[0].id);
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

  function handleSelectSession(session: Session) {
    setActiveSessionId(session.id);
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
      setActiveSessionId(session.id);
      setAddingSessionFor(null);
      setNewSessionName("");
    } catch {
      // handled by toast in future
    }
  }

  function formatAge(timestamp: number): string {
    const delta = Math.floor(Date.now() / 1000 - timestamp);
    if (delta < 60) return "now";
    if (delta < 3600) return `${Math.floor(delta / 60)}m`;
    if (delta < 86400) return `${Math.floor(delta / 3600)}h`;
    return `${Math.floor(delta / 86400)}d`;
  }

  return (
    <div className="py-1">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-1.5">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Workspaces
        </span>
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

      {workspaces.map((ws) => {
        const isExpanded = expandedIds.has(ws.id);
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
                {ws.sessions.map((s) => (
                  <button
                    key={s.id}
                    onClick={() => handleSelectSession(s)}
                    className={`flex w-full items-center gap-1.5 pl-7 pr-3 py-1 text-left transition-colors ${
                      activeSessionId === s.id
                        ? "bg-secondary/60 text-foreground"
                        : "hover:bg-secondary/40 text-foreground/70"
                    }`}
                  >
                    <span
                      className={`size-1.5 shrink-0 rounded-full ${STATUS_DOT_COLORS[s.status]}`}
                      aria-hidden="true"
                    />
                    <span className="flex-1 truncate text-[11px]">
                      {s.name}
                    </span>
                    <span className="shrink-0 text-[10px] text-muted-foreground/50 tabular-nums">
                      {formatAge(s.updated_at)}
                    </span>
                  </button>
                ))}

                {/* Add session inline form */}
                {addingSessionFor === ws.id ? (
                  <div className="flex items-center gap-1 pl-7 pr-3 py-1">
                    <input
                      type="text"
                      value={newSessionName}
                      onChange={(e) => setNewSessionName(e.target.value)}
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
    </div>
  );
}

function TabIcon({ tab }: { tab: SidebarTab }) {
  switch (tab) {
    case "workspaces":
      return (
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
        </svg>
      );
    case "tasks":
      return (
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M9 11l3 3L22 4" /><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
        </svg>
      );
    case "git":
      return (
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="18" cy="18" r="3" /><circle cx="6" cy="6" r="3" /><path d="M6 21V9a9 9 0 0 0 9 9" />
        </svg>
      );
  }
}
