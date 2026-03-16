import { useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { workspacesAtom, activeSessionIdAtom } from "@/stores/workspace";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

interface NavSidebarProps {
  onOpenSettings: () => void;
}

export function NavSidebar({ onOpenSettings }: NavSidebarProps) {
  const [expanded, setExpanded] = useState(false);
  const workspaces = useAtomValue(workspacesAtom);
  const setActiveSessionId = useSetAtom(activeSessionIdAtom);

  const totalSessions = workspaces.reduce((acc, ws) => acc + ws.sessions.length, 0);
  const activeCount = workspaces.reduce(
    (acc, ws) => acc + ws.sessions.filter((s) => s.status === "running").length,
    0,
  );

  if (expanded) {
    return (
      <aside
        className="flex w-52 flex-col overflow-hidden rounded-xl bg-card"
        aria-label="Navigation"
      >
        {/* Header */}
        <div className="flex h-9 shrink-0 items-center justify-between border-b border-border/50 px-3">
          <span className="text-xs font-medium text-foreground">Workspaces</span>
          <button
            onClick={() => setExpanded(false)}
            className="flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
            aria-label="Collapse sidebar"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="15 18 9 12 15 6" />
            </svg>
          </button>
        </div>

        {/* Workspace tree */}
        <div className="flex-1 overflow-y-auto py-1">
          {workspaces.length === 0 ? (
            <div className="flex items-center justify-center py-8">
              <span className="text-xs text-muted-foreground">No sessions</span>
            </div>
          ) : (
            workspaces.map((ws) => (
              <div key={ws.id} className="px-1.5 py-0.5">
                {/* Workspace name */}
                <div className="flex items-center gap-1.5 rounded-md px-2 py-1">
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="shrink-0 text-muted-foreground">
                    <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
                  </svg>
                  <span className="truncate text-xs font-medium text-foreground">{ws.name}</span>
                </div>

                {/* Sessions */}
                {ws.sessions.map((s) => (
                  <button
                    key={s.id}
                    onClick={() => setActiveSessionId(s.id)}
                    className="flex w-full items-center gap-1.5 rounded-md px-2 py-1 pl-6 text-left transition-colors hover:bg-accent"
                  >
                    <span
                      className={`inline-block size-1.5 shrink-0 rounded-full ${s.status === "running" ? "bg-green-500" : "bg-muted-foreground/50"}`}
                    />
                    <span className="truncate text-xs text-muted-foreground">
                      {s.name || s.id.slice(0, 8)}
                    </span>
                  </button>
                ))}
              </div>
            ))
          )}
        </div>

        {/* Footer: settings */}
        <div className="flex shrink-0 items-center border-t border-border/50 px-2 py-1.5">
          <button
            onClick={onOpenSettings}
            className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
              <circle cx="12" cy="12" r="3" />
            </svg>
            <span className="text-xs">Settings</span>
          </button>
        </div>
      </aside>
    );
  }

  // Collapsed: icon rail
  return (
    <aside
      className="flex w-10 flex-col items-center bg-background py-2 gap-1"
      aria-label="Navigation"
    >
      {/* Expand */}
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => setExpanded(true)}
              aria-label="Expand sidebar"
            />
          }
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-muted-foreground">
            <polyline points="9 18 15 12 9 6" />
          </svg>
        </TooltipTrigger>
        <TooltipContent side="right">Workspaces</TooltipContent>
      </Tooltip>

      {/* Sessions count */}
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => setExpanded(true)}
              aria-label={`${totalSessions} sessions`}
            />
          }
        >
          <span className="relative text-sm text-muted-foreground">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <rect x="3" y="3" width="18" height="18" rx="2" />
              <path d="M7 8h10" />
              <path d="M7 12h6" />
            </svg>
            {activeCount > 0 && (
              <span className="absolute -right-1.5 -top-1.5 flex size-3.5 items-center justify-center rounded-full bg-primary text-[9px] font-bold text-primary-foreground">
                {activeCount}
              </span>
            )}
          </span>
        </TooltipTrigger>
        <TooltipContent side="right">
          {totalSessions} session{totalSessions !== 1 && "s"} ({activeCount} active)
        </TooltipContent>
      </Tooltip>

      <div className="flex-1" />

      {/* Settings */}
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={onOpenSettings}
              aria-label="Settings"
            />
          }
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-muted-foreground">
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
            <circle cx="12" cy="12" r="3" />
          </svg>
        </TooltipTrigger>
        <TooltipContent side="right">Settings</TooltipContent>
      </Tooltip>
    </aside>
  );
}
