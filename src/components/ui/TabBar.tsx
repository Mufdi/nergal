import { useState, useRef, useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  activeTabsAtom,
  activeTabAtom,
  activeTabIdAtom,
  closeTabAction,
  reorderTabsAction,
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
  MoreHorizontal,
  AlertTriangle,
  GitPullRequest,
  X,
} from "lucide-react";

const TAB_ICONS: Record<TabType, typeof FileText> = {
  plan: FileText,
  file: FileCode,
  diff: GitCompareArrows,
  spec: ClipboardList,
  tasks: CheckSquare,
  git: GitBranch,
  transcript: ScrollText,
  conflicts: AlertTriangle,
  pr: GitPullRequest,
};

export function TabBar() {
  const rawTabs = useAtomValue(activeTabsAtom);
  // Conflicts tab is always pinned at leftmost position for discoverability.
  const tabs = [
    ...rawTabs.filter((t) => t.type === "conflicts"),
    ...rawTabs.filter((t) => t.type !== "conflicts"),
  ];
  const activeTab = useAtomValue(activeTabAtom);
  const setActiveTabId = useSetAtom(activeTabIdAtom);
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

  // Scroll active tab into view when it changes
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

  if (tabs.length === 0) return null;

  return (
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
            dragOver={dragOverId === tab.id ? dropSideRef.current : null}
            onClick={() => setActiveTabId(tab.id)}
            onClose={() => closeTab(tab.id)}
            onDragStart={(e) => handleDragStart(tab.id, e)}
            onDragEnd={handleDragEnd}
            onDragOver={(e) => handleDragOver(tab.id, e)}
            onDrop={(e) => handleDrop(tab.id, e)}
          />
        ))}
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
  );
}

function TabItem({
  tab,
  isActive,
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
      title={tooltipText}
    >
      <Icon size={12} className="shrink-0" />
      <span className="truncate">{tab.label}</span>
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
    </div>
  );
}
