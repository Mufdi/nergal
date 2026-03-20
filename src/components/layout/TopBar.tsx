import { useState, useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionAtom, activeSessionIdAtom, activeWorkspaceAtom } from "@/stores/workspace";
import {
  activeTabAtom,
  activeTabsAtom,
  activeTabIdAtom,
  activePanelViewAtom,
  expandRightPanelAtom,
  currentSpecArtifactAtom,
  type TabType,
} from "@/stores/rightPanel";
import { toggleRightPanelAtom } from "@/stores/shortcuts";
import { configAtom } from "@/stores/config";
import { appStore } from "@/stores/jotaiStore";
import { invoke } from "@/lib/tauri";
import {
  FileText,
  Files,
  GitCompareArrows,
  ClipboardList,
  CheckSquare,
  GitBranch,
  ChevronDown,
  ExternalLink,
} from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

// Editor logos
import zedLogo from "@/assets/editors/zed.svg";
import vscodeLogo from "@/assets/editors/vscode.svg";
import cursorLogo from "@/assets/editors/cursor.svg";
import webstormLogo from "@/assets/editors/webstorm.svg";
import phpstormLogo from "@/assets/editors/phpstorm.svg";
import pycharmLogo from "@/assets/editors/pycharm.svg";
import neovimLogo from "@/assets/editors/neovim.svg";
import vimLogo from "@/assets/editors/vim.svg";
import sublimeLogo from "@/assets/editors/sublime.png";
import windsurfLogo from "@/assets/editors/windsurf.svg";
import antigravityLogo from "@/assets/editors/antigravity.png";

const EDITOR_LOGO_MAP: Record<string, string> = {
  zed: zedLogo,
  code: vscodeLogo,
  cursor: cursorLogo,
  webstorm: webstormLogo,
  phpstorm: phpstormLogo,
  pycharm: pycharmLogo,
  nvim: neovimLogo,
  vim: vimLogo,
  subl: sublimeLogo,
  windsurf: windsurfLogo,
  antigravity: antigravityLogo,
};

interface TopBarProps {
  onOpenSettings: () => void;
  rightPanelVisible?: boolean;
}

interface EditorInfo {
  id: string;
  name: string;
  command: string;
  available: boolean;
}

function EditorIcon({ editorId, size = 14 }: { editorId: string; size?: number }) {
  const logo = EDITOR_LOGO_MAP[editorId];
  if (logo) {
    return <img src={logo} alt="" width={size} height={size} className="shrink-0" />;
  }
  return <ExternalLink size={size} className="shrink-0" />;
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
  const sessionId = useAtomValue(activeSessionIdAtom);
  const workspace = useAtomValue(activeWorkspaceAtom);
  const activeTab = useAtomValue(activeTabAtom);
  const tabs = useAtomValue(activeTabsAtom);
  const activePanelView = useAtomValue(activePanelViewAtom);
  const config = useAtomValue(configAtom);
  const setActiveTabId = useSetAtom(activeTabIdAtom);
  const setActivePanelView = useSetAtom(activePanelViewAtom);
  const setExpand = useSetAtom(expandRightPanelAtom);
  const setToggleRight = useSetAtom(toggleRightPanelAtom);

  const [editors, setEditors] = useState<EditorInfo[]>([]);
  const [dropdownOpen, setDropdownOpen] = useState(false);


  useEffect(() => {
    invoke<EditorInfo[]>("detect_editors").then(setEditors).catch(() => {});
  }, []);

  const available = editors.filter((e) => e.available);
  const preferred = config.preferred_editor
    ? available.find((e) => e.id === config.preferred_editor)
    : available[0];

  const workspaceName = workspace?.name ?? "cluihud";

  function handleOpenPanel(type: TabType) {
    if (activeTab?.type === type && rightPanelVisible) {
      setToggleRight((n) => n + 1);
      return;
    }
    if (activePanelView === type && !activeTab && rightPanelVisible) {
      setToggleRight((n) => n + 1);
      return;
    }

    const lastOfType = [...tabs].reverse().find((t) => t.type === type);
    if (lastOfType) {
      setActiveTabId(lastOfType.id);
    } else {
      setActiveTabId(null);
    }
    setActivePanelView(type);
    setExpand((n) => n + 1);
  }

  function openEditor(editorId: string) {
    if (!sessionId) return;

    // Read fresh from store at click time to avoid stale closures
    const tab = appStore.get(activeTabAtom);

    let filePath: string | null = null;
    let specChangeName: string | null = null;
    let specArtifactPath: string | null = null;

    if (tab?.data) {
      if (tab.type === "diff" || tab.type === "file" || tab.type === "plan") {
        filePath = (tab.data.path as string) ?? null;
      } else if (tab.type === "spec") {
        // Read live artifact from the global atom (tracks internal navigation)
        const specCtx = appStore.get(currentSpecArtifactAtom);
        if (specCtx) {
          specChangeName = specCtx.changeName;
          specArtifactPath = specCtx.artifactPath;
        }
      }
    }

    invoke("open_in_editor", { sessionId, editorId, filePath, specChangeName, specArtifactPath }).catch(() => {});
    setDropdownOpen(false);
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

      {/* Right: editor button + panel buttons */}
      <div className="flex items-center gap-0.5 shrink-0">
        {/* Editor: logo + "Open in" + chevron */}
        {preferred && sessionId && (
          <div className="relative flex items-center mr-2">
            <button
              onClick={() => openEditor(preferred.id)}
              className="flex h-6 items-center gap-1.5 rounded-l-md border border-border/50 bg-secondary/50 px-2 text-[11px] text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
              aria-label={`Open in ${preferred.name}`}
            >
              <EditorIcon editorId={preferred.id} size={14} />
              <span>Open in</span>
            </button>
            {available.length > 1 ? (
              <button
                onClick={() => setDropdownOpen(!dropdownOpen)}
                className="flex h-6 items-center rounded-r-md border border-l-0 border-border/50 bg-secondary/50 px-1 text-muted-foreground/60 hover:text-foreground hover:bg-secondary transition-colors"
                aria-label="Choose editor"
              >
                <ChevronDown size={10} />
              </button>
            ) : (
              <div className="flex h-6 items-center rounded-r-md border border-l-0 border-border/50 bg-secondary/50 px-1" />
            )}
            {dropdownOpen && (
              <>
                <div className="fixed inset-0 z-40" onClick={() => setDropdownOpen(false)} />
                <div className="absolute right-0 top-full z-50 mt-1 w-44 rounded-md border border-border bg-card py-1 shadow-lg">
                  {available.map((editor) => (
                    <button
                      key={editor.id}
                      onClick={() => openEditor(editor.id)}
                      className={`flex w-full items-center gap-2 px-3 py-1.5 text-[11px] transition-colors ${
                        editor.id === preferred.id
                          ? "text-foreground bg-secondary/50"
                          : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
                      }`}
                    >
                      <EditorIcon editorId={editor.id} size={14} />
                      {editor.name}
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>
        )}

        {/* Panel buttons */}
        {PANEL_BUTTONS.map((btn) => {
          const isActive = rightPanelVisible && (activePanelView === btn.type || activeTab?.type === btn.type);
          const Icon = btn.icon;
          return (
            <Tooltip key={btn.type}>
              <TooltipTrigger
                render={
                  <div
                    role="button"
                    onClick={() => handleOpenPanel(btn.type)}
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
