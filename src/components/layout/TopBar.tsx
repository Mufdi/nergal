import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionAtom, activeWorkspaceAtom } from "@/stores/workspace";
import {
  activeTabsAtom,
  activeTabAtom,
  activeTabIdAtom,
  openTabAction,
  expandRightPanelAtom,
  type TabType,
} from "@/stores/rightPanel";
import { toggleRightPanelAtom } from "@/stores/shortcuts";
import {
  FileText,
  Files,
  GitCompareArrows,
  ClipboardList,
  CheckSquare,
  GitBranch,
} from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

interface TopBarProps {
  onOpenSettings: () => void;
  rightPanelVisible?: boolean;
}

const PANEL_BUTTONS: { type: TabType; label: string; shortcut: string; icon: typeof FileText }[] = [
  { type: "plan", label: "Plan", shortcut: "Ctrl+Shift+P", icon: FileText },
  { type: "file", label: "Files", shortcut: "Ctrl+Shift+F", icon: Files },
  { type: "diff", label: "Diff", shortcut: "Ctrl+Shift+D", icon: GitCompareArrows },
  { type: "spec", label: "Spec", shortcut: "Ctrl+Shift+S", icon: ClipboardList },
  { type: "tasks", label: "Tasks", shortcut: "Ctrl+Shift+K", icon: CheckSquare },
  { type: "git", label: "Git", shortcut: "Ctrl+Shift+G", icon: GitBranch },
];

export function TopBar({ onOpenSettings, rightPanelVisible = true }: TopBarProps) {
  const session = useAtomValue(activeSessionAtom);
  const workspace = useAtomValue(activeWorkspaceAtom);
  const tabs = useAtomValue(activeTabsAtom);
  const activeTab = useAtomValue(activeTabAtom);
  const setActiveTabId = useSetAtom(activeTabIdAtom);
  const openTab = useSetAtom(openTabAction);
  const setExpand = useSetAtom(expandRightPanelAtom);
  const setToggleRight = useSetAtom(toggleRightPanelAtom);

  const workspaceName = workspace?.name ?? "cluihud";

  function handleOpenPanel(type: TabType, label: string) {
    const existing = tabs.find((t) => t.type === type);
    if (existing && activeTab?.id === existing.id) {
      setToggleRight((n) => n + 1);
      return;
    }
    if (existing) {
      setActiveTabId(existing.id);
    } else {
      openTab({ tab: { id: `${type}-panel`, type, label }, isPinned: true });
    }
    setExpand((n) => n + 1);
  }

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

      {/* Center spacer */}
      <div className="flex-1" />

      {/* Right: panel type icon buttons */}
      <div className="flex items-center gap-0.5 shrink-0">
        {PANEL_BUTTONS.map((btn) => {
          const isActive = rightPanelVisible && activeTab?.type === btn.type;
          const Icon = btn.icon;
          return (
            <Tooltip key={btn.type}>
              <TooltipTrigger
                render={
                  <div
                    role="button"
                    onClick={() => handleOpenPanel(btn.type, btn.label)}
                    className={`flex size-7 items-center justify-center rounded transition-colors cursor-pointer ${
                      isActive
                        ? "text-foreground bg-secondary"
                        : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                    }`}
                    aria-label={btn.label}
                  />
                }
              >
                <Icon size={14} />
              </TooltipTrigger>
              <TooltipContent side="bottom">
                {btn.label} ({btn.shortcut})
              </TooltipContent>
            </Tooltip>
          );
        })}
      </div>
    </div>
  );
}
