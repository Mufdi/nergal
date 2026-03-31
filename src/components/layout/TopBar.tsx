import { useState, useEffect, useRef, useCallback } from "react";
import { useAtomValue, useSetAtom, useAtom } from "jotai";
import {
  activeSessionIdAtom,
  activeWorkspaceAtom,
  workspacesAtom,
  sessionTabIdsAtom,
  modeMapAtom,
} from "@/stores/workspace";
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
import * as terminalService from "@/components/terminal/terminalService";
import {
  FileText,
  Files,
  GitCompareArrows,
  ClipboardList,
  CheckSquare,
  GitBranch,
  ChevronDown,
  ExternalLink,
  ChevronDown as MinimizeIcon,
  Maximize2,
  Minimize2,
  X,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
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

const MODE_COLORS: Record<string, string> = {
  idle: "bg-muted-foreground/50",
  thinking: "bg-yellow-500",
  tool: "bg-blue-500",
  responding: "bg-green-500",
};

export function TopBar({ onOpenSettings, rightPanelVisible = true }: TopBarProps) {
  const sessionId = useAtomValue(activeSessionIdAtom);
  const workspace = useAtomValue(activeWorkspaceAtom);
  const workspaces = useAtomValue(workspacesAtom);
  const activeTab = useAtomValue(activeTabAtom);
  const tabs = useAtomValue(activeTabsAtom);
  const activePanelView = useAtomValue(activePanelViewAtom);
  const config = useAtomValue(configAtom);
  const modeMap = useAtomValue(modeMapAtom);
  const setActiveSessionId = useSetAtom(activeSessionIdAtom);
  const [sessionTabIds, setSessionTabIds] = useAtom(sessionTabIdsAtom);
  const setActiveTabId = useSetAtom(activeTabIdAtom);
  const setActivePanelView = useSetAtom(activePanelViewAtom);
  const setExpand = useSetAtom(expandRightPanelAtom);
  const setToggleRight = useSetAtom(toggleRightPanelAtom);

  const [editors, setEditors] = useState<EditorInfo[]>([]);
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [isMaximized, setIsMaximized] = useState(false);
  const tabsContainerRef = useRef<HTMLDivElement>(null);
  const dragIdRef = useRef<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const dropSideRef = useRef<"left" | "right">("left");

  // Auto-add session to tabs when activated
  useEffect(() => {
    if (!sessionId) return;
    setSessionTabIds((prev) => {
      if (prev.includes(sessionId)) return prev;
      return [...prev, sessionId];
    });
  }, [sessionId, setSessionTabIds]);

  useEffect(() => {
    invoke<EditorInfo[]>("detect_editors").then(setEditors).catch(() => {});
    const win = getCurrentWindow();
    win.isMaximized().then(setIsMaximized).catch(() => {});
    const unlisten = win.onResized(() => {
      win.isMaximized().then(setIsMaximized).catch(() => {});
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const available = editors.filter((e) => e.available);
  const preferred = config.preferred_editor
    ? available.find((e) => e.id === config.preferred_editor)
    : available[0];

  const workspaceName = workspace?.name ?? "cluihud";

  // Resolve session objects for open tabs
  const sessionMap = new Map<string, { session: import("@/stores/workspace").Session; workspaceName: string }>();
  for (const ws of workspaces) {
    for (const s of ws.sessions) {
      sessionMap.set(s.id, { session: s, workspaceName: ws.name });
    }
  }

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
    const tab = appStore.get(activeTabAtom);

    let filePath: string | null = null;
    let specChangeName: string | null = null;
    let specArtifactPath: string | null = null;

    if (tab?.data) {
      if (tab.type === "diff" || tab.type === "file" || tab.type === "plan") {
        filePath = (tab.data.path as string) ?? null;
      } else if (tab.type === "spec") {
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

  function handleSelectTab(id: string) {
    setActiveSessionId(id);
  }

  function handleCloseTab(id: string, e: React.MouseEvent) {
    e.stopPropagation();
    const remaining = sessionTabIds.filter((tid) => tid !== id);
    setSessionTabIds(remaining);
    terminalService.destroy(id);

    if (sessionId === id) {
      setActiveSessionId(remaining.length > 0 ? remaining[remaining.length - 1] : null);
    }
  }

  function handleMiddleClick(id: string, e: React.MouseEvent) {
    if (e.button === 1) handleCloseTab(id, e);
  }

  const handleDragStart = useCallback((id: string, e: React.DragEvent) => {
    dragIdRef.current = id;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", id);
    (e.currentTarget as HTMLElement).style.opacity = "0.5";
  }, []);

  const handleDragEnd = useCallback((e: React.DragEvent) => {
    dragIdRef.current = null;
    setDragOverId(null);
    (e.currentTarget as HTMLElement).style.opacity = "1";
  }, []);

  const handleDragOver = useCallback((id: string, e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    if (dragIdRef.current && dragIdRef.current !== id) {
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      const midX = rect.left + rect.width / 2;
      dropSideRef.current = e.clientX < midX ? "left" : "right";
      setDragOverId(id);
    }
  }, []);

  const handleDrop = useCallback((targetId: string, e: React.DragEvent) => {
    e.preventDefault();
    const sourceId = dragIdRef.current;
    if (!sourceId || sourceId === targetId) return;
    setSessionTabIds((prev) => {
      const next = prev.filter((id) => id !== sourceId);
      const targetIdx = next.indexOf(targetId);
      const insertIdx = dropSideRef.current === "right" ? targetIdx + 1 : targetIdx;
      next.splice(insertIdx, 0, sourceId);
      return next;
    });
    setDragOverId(null);
    dragIdRef.current = null;
  }, [setSessionTabIds]);

  return (
    <div className="flex h-8 shrink-0 items-center bg-background px-2">
      {/* Left: settings + workspace */}
      <div className="flex items-center gap-1.5 shrink-0">
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
        <span className="text-[11px] font-medium text-muted-foreground truncate max-w-28">
          {workspaceName}
        </span>
      </div>

      {/* Separator */}
      <div className="mx-2 h-4 w-px bg-border/40" />

      {/* Session tabs */}
      <div
        ref={tabsContainerRef}
        className="flex flex-1 items-center gap-0.5 overflow-x-auto scrollbar-none"
        data-tauri-drag-region
      >
        {sessionTabIds.map((tabId) => {
          const entry = sessionMap.get(tabId);
          if (!entry) return null;
          const isActive = tabId === sessionId;
          const mode = modeMap[tabId] ?? "idle";
          const modeColor = MODE_COLORS[mode] ?? MODE_COLORS.idle;
          const isRunning = mode !== "idle";

          return (
            <button
              key={tabId}
              draggable
              onClick={() => handleSelectTab(tabId)}
              onMouseDown={(e) => handleMiddleClick(tabId, e)}
              onDragStart={(e) => handleDragStart(tabId, e)}
              onDragEnd={handleDragEnd}
              onDragOver={(e) => handleDragOver(tabId, e)}
              onDrop={(e) => handleDrop(tabId, e)}
              className={`group relative flex h-6 max-w-44 items-center gap-1.5 rounded px-2 text-[11px] transition-all ${
                isActive
                  ? "bg-card text-foreground"
                  : "text-muted-foreground hover:bg-card/50 hover:text-foreground/80"
              } ${dragOverId === tabId && dropSideRef.current === "left" ? "border-l-2 border-l-primary/60" : ""} ${dragOverId === tabId && dropSideRef.current === "right" ? "border-r-2 border-r-primary/60" : ""}`}
            >
              {/* Status indicator */}
              <span
                className={`size-1.5 shrink-0 rounded-full ${modeColor} ${isRunning ? "animate-dot-pulse" : ""}`}
              />

              {/* Session name */}
              <span className="truncate">{entry.session.name}</span>

              {/* Close button */}
              <span
                role="button"
                tabIndex={-1}
                onClick={(e) => handleCloseTab(tabId, e)}
                className={`ml-0.5 flex size-4 shrink-0 items-center justify-center rounded-sm transition-colors ${
                  isActive
                    ? "text-muted-foreground/60 hover:text-foreground hover:bg-foreground/10"
                    : "opacity-0 group-hover:opacity-100 text-muted-foreground/40 hover:text-foreground hover:bg-foreground/10"
                }`}
                aria-label="Close tab"
              >
                <X size={10} />
              </span>

              {/* Active tab bottom accent */}
              {isActive && (
                <span className="absolute bottom-0 left-2 right-2 h-px bg-primary/60 rounded-full" />
              )}
            </button>
          );
        })}

        {/* Remaining space is draggable */}
        <div className="flex-1 h-full min-w-8" data-tauri-drag-region />
      </div>

      {/* Right: editor button + panel buttons + window controls */}
      <div className="flex items-center gap-0.5 shrink-0">
        {/* Editor: logo + "Open in" + chevron */}
        {preferred && sessionId && (
          <div className="relative flex items-center mr-1.5">
            <button
              onClick={() => openEditor(preferred.id)}
              className="flex h-6 items-center gap-1.5 rounded-l border border-border/40 bg-card/40 px-2 text-[11px] text-muted-foreground hover:text-foreground hover:bg-card/60 transition-colors"
              aria-label={`Open in ${preferred.name}`}
            >
              <EditorIcon editorId={preferred.id} size={13} />
              <span>Open in</span>
            </button>
            {available.length > 1 ? (
              <button
                onClick={() => setDropdownOpen(!dropdownOpen)}
                className="flex h-6 items-center rounded-r border border-l-0 border-border/40 bg-card/40 px-1 text-muted-foreground/40 hover:text-foreground hover:bg-card/60 transition-colors"
                aria-label="Choose editor"
              >
                <ChevronDown size={10} />
              </button>
            ) : null}
            {dropdownOpen && (
              <>
                <div className="fixed inset-0 z-40" onClick={() => setDropdownOpen(false)} />
                <div className="absolute right-0 top-full z-50 mt-1 w-44 rounded border border-border bg-card py-1 shadow-lg">
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
                        ? "text-foreground bg-card"
                        : "text-muted-foreground hover:bg-card/50 hover:text-foreground"
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

        {/* Window controls */}
        <div className="flex items-center ml-1.5 pl-1.5 border-l border-border/30">
          <button
            onClick={() => getCurrentWindow().minimize()}
            className="flex size-7 items-center justify-center rounded text-amber-500/60 hover:bg-amber-500/10 hover:text-amber-400 transition-colors"
            aria-label="Minimize"
          >
            <MinimizeIcon size={13} />
          </button>
          <button
            onClick={() => getCurrentWindow().toggleMaximize()}
            className="flex size-7 items-center justify-center rounded text-green-500/60 hover:bg-green-500/10 hover:text-green-400 transition-colors"
            aria-label={isMaximized ? "Restore" : "Maximize"}
          >
            {isMaximized ? <Minimize2 size={13} /> : <Maximize2 size={13} />}
          </button>
          <button
            onClick={() => getCurrentWindow().close()}
            className="flex size-7 items-center justify-center rounded text-red-500/60 hover:bg-red-500/10 hover:text-red-400 transition-colors"
            aria-label="Close"
          >
            <X size={13} />
          </button>
        </div>
      </div>
    </div>
  );
}
