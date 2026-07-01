import { useState, useRef, useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  activeTabsAtom,
  activeTabAtom,
  activeTabIdAtom,
  activePanelViewAtom,
  closeTabAction,
  reorderTabsAction,
  viewPanelLabel,
  PANEL_CATEGORY_MAP,
  type Tab,
  type TabType,
} from "@/stores/rightPanel";
import {
  FileText,
  GitCompareArrows,
  ClipboardList,
  CheckSquare,
  GitBranch,
  FileCode,
  ScrollText,
  Globe,
  MessagesSquare,
  MoreHorizontal,
  Pin,
  X,
} from "lucide-react";
import { ObsidianIcon } from "@/components/icons/ObsidianIcon";
import { ClickUpIcon } from "@/components/icons/ClickUpIcon";
import { LinearIcon } from "@/components/icons/LinearIcon";
import { activeSessionPinnedNotesAtom } from "@/stores/pinnedNotes";
import { Tooltip, TooltipProvider, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";
import type { ComponentType } from "react";

const TAB_ICONS: Record<TabType, ComponentType<{ size?: number | string; className?: string }>> = {
  plan: FileText,
  file: FileCode,
  diff: GitCompareArrows,
  spec: ClipboardList,
  tasks: CheckSquare,
  git: GitBranch,
  transcript: ScrollText,
  browser: Globe,
  obsidiannote: ObsidianIcon,
  clickup: ClickUpIcon,
  "clickup-task": ClickUpIcon,
  linear: LinearIcon,
  "linear-issue": LinearIcon,
  crosssession: MessagesSquare,
};

export function TabBar() {
  const tabs = useAtomValue(activeTabsAtom);
  const activeTab = useAtomValue(activeTabAtom);
  const pinnedPaths = useAtomValue(activeSessionPinnedNotesAtom);
  const setActiveTabId = useSetAtom(activeTabIdAtom);
  // A "tool" view panel (ClickUp / git / diff / browser — the view IS the
  // content) shows as a virtual tab so Ctrl+Tab cycling lands somewhere
  // visible. "document" panels (spec / file / plan) are launcher lists, not a
  // cycle target, so they get no virtual tab. Active when activeTabId === null.
  const viewPanel = useAtomValue(activePanelViewAtom);
  const setActivePanelView = useSetAtom(activePanelViewAtom);
  const showVirtual = viewPanel !== null && PANEL_CATEGORY_MAP[viewPanel] === "tool";
  const ViewIcon = viewPanel ? TAB_ICONS[viewPanel] : null;

  function closeViewPanel() {
    // Fall back to the last document tab if the view was the active content;
    // otherwise just clear the standalone view.
    if (!activeTab && tabs.length > 0) setActiveTabId(tabs[tabs.length - 1].id);
    setActivePanelView(null);
  }

  const isContextPinned = (tab: Tab) =>
    tab.type === "obsidiannote" && pinnedPaths.includes(tab.data?.path as string);
  const closeTab = useSetAtom(closeTabAction);
  const reorderTabs = useSetAtom(reorderTabsAction);

  const containerRef = useRef<HTMLDivElement>(null);
  const [overflowing, setOverflowing] = useState(false);
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const dragIdRef = useRef<string | null>(null);
  const dropSideRef = useRef<"left" | "right">("left");
  const [dragOverId, setDragOverId] = useState<string | null>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const check = () => setOverflowing(el.scrollWidth > el.clientWidth);
    check();
    const observer = new ResizeObserver(check);
    observer.observe(el);
    return () => observer.disconnect();
  }, [tabs.length]);

  useEffect(() => {
    if (!activeTab || !containerRef.current) return;
    const el = containerRef.current.querySelector("[data-tab-active='true']") as HTMLElement | null;
    if (el) el.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "nearest" });
  }, [activeTab?.id]);

  function handleDragStart(id: string, e: React.DragEvent) {
    dragIdRef.current = id;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", id);
    (e.currentTarget as HTMLElement).style.opacity = "0.5";
  }

  function handleDragEnd(e: React.DragEvent) {
    dragIdRef.current = null;
    setDragOverId(null);
    (e.currentTarget as HTMLElement).style.opacity = "1";
  }

  function handleDragOver(id: string, e: React.DragEvent) {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    if (dragIdRef.current && dragIdRef.current !== id) {
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      dropSideRef.current = e.clientX < rect.left + rect.width / 2 ? "left" : "right";
      setDragOverId(id);
    }
  }

  function handleDrop(targetId: string, e: React.DragEvent) {
    e.preventDefault();
    const sourceId = dragIdRef.current;
    if (!sourceId || sourceId === targetId) return;
    reorderTabs({ sourceId, targetId, side: dropSideRef.current });
    setDragOverId(null);
    dragIdRef.current = null;
  }

  if (tabs.length === 0 && !showVirtual) return null;

  return (
    <TooltipProvider delay={0}>
    <div className="flex shrink-0 items-center">
      <div
        ref={containerRef}
        className="flex flex-1 items-center overflow-x-auto scrollbar-none"
      >
        {tabs.map((tab) => (
          <TabItem
            key={tab.id}
            tab={tab}
            isActive={activeTab?.id === tab.id}
            contextPinned={isContextPinned(tab)}
            dragOver={dragOverId === tab.id ? dropSideRef.current : null}
            onClick={() => setActiveTabId(tab.id)}
            onClose={() => closeTab(tab.id)}
            onDragStart={(e) => handleDragStart(tab.id, e)}
            onDragEnd={handleDragEnd}
            onDragOver={(e) => handleDragOver(tab.id, e)}
            onDrop={(e) => handleDrop(tab.id, e)}
          />
        ))}
        {viewPanel && showVirtual && ViewIcon && (
          // Virtual tab for a "tool" view panel. Active when no document tab is
          // selected; click to show it, close (X / middle-click) clears the view.
          <Tooltip>
            <TooltipTrigger
              render={
                <div
                  data-tab-active={!activeTab}
                  onClick={() => setActiveTabId(null)}
                  onMouseDown={(e) => { if (e.button === 1) { e.preventDefault(); closeViewPanel(); } }}
                  className={`group flex min-w-20 max-w-40 shrink-0 cursor-pointer items-center gap-1.5 px-2.5 py-1.5 text-xs transition-colors ${
                    !activeTab ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground/80"
                  }`}
                />
              }
            >
              <ViewIcon size={12} className="shrink-0" />
              <span className="truncate">{viewPanelLabel(viewPanel)}</span>
              <button
                onClick={(e) => { e.stopPropagation(); closeViewPanel(); }}
                className="ml-auto flex size-4 shrink-0 items-center justify-center rounded opacity-0 transition-opacity group-hover:opacity-100 hover:bg-secondary"
                aria-label={`Close ${viewPanelLabel(viewPanel)}`}
              >
                <X size={10} />
              </button>
            </TooltipTrigger>
            <TooltipContent side="bottom" className="text-[10px]">{viewPanelLabel(viewPanel)}</TooltipContent>
          </Tooltip>
        )}
      </div>

      {overflowing && (
        <div className="relative shrink-0">
          <button
            onClick={() => setDropdownOpen((v) => !v)}
            className="flex size-7 items-center justify-center text-muted-foreground hover:text-foreground transition-colors"
            aria-label="Show all tabs"
          >
            <MoreHorizontal size={14} />
          </button>
          {dropdownOpen && (
            <>
              <div
                className="fixed inset-0 z-40"
                onClick={() => setDropdownOpen(false)}
              />
              <div className="absolute right-0 top-full z-50 mt-1 w-48 rounded-md border border-border bg-card py-1 shadow-lg">
                {tabs.map((tab) => {
                  const Icon = TAB_ICONS[tab.type];
                  return (
                    <button
                      key={tab.id}
                      onClick={() => {
                        setActiveTabId(tab.id);
                        setDropdownOpen(false);
                      }}
                      className={`flex w-full items-center gap-2 px-3 py-1.5 text-xs transition-colors ${
                        activeTab?.id === tab.id
                          ? "bg-secondary text-foreground"
                          : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
                      }`}
                    >
                      <Icon size={12} />
                      <span className="truncate">{tab.label}</span>
                    </button>
                  );
                })}
              </div>
            </>
          )}
        </div>
      )}
    </div>
    </TooltipProvider>
  );
}

function TabItem({
  tab,
  isActive,
  contextPinned,
  dragOver,
  onClick,
  onClose,
  onDragStart,
  onDragEnd,
  onDragOver,
  onDrop,
}: {
  tab: Tab;
  isActive: boolean;
  contextPinned: boolean;
  dragOver: "left" | "right" | null;
  onClick: () => void;
  onClose: () => void;
  onDragStart: (e: React.DragEvent) => void;
  onDragEnd: (e: React.DragEvent) => void;
  onDragOver: (e: React.DragEvent) => void;
  onDrop: (e: React.DragEvent) => void;
}) {
  const Icon = TAB_ICONS[tab.type];
  const tooltipText = (tab.type === "diff" || tab.type === "file")
    ? (tab.data?.path as string | undefined) ?? tab.label
    : tab.label;

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <div
            draggable
            data-tab-active={isActive}
            onClick={onClick}
            onMouseDown={(e) => { if (e.button === 1) { e.preventDefault(); onClose(); } }}
            onDragStart={onDragStart}
            onDragEnd={onDragEnd}
            onDragOver={onDragOver}
            onDrop={onDrop}
            className={`group flex min-w-20 max-w-40 shrink-0 cursor-pointer items-center gap-1.5 px-2.5 py-1.5 text-xs transition-colors ${
              isActive
                ? "bg-secondary text-foreground"
                : "text-muted-foreground hover:text-foreground/80"
            } ${dragOver === "left" ? "border-l-2 border-l-primary/60" : ""} ${dragOver === "right" ? "border-r-2 border-r-primary/60" : ""}`}
          />
        }
      >
        <Icon size={12} className="shrink-0" />
        <span className="truncate">{tab.label}</span>
        {contextPinned && (
          <Pin size={10} className="shrink-0 text-primary" aria-label="Pinned as context" />
        )}
        <div className="ml-auto shrink-0">
          {tab.dirty ? (
            <span className="block size-1 rounded-full bg-foreground" />
          ) : (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onClose();
              }}
              className="flex size-4 items-center justify-center rounded opacity-0 transition-opacity group-hover:opacity-100 hover:bg-secondary"
              aria-label={`Close ${tab.label}`}
            >
              <X size={10} />
            </button>
          )}
        </div>
      </TooltipTrigger>
      <TooltipContent side="bottom" className="text-[10px]">{tooltipText}</TooltipContent>
    </Tooltip>
  );
}
