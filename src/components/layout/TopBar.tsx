import { useAtomValue } from "jotai";
import { activeSessionAtom, activeWorkspaceAtom } from "@/stores/workspace";
import { openTabsAtom, activeTabIdAtom } from "@/stores/rightPanel";

interface TopBarProps {
  onOpenSettings: () => void;
}

export function TopBar({ onOpenSettings }: TopBarProps) {
  const session = useAtomValue(activeSessionAtom);
  const workspace = useAtomValue(activeWorkspaceAtom);
  const openTabs = useAtomValue(openTabsAtom);
  const activeTabId = useAtomValue(activeTabIdAtom);

  const workspaceName = workspace?.name ?? "cluihud";

  return (
    <div className="flex h-10 shrink-0 items-center border-b border-border/30 bg-background px-3">
      {/* Left: settings + workspace name */}
      <div className="flex items-center gap-2 min-w-0 shrink-0">
        <button
          onClick={onOpenSettings}
          className="flex size-7 items-center justify-center rounded text-muted-foreground hover:bg-accent/20 hover:text-foreground transition-colors"
          aria-label="Settings"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
            <circle cx="12" cy="12" r="3" />
          </svg>
        </button>
        <span className="text-sm font-semibold text-foreground truncate max-w-32">
          {workspaceName}
        </span>
        {session && (
          <span className="text-xs text-muted-foreground truncate max-w-40">
            / {session.name}
          </span>
        )}
      </div>

      {/* Center: dynamic tabs (no close button — use panel collapse) */}
      <div className="flex flex-1 items-center justify-center gap-0.5 mx-4 overflow-x-auto scrollbar-none">
        {openTabs.map((tab) => (
          <div
            key={tab.id}
            className={`flex items-center gap-1 h-7 px-2.5 rounded text-xs font-medium cursor-pointer transition-colors ${
              activeTabId === tab.id
                ? "bg-secondary text-foreground"
                : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
            }`}
          >
            <span className="truncate max-w-24">{tab.label}</span>
          </div>
        ))}
      </div>

      {/* Right: spacer */}
      <div className="shrink-0" />
    </div>
  );
}
